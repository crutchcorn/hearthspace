"""Host-independent evidence, lifecycle, and profile ownership regressions."""

import contextlib
import io
import json
import os
import sys
import tempfile
import time
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import Mock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib import firefox_profile, firefox_shell, map_checks
from agentlib.control import ControlError, Deadline
from agentlib.firefox import FirefoxSmoke
from agentlib.firefox_touch import DeadlineTouchDevice


def args(directory):
    return SimpleNamespace(
        session=Path(directory),
        firefox=Path("/usr/bin/true"),
        timeout=30,
        total_timeout=120,
        log_level="info",
    )


class FirefoxOwnershipTests(unittest.TestCase):
    def test_existing_profile_is_never_reused(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            profile = root / "firefox-profile"
            profile.mkdir()
            marker = profile / "keep"
            marker.write_text("original")
            with self.assertRaises(FileExistsError):
                firefox_profile.prepare(root, "new")
            self.assertEqual(marker.read_text(), "original")

    def test_cleanup_requires_completed_session_and_exact_owner(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            profile = firefox_profile.prepare(root, "marker")
            manifest = root / "session.json"
            manifest.write_text('{"cleanup":{"complete":false}}')
            with self.assertRaisesRegex(ControlError, "incomplete"):
                firefox_profile.cleanup(root, "marker")
            manifest.write_text('{"cleanup":{"complete":true}}')
            with self.assertRaisesRegex(ControlError, "ownership"):
                firefox_profile.cleanup(root, "different")
            self.assertTrue(profile.exists())
            with patch.object(Path, "iterdir", return_value=[]):
                firefox_profile.cleanup(root, "marker")
            self.assertFalse(profile.exists())
            self.assertTrue((root / "browser-fixtures/zoom.html").exists())

    def test_cleanup_refuses_live_profile_process(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            profile = firefox_profile.prepare(root, "marker")
            (root / "session.json").write_text('{"cleanup":{"complete":true}}')
            process = root / "123"
            process.mkdir()
            (process / "cmdline").write_bytes(
                b"firefox\0--profile\0" + os.fsencode(profile) + b"\0"
            )
            with (
                patch.object(Path, "iterdir", return_value=[process]),
                self.assertRaisesRegex(ControlError, "remains"),
            ):
                firefox_profile.cleanup(root, "marker")
            self.assertTrue(profile.exists())

    def test_failed_workflow_stops_session_and_retains_profile_for_debugging(self):
        with (
            tempfile.TemporaryDirectory() as directory,
            contextlib.redirect_stdout(io.StringIO()),
        ):
            smoke = FirefoxSmoke(args(directory))
            operations = []

            def launcher(operation, *extra):
                operations.append((operation, extra))
                (smoke.directory / "session.json").write_text(
                    json.dumps({"cleanup": {"complete": operation == "stop"}})
                )

            with (
                patch("agentlib.daily.shutil.which", return_value="/test"),
                patch.object(smoke, "launcher", side_effect=launcher),
                patch.object(
                    smoke, "workflow", side_effect=ControlError("fixture failure")
                ),
                patch.object(smoke, "verify_restored") as restored,
                patch.object(firefox_profile, "verify_stopped") as verified,
                self.assertRaisesRegex(ControlError, "fixture failure"),
            ):
                smoke.run()
            self.assertEqual([op[0] for op in operations], ["start", "stop"])
            self.assertIn("--no-build", operations[0][1])
            restored.assert_called_once()
            verified.assert_called_once()
            self.assertTrue(smoke.profile.exists())


class FirefoxShellPointerTests(unittest.TestCase):
    def setUp(self):
        self.output = {
            "name": "test",
            "scale": 2,
            "logical_x": 100,
            "logical_y": 50,
            "logical_width": 1280,
            "logical_height": 800,
        }
        self.corner = {
            "corner": "canvas",
            "output": "test",
            "x": 124,
            "y": 772,
            "width": 102,
            "height": 54,
        }
        self.bounds = {"x": 14, "y": 14, "width": 68, "height": 68}

    def test_observed_physical_bounds_convert_to_logical_shell_center(self):
        self.assertEqual(
            firefox_shell.button_center(self.bounds, self.output, self.corner),
            (148, 796),
        )
        for bad in [
            None,
            self.bounds | {"width": 0},
            self.bounds | {"x": float("nan")},
            self.bounds | {"y": 100},
            self.bounds | {"x": 3000},
        ]:
            with self.subTest(bounds=bad), self.assertRaises(ControlError):
                firefox_shell.button_center(bad, self.output, self.corner)

    def test_shell_pointer_focus_changes_before_button_release(self):
        smoke = Mock(target=7)
        surface = {"output": "test", "x": 124, "y": 450, "width": 320, "height": 310}
        smoke.control.status.return_value = {
            "outputs": [self.output],
            "shell_corners": [self.corner],
            "canvas_maps": [surface],
        }
        smoke.until.side_effect = lambda _name, condition: condition()
        events = []
        smoke.window.side_effect = lambda _target: (
            events.append("focus-observed") or {"focused": False}
        )
        with (
            patch.object(
                map_checks,
                "a11y",
                side_effect=[
                    {"elements": [{"bounds": self.bounds}]},
                    {
                        "elements": [
                            {"bounds": {"x": 20, "y": 500, "width": 80, "height": 60}}
                        ]
                    },
                ],
            ) as accessibility,
            patch.object(map_checks, "Input") as input_type,
        ):
            inputs = input_type.return_value.__enter__.return_value
            inputs.down.side_effect = lambda *_: events.append("down")
            inputs.up.side_effect = lambda *_: events.append("up")
            firefox_shell.click_zoom_out(smoke)
            self.assertEqual(
                [call.args for call in inputs.motion.call_args_list],
                [(148, 796), (154, 715)],
            )
        self.assertEqual(events, ["down", "focus-observed", "focus-observed", "up"] * 2)
        self.assertIn("Canvas map", accessibility.call_args_list[0].args[2])
        self.assertIn("Zoom out", accessibility.call_args_list[1].args[2])
        self.assertEqual(accessibility.call_args.args[1], "find")

    def test_canvas_maximized_setup_pans_without_refocusing_browser(self):
        smoke = Mock(target=7)
        smoke.window.return_value = {"focused": True}
        smoke.until.side_effect = lambda _name, condition: condition()
        original = {"x": 90000, "y": 91000, "scale": 1}
        view = {"x": 89920, "y": 91040, "scale": 1}
        with (
            patch.object(firefox_shell, "settled", side_effect=[original, view]),
            patch.object(firefox_shell, "set_viewport") as place,
        ):
            self.assertEqual(firefox_shell.prepare_canvas_view(smoke), view)
        smoke.control.request.assert_not_called()
        place.assert_called_once_with(smoke, **view)
        with (
            patch.object(firefox_shell, "set_viewport"),
            patch.object(firefox_shell, "settled", side_effect=[original, original]),
            self.assertRaisesRegex(ControlError, "pan the maximized canvas view"),
        ):
            firefox_shell.prepare_canvas_view(smoke)


class FirefoxTouchDeadlineTests(unittest.TestCase):
    def test_fragmented_worker_reply_cannot_stall_past_step_deadline(self):
        read, write = os.pipe()
        try:
            with os.fdopen(read, "rb") as reader:
                device = DeadlineTouchDevice(
                    Path("/unused"), None, Deadline(0.03), "touchscreen"
                )
                device.process = SimpleNamespace(stdout=reader)
                os.write(write, b'{"ok":')
                started = time.monotonic()
                with self.assertRaisesRegex(ControlError, "deadline|timed out"):
                    device.receive()
                self.assertLess(time.monotonic() - started, 0.5)
        finally:
            os.close(write)


if __name__ == "__main__":
    unittest.main()
