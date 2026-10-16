"""Isolated pactl peer for shell controls; never connects to an audio server."""

import contextlib
import fcntl
import json
import os
import sys
from pathlib import Path

from .control import ControlError

STATE_ENV = "HEARTHSPACE_CONTROL_FIXTURE_STATE"


@contextlib.contextmanager
def locked(path):
    with path.with_suffix(".lock").open("a+") as guard:
        fcntl.flock(guard, fcntl.LOCK_EX)
        state = json.loads(path.read_text())
        yield state
        path.write_text(json.dumps(state) + "\n")


class Fixture:
    def __init__(self, directory, evidence):
        self.path = directory / "audio-fixture.json"
        self.events = evidence / "audio-requests.jsonl"
        self.path.write_text(
            json.dumps(
                {
                    "present": True,
                    "service": True,
                    "default_sink": "fixture.output",
                    "description": "Fixture speakers",
                    "volume": 40,
                    "raw_volumes": [13107, 26214],
                    "muted": False,
                    "events": str(self.events),
                }
            )
        )
        self.path.chmod(0o600)

    def update(self, **changes):
        if not set(changes) <= {
            "present",
            "service",
            "default_sink",
            "description",
            "volume",
            "muted",
        }:
            raise ValueError("unsupported audio fixture state")
        with locked(self.path) as state:
            state.update(changes)
            if "volume" in changes:
                maximum = round(changes["volume"] * 65536 / 100)
                state["raw_volumes"] = [round(maximum / 2), maximum]

    def status(self):
        with locked(self.path) as state:
            return state.copy()

    def mutations(self):
        with locked(self.path):
            if not self.events.exists():
                return []
            return [
                event
                for line in self.events.read_text().splitlines()
                if (event := json.loads(line))["argv"][0] == "--"
            ]


@contextlib.contextmanager
def services(directory, evidence, env, _deadline):
    expected = f"unix:path={directory / 'bus'}"
    if any(
        env.get(key) != expected
        for key in ("DBUS_SYSTEM_BUS_ADDRESS", "DBUS_SESSION_BUS_ADDRESS")
    ):
        raise ControlError("control acceptance requires matching owned private buses")
    fixture = Fixture(directory, evidence)
    programs = directory / "control-fixture-bin"
    programs.mkdir(mode=0o700)
    script = programs / "pactl"
    scripts = Path(__file__).resolve().parents[1]
    script.write_text(
        "#!/usr/bin/python3\nimport sys\n"
        f"sys.path.insert(0, {str(scripts)!r})\n"
        "from agentlib.control_ui_fixture import pactl_main\n"
        "sys.exit(pactl_main())\n"
    )
    script.chmod(0o700)
    config = directory / "pulse-client.conf"
    config.write_text("autospawn = no\nenable-shm = no\n")
    env.update(
        {
            "PATH": f"{programs}:{env['PATH']}",
            STATE_ENV: str(fixture.path),
            # No server listens here. Even an accidentally invoked real pactl
            # cannot fall back to the host's audio service or autospawn one.
            "PULSE_SERVER": f"unix:{directory / 'audio-fixture.sock'}",
            "PULSE_CLIENTCONFIG": str(config),
            "PULSE_COOKIE": str(directory / "no-host-audio-cookie"),
        }
    )
    yield fixture


def respond(state, arguments):
    if not state["service"]:
        return 1, "", "Connection failure: Connection refused"
    if arguments == ["--format=json", "info"]:
        return 0, json.dumps({"default_sink_name": state["default_sink"]}), ""
    if arguments == ["--format=json", "list", "sinks"]:
        sinks = []
        if state["present"]:
            sinks.append(
                {
                    "name": state["default_sink"],
                    "description": state["description"],
                    "mute": state["muted"],
                    "channel_map": "front-left,front-right",
                    "volume": {
                        name: {"value": value}
                        for name, value in zip(
                            ["front-left", "front-right"],
                            state["raw_volumes"],
                            strict=True,
                        )
                    },
                }
            )
        return 0, json.dumps(sinks), ""
    if len(arguments) < 4 or arguments[0] != "--":
        return 2, "", "Unsupported private fixture request"
    _, command, device, *values = arguments
    if not state["present"] or device != state["default_sink"]:
        return 1, "", "No such entity"
    if command == "set-sink-volume":
        try:
            if len(values) == 1 and values[0].endswith("%"):
                percentage = int(values[0][:-1])
                if not 0 <= percentage <= 100:
                    raise ValueError("Invalid percentage")
                raw = [round(percentage * 65536 / 100)] * 2
            else:
                raw = [int(value) for value in values]
                if len(raw) != 2 or any(not 0 <= value <= 65536 for value in raw):
                    raise ValueError("Invalid channel volumes")
        except ValueError:
            return 2, "", "Invalid fixture volume"
        state["raw_volumes"] = raw
        state["volume"] = (max(raw) * 100 + 32768) // 65536
    elif (
        command == "set-sink-mute"
        and len(values) == 1
        and values[0] in ("toggle", "0", "1")
    ):
        value = values[0]
        state["muted"] = not state["muted"] if value == "toggle" else value == "1"
    else:
        return 2, "", "Unsupported private fixture mutation"
    return 0, "", ""


def pactl_main():
    path = Path(os.environ[STATE_ENV])
    expected = f"unix:{path.parent / 'audio-fixture.sock'}"
    if os.environ.get("PULSE_SERVER") != expected:
        print("Fixture refuses non-private audio routing", file=sys.stderr)
        return 2
    with locked(path) as state:
        result, output, error = respond(state, sys.argv[1:])
        with Path(state["events"]).open("a") as stream:
            stream.write(
                json.dumps({"argv": sys.argv[1:], "server": expected, "result": result})
                + "\n"
            )
    if output:
        print(output)
    if error:
        print(error, file=sys.stderr)
    return result
