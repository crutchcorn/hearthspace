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
from agentlib import firefox_assertions as assertions
from agentlib import firefox_checks, firefox_profile, firefox_shell
from agentlib.control import ControlError, Deadline
from agentlib.firefox import FirefoxSmoke
from agentlib.firefox_touch import DeadlineTouchDevice
from test_agent_visual import window_capture


def args(directory):
    return SimpleNamespace(
        session=Path(directory),
        firefox=Path("/usr/bin/true"),
        timeout=30,
        total_timeout=120,
        log_level="info",
    )


class FirefoxEvidenceTests(unittest.TestCase):
    def setUp(self):
        self.metadata = {
            "width": 320,
            "height": 240,
            "scale": 1,
            "logical_x": 0,
            "logical_y": 0,
        }
        self.before = {
            "x": 20,
            "y": 20,
            "width": 200,
            "height": 160,
            "mapped": True,
            "visible": True,
            "maximized": True,
            "fullscreen": False,
            "presentation_output": "eDP-1",
        }
        self.after = {
            **self.before,
            "x": 60,
            "y": 50,
            "width": 120,
            "height": 96,
            "maximized": False,
            "presentation_output": None,
        }
        self.before_image = window_capture(
            self.metadata, self.before, landmark=(40, 40, 80, 60)
        )
        self.after_image = window_capture(
            self.metadata, self.after, landmark=(70, 60, 100, 75)
        )
        self.output = {
            "name": "eDP-1",
            "logical_x": 20,
            "logical_y": -28,
            "logical_width": 200,
            "logical_height": 208,
        }

    def verify(self, *, after=None, image=None):
        return assertions.verify_zoom(
            {"scale": 1},
            {"scale": 0.75},
            self.before,
            after or self.after,
            self.before_image,
            image or self.after_image,
        )

    def test_zoom_requires_restored_state_and_real_marker_shrink(self):
        self.assertEqual(self.verify()["scale_ratio"], 0.75)
        for state in [
            {"maximized": True},
            {"fullscreen": True},
            {"presentation_output": "eDP-1"},
            {"visible": False},
        ]:
            with (
                self.subTest(state=state),
                self.assertRaisesRegex(ControlError, "restore"),
            ):
                self.verify(after=self.after | state)
        stale = window_capture(self.metadata, self.after, landmark=(70, 60, 110, 80))
        with self.assertRaisesRegex(ControlError, "marker"):
            self.verify(image=stale)

    def test_smaller_marker_does_not_hide_unchanged_window_bounds(self):
        with self.assertRaisesRegex(ControlError, "both axes"):
            self.verify(after=self.after | {"width": self.before["width"]})

    def test_resize_checks_pointer_distance_and_untouched_edges(self):
        before = self.after | {"width": 400}
        after = before | {"width": 240}
        assertions.verify_resize(before, after, 160)
        with self.assertRaisesRegex(ControlError, "pointer distance"):
            assertions.verify_resize(before, before | {"width": 300}, 160)
        with self.assertRaisesRegex(ControlError, "another edge"):
            assertions.verify_resize(before, after | {"x": before["x"] + 10}, 160)

    def test_fullscreen_stays_fixed_in_state_geometry_and_pixels(self):
        full = self.before | {"fullscreen": True}
        assertions.verify_fullscreen(full, full, self.before_image, self.before_image)
        with self.assertRaisesRegex(ControlError, "fullscreen state"):
            assertions.verify_fullscreen(
                full, self.before, self.before_image, self.before_image
            )
        with self.assertRaisesRegex(ControlError, "moved or scaled"):
            assertions.verify_fullscreen(
                full, full | {"width": 180}, self.before_image, self.before_image
            )

    def test_fullscreen_flag_before_full_output_configure_is_not_ready(self):
        before_configure = self.before | {"fullscreen": True}
        full = before_configure | {
            "y": self.output["logical_y"],
            "height": self.output["logical_height"],
        }
        self.assertFalse(
            assertions.fullscreen_at_output(before_configure, [self.output])
        )
        self.assertTrue(assertions.fullscreen_at_output(full, [self.output]))
        for state in [
            {"fullscreen": False},
            {"visible": False},
            {"mapped": False},
            {"presentation_output": "unknown"},
        ]:
            with self.subTest(state=state):
                self.assertFalse(
                    assertions.fullscreen_at_output(full | state, [self.output])
                )

    def test_independent_maximize_case_resets_scale_before_using_real_csd(self):
        smoke = Mock(target=7)
        smoke.control.status.return_value = {
            "viewport": {"scale": 1},
            "outputs": [self.output],
        }
        transitional = self.before | {"width": self.after["width"]}
        smoke.window.side_effect = [
            self.after,
            self.after,
            transitional,
            self.before,
        ]
        samples = []

        def until(_name, condition):
            for _ in range(4):
                samples.append(value := condition())
                if value:
                    return value
            self.fail("maximized geometry never settled")

        smoke.until.side_effect = until
        with patch.object(firefox_checks, "activate") as activate:
            result = firefox_checks.prepare_maximized(smoke)
        self.assertEqual(
            [call.args[0] for call in smoke.control.request.call_args_list],
            ["reset-view", "focus-window 7"],
        )
        activate.assert_called_once_with(smoke, "Maximize")
        self.assertEqual(smoke.floating_before_max, self.after)
        self.assertEqual(result, self.before)
        self.assertEqual(samples, [True, None, self.before])

    def test_maximized_flag_with_transitional_client_geometry_is_rejected(self):
        self.assertTrue(assertions.maximized_at_output(self.before, [self.output]))
        for state in [
            {"width": 120},
            {"height": 96},
            {"x": 30},
            {"y": 30},
            {"presentation_output": "unknown"},
            {"maximized": False},
        ]:
            with self.subTest(state=state):
                self.assertFalse(
                    assertions.maximized_at_output(self.before | state, [self.output])
                )

    def test_cancelled_maximize_waits_for_configured_bounds_before_comparing(self):
        smoke = Mock(target=7, floating_before_max=self.after)
        smoke.control.status.return_value = {
            "outputs": [self.output],
            "viewport": {"scale": 1, "x": 0, "y": 0, "pinch_active": False},
        }
        transitional = self.before | {"width": self.after["width"]}
        smoke.window.side_effect = [transitional, self.before]
        smoke.until.side_effect = lambda _name, condition: (
            self.assertIsNone(condition()) or condition()
        )
        self.assertEqual(firefox_checks.wait_maximized(smoke), self.before)
        self.assertEqual(smoke.window.call_count, 2)


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
            "scale": 2,
            "logical_x": 100,
            "logical_y": 50,
            "logical_width": 1280,
        }
        self.bounds = {"x": 990, "y": 18, "width": 158, "height": 60}

    def test_observed_physical_bounds_convert_to_logical_shell_center(self):
        self.assertEqual(
            firefox_shell.button_center(self.bounds, self.output), (634.5, 74)
        )
        for bad in [
            None,
            self.bounds | {"width": 0},
            self.bounds | {"x": float("nan")},
            self.bounds | {"y": 100},
            self.bounds | {"x": 3000},
        ]:
            with self.subTest(bounds=bad), self.assertRaises(ControlError):
                firefox_shell.button_center(bad, self.output)

    def test_shell_pointer_focus_changes_before_button_release(self):
        smoke = Mock(target=7)
        smoke.control.status.return_value = {"outputs": [self.output]}
        smoke.until.side_effect = lambda _name, condition: condition()
        events = []
        smoke.window.side_effect = lambda _target: (
            events.append("focus-observed") or {"focused": False}
        )
        with (
            patch.object(
                firefox_shell,
                "a11y",
                return_value={"elements": [{"bounds": self.bounds}]},
            ) as accessibility,
            patch.object(firefox_shell, "Input") as input_type,
        ):
            inputs = input_type.return_value.__enter__.return_value
            inputs.down.side_effect = lambda *_: events.append("down")
            inputs.up.side_effect = lambda *_: events.append("up")
            firefox_shell.click_zoom_out(smoke)
            inputs.motion.assert_called_once_with(634.5, 74)
        self.assertEqual(events, ["down", "focus-observed", "focus-observed", "up"])
        self.assertEqual(accessibility.call_args.args[1], "find")

    def test_distant_pinned_setup_resets_with_shell_focus_then_refocuses_browser(self):
        smoke = Mock(target=7)
        smoke.window.return_value = {"focused": True}
        smoke.until.side_effect = lambda _name, condition: condition()
        view = {"x": 0, "y": 0, "scale": 1}
        with patch.object(firefox_shell, "settled", return_value=view):
            self.assertEqual(firefox_shell.reset_pinned_view(smoke), view)
        self.assertEqual(
            [call.args[0] for call in smoke.control.request.call_args_list],
            ["focus-launcher", "reset-view", "focus-window 7"],
        )
        with (
            patch.object(firefox_shell, "settled", return_value=view | {"x": 100000}),
            self.assertRaisesRegex(ControlError, "reset the distant viewport"),
        ):
            firefox_shell.reset_pinned_view(smoke)


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
