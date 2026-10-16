"""Real WayVNC captures and input, enabled alongside other headless E2E tests."""

import contextlib
import os
import shlex
import shutil
import socket
import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib.control import Control, ControlError, Deadline
from test_clipboard_headless import stop_process_group
from wayvnc_client import VncClient

ROOT = Path(__file__).resolve().parents[2]


def until(check, seconds=15):
    deadline = Deadline(seconds)
    while True:
        value = check()
        if value:
            return value
        time.sleep(min(0.05, deadline.remaining()))


@unittest.skipUnless(os.environ.get("HEARTHSPACE_E2E") == "1", "set HEARTHSPACE_E2E=1")
class WayVncHeadlessTests(unittest.TestCase):
    def test_real_vnc_capture_keyboard_pointer_and_disconnect(self):
        self.assertIsNotNone(shutil.which("wayvnc"), "install wayvnc for E2E tests")
        self.assertIsNotNone(shutil.which("foot"), "install foot for E2E tests")
        with tempfile.TemporaryDirectory(prefix="hs-vnc-e2e-") as directory:
            root = Path(directory)
            evidence = Path(os.environ.get("HEARTHSPACE_VNC_EVIDENCE", root))
            evidence.mkdir(parents=True, exist_ok=True)
            env = {
                **os.environ,
                "XDG_RUNTIME_DIR": directory,
                "WAYLAND_DISPLAY": "wayland-vnc-test",
                "HEARTHSPACE_WAYLAND_DISPLAY": "wayland-vnc-test",
                "HEARTHSPACE_COMMAND_SOCKET": str(root / "control.sock"),
                "LIBGL_ALWAYS_SOFTWARE": "1",
            }
            for key in ("DISPLAY", "DBUS_SESSION_BUS_ADDRESS", "WAYLAND_SOCKET"):
                env.pop(key, None)
            with contextlib.ExitStack() as cleanup:
                log = cleanup.enter_context((evidence / "compositor.log").open("w"))
                compositor = subprocess.Popen(
                    [
                        str(ROOT / "target/debug/hearthspace"),
                        "--headless",
                        "--no-shell",
                        "--remote-desktop",
                        "--headless-size",
                        "1600x1000",
                        "--headless-scale",
                        "2",
                    ],
                    env=env,
                    stdout=log,
                    stderr=log,
                    start_new_session=True,
                )
                cleanup.callback(stop_process_group, compositor)
                control = Control(root / "control.sock", Deadline(90))

                def ready():
                    self.assertIsNone(compositor.poll(), "compositor exited")
                    try:
                        return control.status()
                    except ControlError:
                        return None

                until(ready)
                # Let the kernel choose a local port, then release it for WayVNC.
                with socket.socket() as listener:
                    listener.bind(("127.0.0.1", 0))
                    port = listener.getsockname()[1]
                vnc_log = cleanup.enter_context((evidence / "wayvnc.log").open("w"))
                server = subprocess.Popen(
                    [
                        "wayvnc",
                        "--config",
                        "/dev/null",
                        "--render-cursor",
                        "--disable-resizing",
                        "127.0.0.1",
                        str(port),
                    ],
                    env=env,
                    stdout=vnc_log,
                    stderr=vnc_log,
                    start_new_session=True,
                )
                cleanup.callback(stop_process_group, server)

                def connected():
                    self.assertIsNone(
                        server.poll(), (evidence / "wayvnc.log").read_text()
                    )
                    try:
                        return VncClient(port)
                    except ConnectionRefusedError:
                        return None

                client = cleanup.enter_context(until(connected))
                first = until(
                    lambda: (frame := client.frame()) and len(set(frame)) > 16 and frame
                )
                self.assertEqual((client.width, client.height), (1600, 1000))
                self.assertGreater(len(set(first)), 16, "captured frame is blank")
                terminal_log = cleanup.enter_context((evidence / "foot.log").open("w"))
                terminal = subprocess.Popen(
                    [
                        "foot",
                        "--app-id=hearthspace-vnc-test",
                        "bash",
                        "--noprofile",
                        "--norc",
                    ],
                    env=env,
                    stdout=terminal_log,
                    stderr=terminal_log,
                    start_new_session=True,
                )
                cleanup.callback(stop_process_group, terminal)

                def window():
                    return next(
                        (
                            w
                            for w in control.status()["windows"]
                            if w.get("app_id") == "hearthspace-vnc-test"
                            and w.get("mapped")
                        ),
                        None,
                    )

                original = until(window)
                destination = evidence / "received.txt"
                destination.unlink(missing_ok=True)
                client.type(
                    f"printf '%s' 'WayVNC Input 123!' > {shlex.quote(str(destination))}\n"
                )
                until(
                    lambda: (
                        destination.exists()
                        and destination.read_text() == "WayVNC Input 123!"
                    )
                )
                self.assertNotEqual(
                    first, client.frame(), "VNC did not update after terminal mapped"
                )
                # Drag the title bar through RFB, with coordinates in output pixels at scale 2.
                x, y = (original["x"] + 100) * 2, (original["y"] + 10) * 2
                client.pointer(x, y)
                client.pointer(x, y, 1)
                client.pointer(x + 60, y + 40, 1)
                client.pointer(x + 60, y + 40)
                moved = until(
                    lambda: (w := window()) and w["x"] > original["x"] + 20 and w
                )
                self.assertAlmostEqual(moved["x"] - original["x"], 30, delta=2)
                self.assertAlmostEqual(moved["y"] - original["y"], 20, delta=2)
                # Disconnect with a held button; reconnecting must not keep dragging.
                client.pointer(x + 60, y + 40, 1)
                client.close()
                time.sleep(0.15)
                with VncClient(port) as replacement:
                    replacement.frame()
                    replacement.pointer(x + 160, y + 140)
                    time.sleep(0.15)
                    self.assertEqual(
                        (window()["x"], window()["y"]), (moved["x"], moved["y"])
                    )
                    # WayVNC sends modifier masks before key-up. Alt release
                    # must still commit switching instead of leaving its grab.
                    second = subprocess.Popen(
                        [
                            "foot",
                            "--app-id=hearthspace-vnc-second",
                            "bash",
                            "--noprofile",
                            "--norc",
                        ],
                        env=env,
                        stdout=terminal_log,
                        stderr=terminal_log,
                        start_new_session=True,
                    )
                    cleanup.callback(stop_process_group, second)
                    until(
                        lambda: any(
                            w.get("app_id") == "hearthspace-vnc-second"
                            and w.get("focused")
                            for w in control.status()["windows"]
                        )
                    )
                    replacement.key(0xFFE9, True)
                    replacement.key(0xFF09, True)
                    replacement.key(0xFF09, False)
                    replacement.key(0xFFE9, False)
                    until(lambda: window().get("focused"))
                    self.assertFalse(control.status()["window_switcher"]["active"])


if __name__ == "__main__":
    unittest.main()
