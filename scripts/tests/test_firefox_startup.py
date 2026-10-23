"""Owned Firefox onboarding waits for real client size and visible placement."""

import sys
import unittest
from pathlib import Path
from unittest.mock import Mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib import firefox_startup
from agentlib.control import ControlError


class FirefoxStartupTests(unittest.TestCase):
    def setUp(self):
        self.output = {
            "logical_x": 0,
            "logical_y": 0,
            "logical_width": 1920,
            "logical_height": 1080,
        }
        self.before = {
            "id": 7,
            "app_id": "firefox",
            "mapped": True,
            "visible": True,
            "focused": True,
            "maximized": False,
            "fullscreen": False,
            "focused_view": False,
            "presentation_output": None,
            "output_coordinates": False,
            "titlebar_height": 0,
            "x": 710,
            "y": 440,
            "width": 500,
            "height": 200,
            "content_width": 500,
            "content_height": 200,
        }
        self.resized = self.before | {
            "width": 1000,
            "height": 760,
            "content_width": 1000,
            "content_height": 760,
        }
        self.placed = self.resized | {"x": 460, "y": 160}
        self.smoke = Mock(target=7)
        self.status = {
            "outputs": [self.output],
            "viewport": {"x": 0, "y": 0, "scale": 1},
            "windows": [self.before],
        }
        self.smoke.control.status.return_value = self.status
        self.smoke.until.side_effect = self.until

    def samples(self, *windows):
        self.smoke.control.status.side_effect = [
            self.status | {"windows": [window]} for window in windows
        ]

    @staticmethod
    def until(name, condition):
        # Bounded external status samples stand in for the native deadline.
        for _ in range(12):
            if result := condition():
                return result
        raise ControlError(f"timed out waiting for {name}")

    def test_waits_for_client_commit_and_reveal_then_refreshes_after_capture(self):
        stale_client = self.resized | {"content_height": 200}
        stale_projection = self.resized | {"width": 500}
        refreshed = self.placed | {"x": 450}
        self.samples(
            self.before,
            stale_client,
            stale_projection,
            self.resized,
            self.resized,  # Full-size window still extends beyond the output.
            self.placed,
            stale_client,  # Capture is not a guarantee of fresh client geometry.
            refreshed,
        )

        self.assertEqual(firefox_startup.prepare(self.smoke), refreshed)

        self.assertEqual(
            [call.args[0] for call in self.smoke.control.request.call_args_list],
            ["resize-window 7 1000 760", "focus-window 7"],
        )
        self.smoke.capture.assert_called_once_with("firefox-startup-sized")
        self.assertEqual(self.smoke.record.call_args.kwargs["after"], refreshed)
        self.assertEqual(self.smoke.record.call_args.kwargs["before"], self.before)

    def test_resize_acknowledgment_without_committed_dimensions_does_not_pass(self):
        with self.assertRaisesRegex(ControlError, "commits client dimensions"):
            firefox_startup.prepare(self.smoke)

        self.smoke.control.request.assert_called_once_with("resize-window 7 1000 760")
        self.smoke.capture.assert_not_called()
        self.smoke.record.assert_not_called()

    def test_refuses_another_window_or_presentation_mode_before_any_action(self):
        for change in (
            {"id": 8},
            {"app_id": "foot"},
            {"maximized": True},
            {"fullscreen": True},
            {"focused_view": True},
            {"output_coordinates": True},
            {"titlebar_height": 44},
        ):
            with self.subTest(change=change):
                self.status["windows"] = [self.before | change]
                with self.assertRaisesRegex(ControlError, "owned floating CSD"):
                    firefox_startup.prepare(self.smoke)
        self.smoke.control.request.assert_not_called()

    def test_rejects_small_output_without_reducing_onboarding_requirements(self):
        self.output["logical_height"] = 700

        with self.assertRaisesRegex(ControlError, "800 by 600"):
            firefox_startup.prepare(self.smoke)

        self.smoke.control.request.assert_not_called()

    def test_sizes_to_smaller_output_and_uses_its_logical_origin(self):
        self.output.update(
            logical_x=-1200, logical_y=100, logical_width=1000, logical_height=900
        )
        resized = self.resized | {
            "width": 952,
            "height": 708,
            "content_width": 952,
            "content_height": 708,
        }
        placed = resized | {"x": -1176, "y": 196}
        self.samples(self.before, resized, placed, placed)

        self.assertEqual(firefox_startup.prepare(self.smoke), placed)

        self.assertEqual(
            [call.args[0] for call in self.smoke.control.request.call_args_list],
            ["resize-window 7 952 708", "focus-window 7"],
        )

    def test_unfocused_or_offscreen_window_cannot_pass_readiness(self):
        self.samples(
            self.before,
            self.resized,
            *([self.placed | {"focused": False}, self.resized] * 6),
        )

        with self.assertRaisesRegex(ControlError, "fully visible"):
            firefox_startup.prepare(self.smoke)

        self.smoke.capture.assert_not_called()
        self.smoke.record.assert_not_called()

    def test_requires_unit_scale_without_changing_the_viewport(self):
        self.status["viewport"]["scale"] = 0.8

        with self.assertRaisesRegex(ControlError, "unit canvas scale"):
            firefox_startup.prepare(self.smoke)

        self.smoke.control.request.assert_not_called()

    def test_rechecks_scale_after_capture_while_allowing_focus_to_pan(self):
        initial = self.status
        resized = initial | {"windows": [self.resized]}
        panned = initial | {
            "windows": [self.placed],
            "viewport": {"x": 250, "y": 280, "scale": 1},
        }
        changed_scale = panned | {"viewport": panned["viewport"] | {"scale": 0.8}}
        self.smoke.control.status.side_effect = [
            initial,
            resized,
            panned,
            changed_scale,
            panned,
        ]

        self.assertEqual(firefox_startup.prepare(self.smoke), self.placed)

        self.smoke.capture.assert_called_once_with("firefox-startup-sized")
        self.assertEqual(self.smoke.record.call_args.kwargs["after"], self.placed)


if __name__ == "__main__":
    unittest.main()
