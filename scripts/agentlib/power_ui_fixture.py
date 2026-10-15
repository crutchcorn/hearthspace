"""Private UPower and suspend peers; no host power API is ever invoked."""

import contextlib
import json
import socketserver
import subprocess
import threading
import time
from pathlib import Path

from .control import ControlError
from .daily_checks import read_line


class PowerSocket(socketserver.UnixStreamServer):
    """Record the real shell's requests to an owned mock coordinator."""

    def __init__(self, path, log):
        self.log = log
        self.events = []
        self.guard = threading.Lock()
        self.reply = {"ok": True, "available": True, "busy": False, "error": None}
        super().__init__(str(path), PowerRequest)
        Path(path).chmod(0o600)

    def respond(self, value):
        with self.guard:
            if set(value) != {"command"} or value["command"] not in (
                "status",
                "suspend",
            ):
                raise ValueError("unsupported fixture request")
            event = {"command": value["command"], "reply": self.reply.copy()}
            self.events.append(event)
            with self.log.open("a") as stream:
                stream.write(json.dumps(event) + "\n")
            return event["reply"]

    def suspend_events(self):
        with self.guard:
            return [item.copy() for item in self.events if item["command"] == "suspend"]

    def unavailable(self, reason):
        with self.guard:
            self.reply = {
                "ok": True,
                "available": False,
                "busy": False,
                "error": reason,
            }


class PowerRequest(socketserver.StreamRequestHandler):
    def handle(self):
        self.request.settimeout(2)
        try:
            line = self.rfile.readline(4097)
            if len(line) > 4096 or not line.endswith(b"\n"):
                raise ValueError("incomplete or oversized fixture request")
            value = json.loads(line)
            if not isinstance(value, dict):
                raise TypeError("expected request object")
            response = self.server.respond(value)
        except (ValueError, TypeError, OSError) as error:
            response = {
                "ok": False,
                "available": False,
                "busy": False,
                "error": str(error),
            }
        self.wfile.write(json.dumps(response).encode() + b"\n")


class Fixture:
    def __init__(self, directory, evidence, env, deadline):
        self.directory, self.evidence = directory, evidence
        self.env, self.deadline = env, deadline
        self.socket = directory / "upower-fixture.sock"
        self.state = {
            "present": True,
            "kind": 2,
            "percentage": 72.0,
            "state": 1,
            "warning": 1,
            "on_battery": False,
        }
        self.process = None
        self.power = None

    def update(self, **changes):
        if self.process.poll() is not None:
            raise ControlError("private UPower fixture exited")
        state = self.state | changes
        result = json.loads(
            read_line(self.socket, (json.dumps(state) + "\n").encode(), self.deadline)
        )
        if not result.get("ok"):
            raise ControlError(f"UPower fixture rejected state: {result}")
        self.state = state
        with (self.evidence / "battery-changes.jsonl").open("a") as stream:
            stream.write(json.dumps(state) + "\n")


@contextlib.contextmanager
def services(directory, evidence, env, deadline):
    """Start only under run_shell_session's already-created isolated bus."""
    binary = Path(env["PATH"].split(":", 1)[0]) / "examples/power_ui_fixture"
    if not binary.is_file():
        raise ControlError(
            "build cargo build -p hearthspace-shell --example power_ui_fixture"
        )
    expected = f"unix:path={directory / 'bus'}"
    if (
        env.get("DBUS_SYSTEM_BUS_ADDRESS") != expected
        or env.get("DBUS_SESSION_BUS_ADDRESS") != expected
    ):
        raise ControlError("power acceptance requires matching owned private buses")
    fixture = Fixture(directory, evidence, env, deadline)
    socket = directory / "power-control.sock"
    with contextlib.ExitStack() as cleanup:
        power = PowerSocket(socket, evidence / "power-requests.jsonl")
        fixture.power = power
        cleanup.callback(socket.unlink, missing_ok=True)
        cleanup.callback(power.server_close)
        thread = threading.Thread(target=power.serve_forever, args=(0.05,), daemon=True)
        thread.start()
        cleanup.callback(thread.join, 3)
        cleanup.callback(power.shutdown)
        log = cleanup.enter_context((evidence / "upower-fixture.log").open("w"))
        fixture.process = subprocess.Popen(
            [str(binary), str(fixture.socket)], env=env, stdout=log, stderr=log
        )
        cleanup.callback(stop, fixture.process)
        while not fixture.socket.exists():
            if fixture.process.poll() is not None:
                raise ControlError(
                    "private UPower fixture failed: "
                    + (evidence / "upower-fixture.log").read_text()
                )
            time.sleep(min(0.02, deadline.remaining()))
        yield fixture


def stop(process):
    if process.poll() is None:
        process.terminate()
        try:
            process.wait(timeout=2)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=2)
