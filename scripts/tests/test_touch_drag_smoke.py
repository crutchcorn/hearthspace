"""Reject incomplete touchscreen drags, stale captures, and stuck window grabs."""

import sys
import unittest
from pathlib import Path
from types import SimpleNamespace

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib import touch_drag_assertions as verify
from agentlib.control import ControlError
from agentlib.touch_drag_checks import point
from test_agent_visual import window_capture


class TouchDragEvidenceTests(unittest.TestCase):
    def setUp(self):
        self.before = {
            "x": 20,
            "y": 30,
            "width": 180,
            "height": 150,
            "content_width": 180,
            "canvas_x": 100,
            "canvas_y": 75,
            "mapped": True,
            "visible": True,
            "maximized": False,
            "fullscreen": False,
            "presentation_output": None,
        }
        self.after = self.before | {"x": 40, "y": 50, "canvas_x": 125, "canvas_y": 100}
        self.metadata = {
            "width": 640,
            "height": 480,
            "scale": 2,
            "logical_x": 0,
            "logical_y": 0,
        }
        self.image = window_capture(self.metadata, self.before)
        self.captured = window_capture(self.metadata, self.after)

    def test_zoomed_window_follows_screen_and_canvas_coordinates(self):
        verify.moved(self.before, self.after, (20, 20), 0.8)
        for wrong in (
            self.before,
            self.after | {"x": 25, "y": 35},
            self.after | {"canvas_x": 120, "canvas_y": 95},
        ):
            with self.subTest(wrong=wrong), self.assertRaises(ControlError):
                verify.moved(self.before, wrong, (20, 20), 0.8)

    def test_drag_rejects_resize_maximize_and_unmapped_window(self):
        for state in (
            {"width": 170},
            {"height": 145},
            {"maximized": True},
            {"fullscreen": True},
            {"mapped": False},
            {"visible": False},
        ):
            with self.subTest(state=state), self.assertRaises(ControlError):
                verify.moved(self.before, self.after | state, (20, 20), 0.8)

    def test_reported_move_requires_a_matching_presented_marker(self):
        verify.move_pixels(self.before, self.after, self.image, self.captured)
        with self.assertRaises(ControlError):
            verify.move_pixels(self.before, self.after, self.image, self.image)
        stretched = window_capture(self.metadata, self.after, landmark=(45, 60, 65, 65))
        with self.assertRaisesRegex(ControlError, "marker size"):
            verify.move_pixels(self.before, self.after, self.image, stretched)

    def test_release_checks_canvas_state_as_well_as_screen_geometry(self):
        verify.unchanged(self.after, self.after)
        for wrong in (
            None,
            self.before,
            self.after | {"canvas_y": 110},
            self.after | {"width": 170},
        ):
            with (
                self.subTest(wrong=wrong),
                self.assertRaisesRegex(ControlError, "remained active"),
            ):
                verify.unchanged(self.after, wrong)

    def test_single_finger_must_not_pan_or_zoom_canvas(self):
        before = {"x": 50, "y": 20, "scale": 0.8, "pinch_active": False}
        verify.viewport_unchanged(before, before)
        for change in ({"x": 60}, {"y": 30}, {"scale": 0.9}, {"pinch_active": True}):
            with (
                self.subTest(change=change),
                self.assertRaisesRegex(ControlError, "viewport"),
            ):
                verify.viewport_unchanged(before, before | change)

    def test_resize_requires_final_distance_and_untouched_edges(self):
        resized = self.before | {"width": 120, "content_width": 120}
        verify.resized(self.before, resized, 60)
        for wrong in (
            self.before | {"width": 170},
            resized | {"x": 10},
            resized | {"height": 145},
        ):
            with self.subTest(wrong=wrong), self.assertRaises(ControlError):
                verify.resized(self.before, wrong, 60)

    def test_zoomed_resize_waits_for_exact_final_client_commit(self):
        before = self.before | {"width": 720, "content_width": 900}
        final = before | {"width": 640, "content_width": 800}
        verify.resized(before, final, 80, 0.8)
        for width, content in ((642, 802), (641, 801), (639, 799)):
            intermediate = before | {"width": width, "content_width": content}
            with (
                self.subTest(content=content),
                self.assertRaisesRegex(ControlError, "final touchscreen resize width"),
            ):
                verify.resized(before, intermediate, 80, 0.8)

    def test_resize_half_canvas_pixel_rounds_away_from_zero(self):
        before = self.before | {"width": 1800, "content_width": 900}
        final = before | {"width": 1722, "content_width": 861}
        verify.resized(before, final, 77, 2)
        with self.assertRaisesRegex(ControlError, "final touchscreen resize width"):
            verify.resized(before, final | {"width": 1724, "content_width": 862}, 77, 2)

    def test_resize_requires_new_pixels_in_vacated_strip(self):
        resized = self.before | {"width": 120}
        image = window_capture(self.metadata, resized)
        verify.resize_pixels(self.before, resized, self.image, image)
        with self.assertRaisesRegex(ControlError, "stale pixels"):
            verify.resize_pixels(self.before, resized, self.image, self.image)
        shifted_marker = window_capture(
            self.metadata, resized, landmark=(30, 50, 40, 55)
        )
        with self.assertRaisesRegex(ControlError, "fixed page marker"):
            verify.resize_pixels(self.before, resized, self.image, shifted_marker)

    def test_touch_coordinates_use_output_origin_and_return_quantized_point(self):
        smoke = SimpleNamespace(
            output={
                "logical_x": -600,
                "logical_y": 200,
                "logical_width": 1280,
                "logical_height": 800,
            }
        )
        device, actual = point(smoke, (40.3, 600.3))
        self.assertEqual(device, [[1000, 701]])
        self.assertAlmostEqual(actual[0], 40)
        self.assertAlmostEqual(actual[1], 600.57142857)
        for outside in ((-601, 200), (680, 200), (0, 199), (0, 1000)):
            with (
                self.subTest(outside=outside),
                self.assertRaisesRegex(ControlError, "outside"),
            ):
                point(smoke, outside)


if __name__ == "__main__":
    unittest.main()
