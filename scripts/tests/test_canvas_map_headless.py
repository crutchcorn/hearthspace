"""Run the native map workflow headlessly with a private accessibility bus."""

import contextlib
import io
import json
import os
import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path
from types import SimpleNamespace
from xml.sax.saxutils import escape

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib.control import Control, ControlError, Deadline
from agentlib.map_smoke import run
from agentlib.smoke import Smoke
from test_clipboard_headless import private_bus_command, stop_process_group

ROOT = Path(__file__).resolve().parents[2]


def map_bus_command(directory, address):
    # Only accessibility may activate. Excluding desktop portals avoids any
    # host mounts or unrelated desktop services in this owned test session.
    command = private_bus_command(directory, address)
    services = directory / "services"
    services.mkdir()
    source = Path("/usr/share/dbus-1/services/org.a11y.Bus.service")
    service = source.read_text()
    (services / source.name).write_text(
        "\n".join(
            line
            for line in service.splitlines()
            if not line.startswith("SystemdService=")
        )
        + "\n"
    )
    config = directory / "bus.conf"
    config.write_text(
        config.read_text().replace(
            "</busconfig>",
            f"<servicedir>{escape(str(services))}</servicedir></busconfig>",
        )
    )
    return command


@unittest.skipUnless(os.environ.get("HEARTHSPACE_E2E") == "1", "set HEARTHSPACE_E2E=1")
class CanvasMapHeadlessTests(unittest.TestCase):
    def test_live_map_pointer_zoom_and_dismissal(self):
        with tempfile.TemporaryDirectory(prefix="hs-map-e2e-") as temporary:
            directory = Path(temporary)
            evidence = Path(os.environ.get("HEARTHSPACE_MAP_EVIDENCE", directory))
            evidence.mkdir(parents=True, exist_ok=True)
            self.run_session(directory, evidence)

    def run_session(self, directory, evidence):
        binary = Path(
            os.environ.get("HEARTHSPACE_BINARY", ROOT / "target/debug/hearthspace")
        )
        socket = directory / "control.sock"
        address = f"unix:path={directory / 'bus'}"
        env = {
            **os.environ,
            "XDG_RUNTIME_DIR": str(directory),
            "HEARTHSPACE_COMMAND_SOCKET": str(socket),
            "HEARTHSPACE_WAYLAND_DISPLAY": "wayland-map",
            "WAYLAND_DISPLAY": "wayland-map",
            "DBUS_SESSION_BUS_ADDRESS": address,
            "GSETTINGS_BACKEND": "memory",
            "XDG_CURRENT_DESKTOP": "Hearthspace",
        }
        for key in (
            "AT_SPI_BUS_ADDRESS",
            "DISPLAY",
            "DBUS_SESSION_BUS_PID",
            "GTK_A11Y",
        ):
            env.pop(key, None)
        (evidence / "session.json").write_text(
            json.dumps(
                {
                    "dbus_session_bus_address": address,
                    "command_socket": str(socket),
                    "binary_dir": str(binary.parent),
                }
            )
        )
        with contextlib.ExitStack() as cleanup:
            bus_log = cleanup.enter_context((evidence / "dbus.log").open("w"))
            bus = subprocess.Popen(
                map_bus_command(directory, address),
                env=env,
                stdout=bus_log,
                stderr=subprocess.STDOUT,
                start_new_session=True,
            )
            cleanup.callback(stop_process_group, bus)
            deadline = Deadline(120)
            while not (directory / "bus").exists():
                self.assertIsNone(bus.poll(), "private bus exited")
                time.sleep(min(0.02, deadline.remaining()))
            log = cleanup.enter_context((evidence / "compositor.log").open("w"))
            compositor = subprocess.Popen(
                [
                    str(binary),
                    "--headless",
                    "--no-shell",
                    "--headless-size",
                    "2560x1600",
                    "--headless-scale",
                    "2",
                ],
                env=env,
                stdout=log,
                stderr=log,
                start_new_session=True,
            )
            cleanup.callback(stop_process_group, compositor)
            smoke = Smoke(
                SimpleNamespace(session=evidence, timeout=120), presentation="offscreen"
            )
            smoke.control = Control(socket, deadline)
            smoke.marker = "HEARTHSPACE_MAP_FIXTURE"
            while not socket.exists():
                self.assertIsNone(compositor.poll(), "headless compositor exited")
                time.sleep(min(0.05, deadline.remaining()))
            shell_log = cleanup.enter_context((evidence / "shell.log").open("w"))
            shell = subprocess.Popen(
                [str(binary.with_name("hearthspace-shell"))],
                env=env,
                stdout=shell_log,
                stderr=shell_log,
                start_new_session=True,
            )
            cleanup.callback(stop_process_group, shell)
            while True:
                try:
                    if smoke.control.status().get("shell_ready"):
                        break
                except ControlError:
                    pass
                self.assertIsNone(compositor.poll(), "headless compositor exited")
                self.assertIsNone(shell.poll(), "headless shell exited")
                time.sleep(min(0.05, deadline.remaining()))
            try:
                with contextlib.redirect_stdout(io.StringIO()):
                    run(smoke)
            except (
                ControlError,
                OSError,
                ValueError,
                KeyError,
                subprocess.SubprocessError,
            ) as error:
                log.flush()
                self.fail(
                    f"{error}\n{(evidence / 'compositor.log').read_text()[-16000:]}"
                )


if __name__ == "__main__":
    unittest.main()
