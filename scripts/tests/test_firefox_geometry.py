"""State, client geometry, and presented-pixel proofs for canvas maximization."""

import sys
import unittest
from pathlib import Path
from unittest.mock import Mock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib import firefox_assertions as assertions
from agentlib import firefox_checks
from agentlib.control import ControlError
from test_agent_visual import window_capture


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
            "canvas_x": 100000,
            "canvas_y": 100000,
            "content_width": 200,
            "content_height": 160,
            "mapped": True,
            "visible": True,
            "maximized": True,
            "fullscreen": False,
            "presentation_output": None,
            "output_coordinates": False,
            "maximized_canvas_bounds": {"x": 20, "y": 20, "width": 200, "height": 160},
        }
        self.after = {
            **self.before,
            "x": 60,
            "y": 50,
            "width": 150,
            "height": 120,
        }
        self.before_image = window_capture(
            self.metadata, self.before, landmark=(40, 40, 80, 60)
        )
        self.after_image = window_capture(
            self.metadata, self.after, landmark=(75, 65, 105, 80)
        )
        self.output = {
            "name": "eDP-1",
            "logical_x": 8,
            "logical_y": -58,
            "logical_width": 224,
            "logical_height": 316,
        }
        self.focused = self.before | {
            "focused": True,
            "focused_view": True,
            "output_coordinates": True,
            "presentation_output": "eDP-1",
            "titlebar_height": 0,
            "x": 8,
            "y": -58,
            "width": 224,
            "height": 316,
            "content_width": 224,
            "content_height": 316,
        }

    def verify(self, *, after=None, image=None):
        return assertions.verify_zoom(
            {"x": 0, "y": 0, "scale": 1},
            {"x": -60, "y": -140 / 3, "scale": 0.75},
            self.before,
            after or self.after,
            self.before_image,
            image or self.after_image,
        )

    def test_zoom_preserves_canvas_maximization_and_real_marker_shrink(self):
        self.assertEqual(self.verify()["scale_ratio"], 0.75)
        for state in [
            {"maximized": False},
            {"fullscreen": True},
            {"presentation_output": "eDP-1"},
            {"output_coordinates": True},
            {"visible": False},
        ]:
            with (
                self.subTest(state=state),
                self.assertRaisesRegex(ControlError, "maximization"),
            ):
                self.verify(after=self.after | state)
        stale = window_capture(self.metadata, self.after, landmark=(70, 60, 110, 80))
        with self.assertRaisesRegex(ControlError, "marker"):
            self.verify(image=stale)

    def test_smaller_marker_does_not_hide_unchanged_window_bounds(self):
        with self.assertRaisesRegex(ControlError, "both axes"):
            self.verify(after=self.after | {"width": self.before["width"]})

    def test_resize_checks_pointer_distance_and_untouched_edges(self):
        before = self.after | {"width": 400, "maximized": False}
        after = before | {"width": 240}
        assertions.verify_resize(before, after, 160)
        with self.assertRaisesRegex(ControlError, "pointer distance"):
            assertions.verify_resize(before, before | {"width": 300}, 160)
        with self.assertRaisesRegex(ControlError, "another edge"):
            assertions.verify_resize(before, after | {"x": before["x"] + 10}, 160)

    def test_fullscreen_stays_fixed_in_state_geometry_and_pixels(self):
        full = self.before | {"fullscreen": True, "presentation_output": "eDP-1"}
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
        before_configure = self.before | {
            "fullscreen": True,
            "presentation_output": "eDP-1",
        }
        full = before_configure | {
            "x": self.output["logical_x"],
            "y": self.output["logical_y"],
            "width": self.output["logical_width"],
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
        floating = self.after | {"maximized": False}
        smoke.window.side_effect = [
            floating,
            floating,
            self.focused | {"content_width": 150},
            self.focused,
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
        with (
            patch.object(firefox_checks, "activate") as activate,
            patch.object(firefox_checks.firefox_focused, "evidence") as focused,
            patch.object(firefox_checks, "set_viewport") as place,
            patch.object(firefox_checks, "settled"),
        ):
            result = firefox_checks.prepare_maximized(smoke)
        self.assertEqual(
            [call.args[0] for call in smoke.control.request.call_args_list],
            [
                "focus-window 7",
                "pointer-motion-abs 120.0 100.0",
                "key-down 125",
                "key-down 50",
                "key-up 50",
                "key-up 125",
            ],
        )
        place.assert_called_once_with(smoke, x=0, y=0, scale=1)
        activate.assert_called_once_with(smoke, "Maximize")
        focused.assert_called_once_with(smoke)
        self.assertEqual(smoke.floating_before_max, floating)
        self.assertEqual(result, self.before)
        self.assertEqual(samples, [True, False, True, None, self.before])

    def test_csd_focused_setup_rejects_fullscreen_stale_size_and_wrong_output(self):
        self.assertTrue(assertions.focused_at_output(self.focused, [self.output]))
        for change in (
            {"fullscreen": True},
            {"focused_view": False},
            {"maximized": False},
            {"focused": False},
            {"content_width": 200},
            {"content_height": 160},
            {"width": 200},
            {"x": 0},
            {"presentation_output": "unknown"},
            {"output_coordinates": False},
        ):
            with self.subTest(change=change):
                self.assertFalse(
                    assertions.focused_at_output(self.focused | change, [self.output])
                )

    def test_maximized_flag_with_transitional_client_geometry_is_rejected(self):
        self.assertTrue(assertions.maximized_at_output(self.before, [self.output]))
        for state in [
            {"width": 120},
            {"height": 96},
            {"x": 30},
            {"y": 30},
            # Legacy corner clearance must not count as the website workarea.
            {"y": -4, "height": 256},
            {"presentation_output": "eDP-1"},
            {"maximized": False},
        ]:
            with self.subTest(state=state):
                self.assertFalse(
                    assertions.maximized_at_output(self.before | state, [self.output])
                )

    def test_next_case_restores_client_size_before_maximizing_at_new_view(self):
        floating = self.after | {
            "maximized": False,
            "content_width": 120,
            "content_height": 96,
        }
        smoke = Mock(target=7, floating_before_max=floating)
        smoke.control.status.return_value = {
            "viewport": {"x": 0, "y": 0, "scale": 1},
            "outputs": [self.output],
        }
        smoke.window.side_effect = [
            self.before,
            self.before | {"maximized": False},
            floating,
            floating,
            self.focused,
            self.before,
        ]

        def until(_name, condition):
            for _ in range(4):
                if result := condition():
                    return result
            self.fail("independent maximization never settled")

        smoke.until.side_effect = until
        with (
            patch.object(firefox_checks, "activate") as activate,
            patch.object(firefox_checks.firefox_focused, "evidence"),
            patch.object(firefox_checks, "set_viewport"),
            patch.object(firefox_checks, "settled"),
        ):
            self.assertEqual(firefox_checks.prepare_maximized(smoke), self.before)
        self.assertEqual(
            [call.args[1] for call in activate.call_args_list],
            ["Restore Down", "Maximize"],
        )
        self.assertEqual(smoke.floating_before_max, floating)

    def test_tiny_output_preserves_one_pixel_between_corner_clearances(self):
        output = self.output | {"logical_height": 1}
        window = self.before | {"y": output["logical_y"], "height": 1}
        self.assertTrue(assertions.maximized_at_output(window, [output]))
        output = output | {"logical_height": 120}
        window = window | {"y": output["logical_y"] + 59, "height": 2}
        self.assertTrue(assertions.maximized_at_output(window, [output]))

    def test_zoom_rejects_changed_client_size_or_maximized_world_position(self):
        for state in (
            {"content_width": 180},
            {"content_height": 130},
            {"canvas_x": 12},
        ):
            with (
                self.subTest(state=state),
                self.assertRaisesRegex(ControlError, "world geometry"),
            ):
                self.verify(after=self.after | state)
        with self.assertRaisesRegex(ControlError, "position"):
            self.verify(after=self.after | {"x": 80})
        for bounds in (None, self.before["maximized_canvas_bounds"] | {"width": 120}):
            with (
                self.subTest(bounds=bounds),
                self.assertRaisesRegex(ControlError, "world bounds"),
            ):
                self.verify(after=self.after | {"maximized_canvas_bounds": bounds})

    def test_full_size_output_reserves_twenty_four_and_ninety_six_pixels(self):
        output = self.output | {
            "logical_x": 0,
            "logical_y": 0,
            "logical_width": 1280,
            "logical_height": 800,
        }
        window = self.before | {"x": 24, "y": 96, "width": 1232, "height": 608}
        self.assertTrue(assertions.maximized_at_output(window, [output]))
        self.assertFalse(
            assertions.maximized_at_output(window | {"x": 0, "width": 1280}, [output])
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


if __name__ == "__main__":
    unittest.main()
