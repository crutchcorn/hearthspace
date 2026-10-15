"""An independent app peer on the explicitly owned session bus."""

import contextlib
import json
import os
import subprocess
from pathlib import Path

from .control import ControlError
from .daily_checks import read_line


class Fixture:
    def __init__(self, smoke, generation):
        self.smoke = smoke
        self.socket = Path(smoke.manifest["runtime_dir"]) / "tray-fixture.sock"
        self.actions = smoke.artifacts / f"tray-actions-{generation}.jsonl"
        self.process = None

    def events(self):
        if not self.actions.exists():
            return []
        return [
            json.loads(line)
            for line in self.actions.read_text().splitlines(keepends=True)
            if line.endswith("\n")
        ]

    def command(self, command):
        result = json.loads(
            read_line(
                self.socket, (command + "\n").encode(), self.smoke.control.deadline
            )
        )
        if not result.get("ok"):
            raise ControlError(f"tray fixture command failed: {result}")

    def expect(self, action, *, after=0, **fields):
        def received():
            if self.process.poll() is not None:
                raise ControlError("tray fixture exited before receiving app action")
            return next(
                (
                    event
                    for event in reversed(self.events()[after:])
                    if event.get("action") == action
                    and all(event.get(key) == value for key, value in fields.items())
                ),
                None,
            )

        event = self.smoke.until(f"tray app receives {action} {fields}", received)
        self.smoke.record("tray-app-action", event=event)
        return event


@contextlib.contextmanager
def fixture(smoke, generation=1):
    peer = Fixture(smoke, generation)
    binary = Path(smoke.manifest["binary_dir"]) / "examples/tray_fixture"
    if not binary.is_file():
        raise ControlError(
            "build cargo build -p hearthspace-shell --example tray_fixture"
        )
    env = {
        **os.environ,
        "DBUS_SESSION_BUS_ADDRESS": smoke.manifest["dbus_session_bus_address"],
        "XDG_RUNTIME_DIR": smoke.manifest["runtime_dir"],
    }
    for key in ("DBUS_SESSION_BUS_PID", "DISPLAY", "AT_SPI_BUS_ADDRESS"):
        env.pop(key, None)
    with (smoke.artifacts / f"tray-fixture-{generation}.log").open("w") as log:
        peer.process = subprocess.Popen(
            [str(binary), str(peer.socket), str(peer.actions)],
            env=env,
            stdout=log,
            stderr=log,
        )
        try:

            def ready():
                if peer.process.poll() is not None:
                    log.flush()
                    raise ControlError(
                        "tray fixture exited during registration: "
                        + (
                            smoke.artifacts / f"tray-fixture-{generation}.log"
                        ).read_text()
                    )
                return peer.socket.exists()

            smoke.until("independent tray fixture registered", ready)
            yield peer
        finally:
            if peer.process.poll() is None:
                peer.process.terminate()
                try:
                    peer.process.wait(timeout=2)
                except subprocess.TimeoutExpired:
                    peer.process.kill()
                    peer.process.wait(timeout=2)
            peer.socket.unlink(missing_ok=True)
            smoke.record(
                "tray-fixture-stopped",
                generation=generation,
                pid=peer.process.pid,
                returncode=peer.process.returncode,
                actions=str(peer.actions),
            )
