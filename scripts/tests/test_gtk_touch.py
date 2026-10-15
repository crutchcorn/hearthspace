"""GTK touch evidence must reject stale content and invalid app coordinates."""

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib.control import ControlError
from agentlib.gtk_touch_checks import local_center
from agentlib.gtk_touch_pixels import translated
from test_agent_visual import capture


class GtkTouchEvidenceTests(unittest.TestCase):
    def setUp(self):
        self.metadata = {
            "width": 620,
            "height": 520,
            "scale": 1,
            "logical_x": 0,
            "logical_y": 0,
        }
        self.before = {"x": 80, "y": 70, "width": 300, "height": 310}
        self.after = self.before | {"x": 125, "y": 95}
        self.image = self.window(self.before)
        self.captured = self.window(self.after)

    def window(self, state, *, blank=False, stale=False):
        def rectangles(window):
            x, y = window["x"], window["y"]
            values = [((x, y, x + 300, y + 310), (242, 242, 242))]
            if not blank:
                values.extend(
                    ((x + dx, y + dy, x + dx + 17, y + dy + 19), (12, 120, 25))
                    for dx in range(25, 280, 31)
                    for dy in range(80, 290, 29)
                )
            return values

        shapes = rectangles(self.before) if stale else []
        return capture(self.metadata, shapes + rectangles(state))

    def test_content_and_vacated_pixels_follow_reported_move(self):
        result = translated(self.before, self.after, self.image, self.captured)
        self.assertGreater(result["content_edge_samples"], 30)
        self.assertGreater(result["translated_fraction"], 0.98)
        self.assertGreater(result["vacated_changed_fraction"], 0.98)

    def test_geometry_without_moving_pixels_fails(self):
        with self.assertRaisesRegex(ControlError, "content did not follow"):
            translated(self.before, self.after, self.image, self.image)

    def test_uniform_window_cannot_supply_content_proof(self):
        with self.assertRaisesRegex(ControlError, "content did not follow"):
            translated(
                self.before,
                self.after,
                self.window(self.before, blank=True),
                self.window(self.after, blank=True),
            )

    def test_correct_translation_still_rejects_stale_old_window(self):
        with self.assertRaisesRegex(ControlError, "stale pixels"):
            translated(
                self.before, self.after, self.image, self.window(self.after, stale=True)
            )

    def test_local_bounds_use_canvas_scale_not_output_dpi(self):
        window = {"x": -200, "y": 150, "content_width": 600, "content_height": 400}
        self.assertEqual(local_center((120, 180, 100, 40), window, 0.8), (-64, 310))
        for bounds in (
            (-1, 0, 20, 20),
            (0, 0, 0, 30),
            (590, 20, 20, 20),
            (10, 390, 30, 40),
        ):
            with self.subTest(bounds=bounds), self.assertRaises(ControlError):
                local_center(bounds, window, 1)

    def test_gtk3_shadow_relative_bounds_use_observed_header_origin(self):
        # Actual GTK 3 AT-SPI geometry from the scale-2 native run: the whole
        # surface is 572x479 while xdg window geometry is 520x427 at (26,23).
        window = {"x": 216, "y": 128, "content_width": 520, "content_height": 427}
        button = (50, 135, 472, 34)
        with self.assertRaisesRegex(ControlError, "outside"):
            local_center(button, window, 1)
        self.assertEqual(local_center(button, window, 1, (26, 23)), (476, 257))
        self.assertEqual(local_center(button, window, 0.8, (26, 23)), (424, 231.2))


if __name__ == "__main__":
    unittest.main()
