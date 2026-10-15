"""Opt in with HEARTHSPACE_E2E=1; requires a built compositor."""

import contextlib
import io
import json
import os
import signal
import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path
from types import SimpleNamespace

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib.control import (
    PNG_SIGNATURE,
    Control,
    ControlError,
    Deadline,
    pixel_to_logical,
)
from agentlib.input import Input
from agentlib.smoke import Smoke

ROOT = Path(__file__).resolve().parents[2]


@unittest.skipUnless(os.environ.get("HEARTHSPACE_E2E") == "1", "set HEARTHSPACE_E2E=1")
class HeadlessIntegrationTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.directory = tempfile.TemporaryDirectory(prefix="hs-agent-e2e-")
        cls.runtime = Path(cls.directory.name)
        cls.socket = cls.runtime / "control.sock"
        cls.log = (cls.runtime / "compositor.log").open("w+")
        env = {
            **os.environ,
            "XDG_RUNTIME_DIR": str(cls.runtime),
            "WAYLAND_DISPLAY": "wayland-test",
            "HEARTHSPACE_COMMAND_SOCKET": str(cls.socket),
        }
        binary = os.environ.get(
            "HEARTHSPACE_BINARY", str(ROOT / "target/debug/hearthspace")
        )
        cls.process = subprocess.Popen(
            [
                binary,
                "--headless",
                "--no-shell",
                "--headless-size",
                "801x601",
                "--headless-scale",
                "2",
            ],
            env=env,
            stdout=cls.log,
            stderr=subprocess.STDOUT,
        )
        cls.addClassCleanup(cls.cleanup)
        deadline = Deadline(20)
        while True:
            try:
                Control(cls.socket, Deadline(1)).status()
                return
            except ControlError:
                if cls.process.poll() is not None:
                    cls.log.seek(0)
                    raise AssertionError(cls.log.read()) from None
                time.sleep(min(0.05, deadline.remaining()))

    @classmethod
    def cleanup(cls):
        try:
            Control(cls.socket, Deadline(1)).request("quit")
        except ControlError:
            pass
        try:
            cls.process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            cls.process.kill()
            cls.process.wait(timeout=5)
        cls.log.close()
        cls.directory.cleanup()

    def test_capture_legacy_png_fresh_frames_and_cursor(self):
        control = Control(self.socket, Deadline(15))
        output = control.status()["outputs"][0]["name"]
        first, png = control.capture(output)
        self.assertEqual(
            (first["width"], first["height"], first["scale"]), (801, 601, 2)
        )
        self.assertEqual(first["output"], output)
        self.assertEqual(first["cursor"], "embedded")
        self.assertTrue(png.startswith(PNG_SIGNATURE))
        self.assertTrue(control.request("screenshot").startswith(PNG_SIGNATURE))
        with Input(control) as inputs:
            inputs.motion(40, 100)
        moved, moved_png = control.capture(output)
        self.assertGreater(moved["frame_id"], first["frame_id"])
        self.assertNotEqual(moved_png, png)
        self.assertEqual(pixel_to_logical(800, 600, first), (400, 300))

    def test_cli_saves_metadata_and_reports_bad_output(self):
        destination = self.runtime / "agent.png"
        command = [
            str(ROOT / "scripts/hearthspace-agent"),
            "--socket",
            str(self.socket),
        ]
        result = subprocess.run(
            [*command, "screenshot", str(destination)],
            check=True,
            capture_output=True,
            text=True,
            timeout=15,
        )
        response = json.loads(result.stdout)
        self.assertTrue(response["ok"])
        self.assertEqual(
            json.loads(destination.with_suffix(".json").read_text())["scale"], 2
        )
        result = subprocess.run(
            [*command, "--output", "missing-output", "screenshot", str(destination)],
            check=False,
            capture_output=True,
            text=True,
            timeout=15,
        )
        self.assertEqual(result.returncode, 1)
        self.assertFalse(json.loads(result.stderr)["ok"])

    def test_real_terminal_typing_move_resize_and_cursor(self):
        with tempfile.TemporaryDirectory(prefix="hs-foot-e2e-") as temporary:
            directory = Path(temporary)
            socket = directory / "control.sock"
            env = {
                **os.environ,
                "XDG_RUNTIME_DIR": str(directory),
                "HEARTHSPACE_COMMAND_SOCKET": str(socket),
                "HEARTHSPACE_WAYLAND_DISPLAY": "wayland-foot-e2e",
            }
            with (directory / "compositor.log").open("w") as log:
                process = subprocess.Popen(
                    [
                        str(ROOT / "target/debug/hearthspace"),
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
                try:
                    control = Control(socket, Deadline(60))
                    while not socket.exists():
                        self.assertIsNone(process.poll())
                        time.sleep(min(0.02, control.deadline.remaining()))
                    smoke = Smoke(
                        SimpleNamespace(session=directory, timeout=60),
                        presentation="offscreen",
                    )
                    smoke.control = control
                    with contextlib.redirect_stdout(io.StringIO()):
                        smoke.terminal(control.capture())
                    self.assertEqual(
                        (smoke.artifacts / "typed-marker.txt").read_text(),
                        "HEARTHSPACE_NATIVE_MARKER",
                    )
                    self.assertIn(
                        "cursor-inclusion", [step["step"] for step in smoke.results]
                    )
                finally:
                    os.killpg(process.pid, signal.SIGTERM)
                    process.wait(timeout=5)


if __name__ == "__main__":
    unittest.main()
