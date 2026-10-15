import contextlib
import io
import sys
import unittest
from pathlib import Path
from unittest.mock import Mock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib.cli import action, parser, run
from agentlib.control import Control, ControlError, Deadline


class WindowCommandTests(unittest.TestCase):
    def arguments(self, *args):
        return parser().parse_args(["--socket", "/tmp/unused.sock", *args])

    def test_window_actions_keep_exact_ids_and_canvas_coordinates(self):
        for args, expected in [
            (("focus", "42"), "focus-window 42"),
            (("close", "42"), "close-window 42"),
            (("move", "42", "-1000000", "1000000"), "move-window 42 -1000000 1000000"),
            (("resize", "42", "640", "480"), "resize-window 42 640 480"),
            (("focus-next",), "focus-next-window"),
            (("focus-previous",), "focus-previous-window"),
            (("focus-launcher",), "focus-launcher"),
            (("fullscreen", "42"), "fullscreen-window 42 true"),
            (("fullscreen", "42", "--off"), "fullscreen-window 42 false"),
            (("maximize", "42"), "maximize-window 42 true"),
            (("maximize", "42", "--off"), "maximize-window 42 false"),
            (("restore", "42"), "restore-window 42"),
        ]:
            with self.subTest(args=args):
                control = Mock(path="/tmp/unused.sock")
                result = action(self.arguments(*args, "--wait", "none"), control)
                self.assertTrue(result["accepted"])
                self.assertEqual(result["verification"], "none")
                control.request.assert_called_once_with(expected)
                control.capture.assert_not_called()

    def test_invalid_ids_coordinates_and_sizes_fail_before_connecting(self):
        for args in [
            ("reset-view",),
            ("focus", "0"),
            ("focus", str(1 << 64)),
            ("resize", "1", "0", "400"),
            ("resize", "1", "16385", "400"),
            ("move", "1", "1000001", "0"),
            ("move", "1", "-1000001", "0"),
        ]:
            with (
                self.subTest(args=args),
                contextlib.redirect_stderr(io.StringIO()),
                self.assertRaises(SystemExit),
            ):
                self.arguments(*args)

    def test_default_window_action_waits_for_capture(self):
        control = Mock(path="/tmp/unused.sock")
        control.capture.return_value = ({"frame_id": 7}, b"png")
        result = action(self.arguments("focus", "42"), control)
        self.assertEqual(result["capture"], {"frame_id": 7})
        control.capture.assert_called_once_with(None)

    def test_failed_action_does_not_report_success_or_capture(self):
        control = Mock(path="/tmp/unused.sock")
        control.request.side_effect = ControlError("unknown window")
        with self.assertRaisesRegex(ControlError, "unknown window"):
            action(self.arguments("close", "999"), control)
        control.capture.assert_not_called()

    def test_apps_and_windows_queries_return_live_payload(self):
        for command in ("apps", "windows"):
            payload = {"version": 1, command: [{"id": "example"}]}
            with self.subTest(command=command), patch("agentlib.cli.Control") as ctor:
                ctor.return_value.query.return_value = payload
                self.assertEqual(run(self.arguments(command)), {"ok": True, **payload})
                ctor.return_value.query.assert_called_once_with(command)

    def test_session_status_promotes_live_fields_and_retains_recovery_details(self):
        args = parser().parse_args(["--session", "/tmp/session", "status"])
        live = {"version": 1, "outputs": [{"frame_id": 99}], "windows": [{"id": 42}]}
        lifecycle = {
            "ok": True,
            "status": "ready",
            "outputs": [{"frame_id": 1}],
            "compositor": live,
            "responsive": True,
            "original_vt": 2,
        }
        with patch("agentlib.cli.run_launcher", return_value=lifecycle):
            result = run(args)
        self.assertEqual(result["outputs"], live["outputs"])
        self.assertEqual(result["windows"], live["windows"])
        self.assertEqual(result["original_vt"], 2)
        self.assertTrue(result["responsive"])

    def test_unresponsive_status_does_not_present_startup_outputs_as_live(self):
        args = parser().parse_args(["--session", "/tmp/session", "status"])
        lifecycle = {
            "compositor": None,
            "responsive": False,
            "outputs": [{"frame_id": 1}],
        }
        with patch("agentlib.cli.run_launcher", return_value=lifecycle):
            result = run(args)
        self.assertFalse(result["responsive"])
        self.assertEqual(result["outputs"], [])

    def test_queries_reject_nonobject_or_malformed_json(self):
        control = Control("/tmp/unused.sock", Deadline(1))
        for payload in [b"[]", b"null", b"not-json"]:
            with (
                self.subTest(payload=payload),
                patch.object(control, "request", return_value=payload),
                self.assertRaisesRegex(ControlError, "invalid windows JSON"),
            ):
                control.query("windows")


if __name__ == "__main__":
    unittest.main()
