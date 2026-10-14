"""Opt in with HEARTHSPACE_E2E=1; requires a built compositor."""

import json
import os
import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib.control import (
    PNG_SIGNATURE,
    Control,
    ControlError,
    Deadline,
    pixel_to_logical,
)
from agentlib.input import Input

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


if __name__ == "__main__":
    unittest.main()
