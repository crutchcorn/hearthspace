import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib.control import ControlError
from agentlib.visual import pixel_rect, verify_cursor_motion, verify_move, verify_resize
from test_agent_png import png


def capture(metadata, rectangles):
    width, height = metadata["width"], metadata["height"]
    rows = [bytearray(bytes([90, 80, 70]) * width) for _ in range(height)]
    for bounds, color in rectangles:
        left, top, right, bottom = map(int, pixel_rect(metadata, bounds))
        for row in rows[max(0, top) : min(height, bottom)]:
            for x in range(max(0, left), min(width, right)):
                row[x * 3 : x * 3 + 3] = bytes(color)
    return metadata, png(b"".join(b"\0" + row for row in rows), width, height)


def window_capture(metadata, window, *, landmark=None):
    x, y, width, height = (window[key] for key in ("x", "y", "width", "height"))
    marker = landmark or (x + 5, y + 10, x + 15, y + 15)
    return capture(
        metadata, [((x, y, x + width, y + height), (10, 10, 10)), (marker, (0, 255, 0))]
    )


class VisualTests(unittest.TestCase):
    def setUp(self):
        self.metadata = {
            "width": 281,
            "height": 281,
            "scale": 2,
            "logical_x": -50,
            "logical_y": 20,
            "logical_width": 141,
            "logical_height": 141,
        }
        self.window = {
            "x": -40,
            "y": 30,
            "width": 100,
            "height": 90,
            "titlebar_height": 5,
        }

    def test_move_requires_marker_to_follow_reported_geometry(self):
        moved = {**self.window, "x": -45, "y": 35}
        before = window_capture(self.metadata, self.window)
        after = window_capture(self.metadata, moved)
        verify_move(before, after, self.window, moved)
        with self.assertRaises(ControlError):
            verify_move(before, before, self.window, moved)

    def test_green_elsewhere_cannot_verify_moved_window(self):
        moved = {**self.window, "x": -45, "y": 35}
        unrelated = window_capture(self.metadata, moved, landmark=(70, 130, 80, 140))
        with self.assertRaisesRegex(ControlError, "outside"):
            verify_move(
                window_capture(self.metadata, self.window),
                unrelated,
                self.window,
                moved,
            )

    def test_resize_requires_rendered_background_in_vacated_strip(self):
        resized = {**self.window, "width": 60, "height": 70}
        background = capture(self.metadata, [])
        before = window_capture(self.metadata, self.window)
        after = window_capture(self.metadata, resized)
        verify_resize(background, before, after, self.window, resized)
        # The old marker still lies inside the new bounds; only the pixel
        # assertion distinguishes this stale framebuffer from a real resize.
        with self.assertRaisesRegex(ControlError, "background"):
            verify_resize(background, before, before, self.window, resized)

    def test_software_cursor_must_change_old_and_new_regions(self):
        metadata = {
            "width": 320,
            "height": 240,
            "scale": 1,
            "logical_x": 0,
            "logical_y": 0,
            "cursor": "embedded",
            "cursor_source": "framebuffer",
        }
        before = capture(metadata, [((100, 100, 102, 110), (255, 255, 255))])
        after = capture(metadata, [((170, 100, 172, 110), (255, 255, 255))])
        verify_cursor_motion(before, after, 100, 100)
        for missing in (before, capture(metadata, [])):
            with self.assertRaises(ControlError):
                verify_cursor_motion(before, missing, 100, 100)
        with self.assertRaisesRegex(ControlError, "framebuffer"):
            verify_cursor_motion(
                before,
                ({**metadata, "cursor_source": "hardware-overlay"}, after[1]),
                100,
                100,
            )


if __name__ == "__main__":
    unittest.main()
