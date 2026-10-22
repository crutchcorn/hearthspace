"""Require presented client pixels and preserved geometry in window previews."""

import copy
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib.control import ControlError
from agentlib.window_preview_assertions import (
    COLORS,
    marker_count,
    presented,
    preserved,
)
from test_agent_visual import capture


class WindowPreviewPixelTests(unittest.TestCase):
    def setUp(self):
        self.metadata = {
            "width": 400,
            "height": 300,
            "scale": 2,
            "logical_x": -20,
            "logical_y": 10,
            "output": "test",
        }
        self.items = [
            {"window_id": 1, "x": 0, "y": 30, "width": 60, "height": 80},
            {"window_id": 2, "x": 80, "y": 30, "width": 60, "height": 80},
        ]
        for item in self.items:
            item.update(
                preview_x=item["x"], preview_y=40, preview_width=60, preview_height=50
            )
        self.status = {
            "viewport": {"x": 15, "y": -20, "scale": 0.7},
            "outputs": [
                {
                    "name": "test",
                    "logical_x": -20,
                    "logical_y": 10,
                    "logical_width": 200,
                    "logical_height": 150,
                }
            ],
            "window_switcher": {"active": True, "output": "test", "items": self.items},
            "windows": [
                {
                    "id": n,
                    "mapped": True,
                    "canvas_x": 100 * n,
                    "canvas_y": 300,
                    "content_width": 900,
                    "content_height": 600,
                    "maximized": False,
                    "fullscreen": n == 1,
                    "presentation_output": "test" if n == 1 else None,
                }
                for n in (1, 2)
            ],
        }
        self.targets = {1: COLORS[0], 2: COLORS[1]}
        self.image = capture(
            self.metadata,
            [((10, 50, 30, 60), COLORS[0]), ((90, 50, 110, 60), COLORS[1])],
        )

    def test_each_thumbnail_requires_its_own_presented_client_marker(self):
        self.assertEqual(
            set(presented(self.status, self.image, self.targets)), {"1", "2"}
        )
        stale = capture(self.metadata, [((10, 50, 30, 60), COLORS[0])])
        with self.assertRaisesRegex(ControlError, "marker"):
            presented(self.status, stale, self.targets)
        swapped = capture(
            self.metadata,
            [((10, 50, 30, 60), COLORS[1]), ((90, 50, 110, 60), COLORS[0])],
        )
        with self.assertRaisesRegex(ControlError, "marker"):
            presented(self.status, swapped, self.targets)

    def test_marker_measurement_respects_output_origin_scale_and_bounds(self):
        self.assertEqual(marker_count(self.image, (10, 50, 30, 60), COLORS[0]), 800)
        self.assertEqual(marker_count(self.image, (90, 50, 110, 60), COLORS[0]), 0)

    def test_color_in_card_chrome_cannot_prove_client_preview(self):
        self.items[0]["preview_y"] = 70
        self.items[0]["preview_height"] = 20
        with self.assertRaisesRegex(ControlError, "marker"):
            presented(self.status, self.image, self.targets)

    def test_overlapping_off_output_or_duplicate_thumbnails_cannot_pass(self):
        for change in ({"x": 59}, {"x": -21}, {"width": 0}, {"window_id": 1}):
            state = copy.deepcopy(self.status)
            state["window_switcher"]["items"][1].update(change)
            with self.subTest(change=change), self.assertRaises(ControlError):
                presented(state, self.image, self.targets)

    def test_missing_window_and_wrong_output_cannot_pass(self):
        for change in (
            {"items": self.items[:1]},
            {"active": False},
            {"output": "other"},
        ):
            state = copy.deepcopy(self.status)
            state["window_switcher"].update(change)
            with self.subTest(change=change), self.assertRaises(ControlError):
                presented(state, self.image, self.targets)

    def test_presentation_geometry_and_viewport_survive_open_and_cancel(self):
        preserved(self.status, copy.deepcopy(self.status))
        for field, value in (
            ("canvas_x", 50),
            ("fullscreen", False),
            ("content_width", 300),
        ):
            state = copy.deepcopy(self.status)
            state["windows"][0][field] = value
            with (
                self.subTest(field=field),
                self.assertRaisesRegex(ControlError, "mutated"),
            ):
                preserved(self.status, state)
        state = copy.deepcopy(self.status)
        state["viewport"]["scale"] = 1
        with self.assertRaisesRegex(ControlError, "viewport"):
            preserved(self.status, state)
        preserved(self.status, state, viewport=False)


if __name__ == "__main__":
    unittest.main()
