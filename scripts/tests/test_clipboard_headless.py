"""Run the same cross-app clipboard acceptance workflow used on a native VT."""

import contextlib
import io
import os
import signal
import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path
from types import SimpleNamespace
from xml.sax.saxutils import escape

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib.clipboard import run_clipboard
from agentlib.control import Control, ControlError, Deadline
from agentlib.smoke import Smoke

ROOT = Path(__file__).resolve().parents[2]


def private_bus_command(directory, address):
    """Serve test clients without importing host D-Bus activation directories."""
    config = directory / "bus.conf"
    config.write_text(
        f"""<busconfig>
  <type>session</type>
  <listen>{escape(address)}</listen>
  <auth>EXTERNAL</auth>
  <policy context="default">
    <allow own="*"/>
    <allow send_destination="*"/>
    <allow receive_sender="*"/>
  </policy>
</busconfig>
""",
        encoding="utf-8",
    )
    return [
        "dbus-daemon",
        f"--config-file={config}",
        "--nofork",
    ]


def stop_process_group(process):
    """Include the compositor's Xwayland and private bus activation children."""
    with contextlib.suppress(ProcessLookupError):
        os.killpg(process.pid, signal.SIGTERM)
    try:
        process.wait(timeout=5)
    except subprocess.TimeoutExpired:
        with contextlib.suppress(ProcessLookupError):
            os.killpg(process.pid, signal.SIGKILL)
        process.wait(timeout=5)


class ClipboardBusIsolationTests(unittest.TestCase):
    def test_private_bus_cannot_activate_desktop_services(self):
        """Portal/GVfs activation must not create mounts in the owned runtime."""
        with tempfile.TemporaryDirectory(prefix="hs-clipboard-bus-") as temporary:
            directory = Path(temporary)
            address = f"unix:path={directory / 'bus'}"
            bus = subprocess.Popen(
                private_bus_command(directory, address),
                env={**os.environ, "XDG_RUNTIME_DIR": str(directory)},
                stdout=subprocess.DEVNULL,
                stderr=subprocess.PIPE,
                start_new_session=True,
            )
            try:
                deadline = Deadline(5)
                while not (directory / "bus").exists():
                    self.assertIsNone(bus.poll(), "isolated bus exited")
                    time.sleep(min(0.02, deadline.remaining()))
                for service in (
                    "org.freedesktop.portal.Desktop",
                    "org.freedesktop.portal.Documents",
                    "org.gtk.vfs.Daemon",
                ):
                    result = subprocess.run(
                        [
                            "dbus-send",
                            f"--bus={address}",
                            "--print-reply",
                            "--dest=org.freedesktop.DBus",
                            "/org/freedesktop/DBus",
                            "org.freedesktop.DBus.StartServiceByName",
                            f"string:{service}",
                            "uint32:0",
                        ],
                        check=False,
                        capture_output=True,
                        text=True,
                        timeout=5,
                    )
                    self.assertNotEqual(result.returncode, 0, service)
                    self.assertIn(
                        "org.freedesktop.DBus.Error.ServiceUnknown", result.stderr
                    )
                self.assertFalse((directory / "doc").exists())
            finally:
                stop_process_group(bus)
                bus.stderr.close()


@unittest.skipUnless(os.environ.get("HEARTHSPACE_E2E") == "1", "set HEARTHSPACE_E2E=1")
class ClipboardIntegrationTests(unittest.TestCase):
    def test_wayland_x11_clipboard_roundtrip(self):
        """Exact text/ownership/focus/source-lifetime assertions live in the workflow."""
        binary = Path(
            os.environ.get("HEARTHSPACE_BINARY", ROOT / "target/debug/hearthspace")
        )
        fixture = binary.with_name("hearthspace-clipboard-test")
        self.assertTrue(fixture.is_file(), "build with --features test-apps,xwayland")
        with tempfile.TemporaryDirectory(prefix="hs-clipboard-e2e-") as temporary:
            directory = Path(temporary)
            evidence = Path(os.environ.get("HEARTHSPACE_CLIPBOARD_EVIDENCE", directory))
            self.run_session(binary, fixture, directory, evidence)

    def run_session(self, binary, fixture, directory, evidence):
        evidence.mkdir(parents=True, exist_ok=True)
        socket = directory / "control.sock"
        address = f"unix:path={directory / 'bus'}"
        env = {
            **os.environ,
            "XDG_RUNTIME_DIR": str(directory),
            "HEARTHSPACE_COMMAND_SOCKET": str(socket),
            "HEARTHSPACE_WAYLAND_DISPLAY": "wayland-clipboard",
            "WAYLAND_DISPLAY": "wayland-clipboard",
            "DBUS_SESSION_BUS_ADDRESS": address,
            "GTK_A11Y": "none",
        }
        for key in ("AT_SPI_BUS_ADDRESS", "DISPLAY", "DBUS_SESSION_BUS_PID"):
            env.pop(key, None)
        with contextlib.ExitStack() as cleanup:
            bus_log = cleanup.enter_context((evidence / "dbus.log").open("w"))
            bus = subprocess.Popen(
                private_bus_command(directory, address),
                env=env,
                stdout=bus_log,
                stderr=subprocess.STDOUT,
                start_new_session=True,
            )
            cleanup.callback(stop_process_group, bus)
            deadline = Deadline(120)
            while not (directory / "bus").exists():
                self.assertIsNone(bus.poll(), "private session bus exited")
                time.sleep(min(0.02, deadline.remaining()))
            compositor_log = cleanup.enter_context(
                (evidence / "compositor.log").open("w")
            )
            compositor = subprocess.Popen(
                [
                    str(binary),
                    "--headless",
                    "--no-shell",
                    "--headless-size",
                    "1280x800",
                    "--headless-scale",
                    "1",
                ],
                env=env,
                stdout=compositor_log,
                stderr=subprocess.STDOUT,
                start_new_session=True,
            )
            cleanup.callback(stop_process_group, compositor)
            while True:
                try:
                    Control(socket, Deadline(1)).status()
                    break
                except ControlError:
                    self.assertIsNone(compositor.poll(), "headless compositor exited")
                    time.sleep(min(0.05, deadline.remaining()))
            smoke = Smoke(
                SimpleNamespace(session=evidence, timeout=120), presentation="offscreen"
            )
            smoke.control = Control(socket, deadline)
            try:
                with contextlib.redirect_stdout(io.StringIO()):
                    run_clipboard(smoke, env, fixture)
            except (
                ControlError,
                OSError,
                ValueError,
                KeyError,
                subprocess.SubprocessError,
            ) as error:
                # Retain the compositor's protocol errors in ordinary unittest
                # output even when this run used an ephemeral evidence folder.
                compositor_log.flush()
                self.fail(
                    f"{error}\n{(evidence / 'compositor.log').read_text()[-16000:]}"
                )


if __name__ == "__main__":
    unittest.main()
