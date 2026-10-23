"""Shell zoom setup keeps the full Firefox marker available for pixel evidence."""

import sys
import unittest
from pathlib import Path
from unittest.mock import Mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib import firefox_assertions as assertions
from agentlib import firefox_shell
from agentlib.control import ControlError
from test_agent_visual import capture


class FirefoxShellMarkerTests(unittest.TestCase):
    def setUp(self):
        # Geometry from the native 1920x1080 failure, before the setup pan.
        self.entry_view = {"x": 99486, "y": 99863, "scale": 1}
        self.world = {"x": 99510, "y": 99959, "width": 1872, "height": 888}
        self.metadata = {
            "width": 1920,
            "height": 1080,
            "scale": 1,
            "logical_x": 0,
            "logical_y": 0,
        }
        self.window = {
            "canvas_x": 100000,
            "canvas_y": 100000,
            "content_width": 1872,
            "content_height": 888,
            "focused": True,
            "mapped": True,
            "visible": True,
            "maximized": True,
            "fullscreen": False,
            "focused_view": False,
            "output_coordinates": False,
            "presentation_output": None,
            "maximized_canvas_bounds": self.world,
        }

    def projected_capture(self, view):
        scale = view["scale"]
        x, y = (self.world[axis] - view[axis] for axis in ("x", "y"))
        window = self.window | {
            "x": round(x * scale),
            "y": round(y * scale),
            "width": round(self.world["width"] * scale),
            "height": round(self.world["height"] * scale),
        }
        # The fixture's 32px inset and native Firefox's observed 212px page offset.
        # Capture clips at the output boundary, just like the retained frames.
        marker = (
            (x + 32) * scale,
            (y + 212) * scale,
            (x + 32 + 320) * scale,
            (y + 212 + 96) * scale,
        )
        return window, capture(self.metadata, [(marker, (0, 255, 0))])

    def zoomed_view(self, view):
        # Observed 1 -> 0.8 zoom about the 1920x1080 output's center.
        return {"x": view["x"] - 240, "y": view["y"] - 135, "scale": 0.8}

    def test_setup_pan_preserves_full_marker_through_shell_zoom(self):
        smoke = Mock(target=7)
        view = self.entry_view.copy()
        smoke.control.status.side_effect = lambda: {"viewport": view.copy()}
        smoke.control.deadline.remaining.return_value = 10
        smoke.window.return_value = self.window

        def request(command):
            operation, *values = command.split()
            if operation == "zoom-to":
                view["scale"] = float(values[0])
            elif operation == "pan-to":
                view.update(x=float(values[0]), y=float(values[1]))
            else:
                self.fail(f"setup must not refocus or move Firefox: {command}")

        def until(name, condition):
            result = condition()
            self.assertTrue(result, name)
            return result

        smoke.control.request.side_effect = request
        smoke.until.side_effect = until
        before_view = firefox_shell.prepare_canvas_view(smoke)
        self.assertNotEqual(before_view["x"], self.entry_view["x"])
        self.assertNotEqual(before_view["y"], self.entry_view["y"])
        self.assertEqual(before_view["scale"], 1)
        after_view = self.zoomed_view(before_view)
        before, before_image = self.projected_capture(before_view)
        after, after_image = self.projected_capture(after_view)
        proof = assertions.verify_zoom(
            before_view, after_view, before, after, before_image, after_image
        )
        self.assertEqual(proof["before_marker_size"], [320, 96])
        self.assertEqual(proof["after_marker_size"], [256, 77])

    def test_original_right_pan_clips_marker_and_fails_pixel_ratio(self):
        before_view = {"x": 99566, "y": 99903, "scale": 1}
        after_view = self.zoomed_view(before_view)
        before, before_image = self.projected_capture(before_view)
        after, after_image = self.projected_capture(after_view)
        self.assertEqual((before["x"], before["y"]), (-56, 56))
        self.assertEqual((after["x"], after["y"]), (147, 153))
        self.assertEqual(assertions.marker_size(before_image, before), [296, 96])
        with self.assertRaisesRegex(ControlError, "marker did not scale"):
            assertions.verify_zoom(
                before_view, after_view, before, after, before_image, after_image
            )


if __name__ == "__main__":
    unittest.main()
