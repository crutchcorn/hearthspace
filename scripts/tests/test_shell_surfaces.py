"""Independent frame members retain strict corner and native-coordinate checks."""

import copy
import sys
import unittest
from pathlib import Path
from unittest.mock import Mock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib import shell_checks
from agentlib.control import ControlError
from agentlib.shell_surfaces import CONTROL_WIDGETS, corner_surface, widget_surface


def frame_status():
    output = {
        "name": "test",
        "logical_x": 0,
        "logical_y": 0,
        "logical_width": 1000,
        "logical_height": 600,
        "scale": 2,
    }
    rects = {
        "notifications": {"x": 24, "y": 24, "width": 100, "height": 54},
        "tools": {"x": 24, "y": 522, "width": 246, "height": 54},
        "apps": {"x": 876, "y": 522, "width": 100, "height": 54},
        "volume": {"x": 832, "y": 29, "width": 44, "height": 44},
        "brightness": {"x": 880, "y": 29, "width": 44, "height": 44},
        "power": {"x": 928, "y": 29, "width": 44, "height": 44},
    }
    layout = {
        "output": "test",
        "widgets": [{"id": name, "rect": rect} for name, rect in rects.items()],
        "groups": [
            {
                "id": "system",
                "rect": {
                    "x": 776,
                    "y": 24,
                    "width": 200,
                    "height": 54,
                },
            }
        ],
    }
    return {
        "outputs": [output],
        "shell_corners": [],
        "frame_layout": {"outputs": [layout]},
    }


def smoke_with(status):
    smoke = Mock()
    smoke.control.status.return_value = status
    smoke.control.query.return_value = status["frame_layout"]
    smoke.until.side_effect = lambda _name, condition: condition()
    return smoke


class FrameSurfaceTests(unittest.TestCase):
    def test_click_uses_member_origin_and_44px_native_frame_not_group_backing(self):
        smoke = smoke_with(frame_status())
        snapshot = {
            "elements": [
                {
                    "role": "Frame",
                    "bounds": {"x": 0, "y": 0, "width": 88, "height": 88},
                },
                {
                    "role": "Button",
                    "name": "Sound settings",
                    "bounds": {
                        "x": 8,
                        "y": 8,
                        "width": 72,
                        "height": 72,
                    },
                },
            ]
        }
        stale = copy.deepcopy(snapshot)
        stale["elements"][0]["bounds"]["height"] = 108
        with (
            patch.object(shell_checks, "a11y", side_effect=[stale, snapshot]),
            patch.object(shell_checks, "Input") as inputs,
        ):

            def until(_name, condition):
                self.assertIsNone(condition(), "stale aggregate bounds must wait")
                inputs.assert_not_called()
                return condition()

            smoke.until.side_effect = until
            shell_checks.click(smoke, "Sound settings")
        inputs.return_value.__enter__.return_value.click.assert_called_once_with(
            854, 51
        )

    def test_default_parts_still_prove_all_four_corner_purposes(self):
        smoke = smoke_with(frame_status())
        buttons = [
            {
                "name": name,
                "bounds": {
                    "x": 8,
                    "y": 8,
                    "width": 72,
                    "height": 72,
                },
            }
            for name in shell_checks.BAR_ICONS
        ]
        with (
            patch.object(shell_checks, "closed"),
            patch.object(
                shell_checks,
                "a11y",
                side_effect=[
                    {"elements": buttons},
                    {"elements": []},
                ],
            ),
        ):
            shell_checks.bar(smoke)
        corners = smoke.record.call_args.kwargs["corners"]
        self.assertEqual(
            [item["corner"] for item in corners],
            [
                "notifications",
                "system",
                "canvas",
                "apps",
            ],
        )

    def test_member_bounds_must_not_be_validated_against_larger_group(self):
        smoke = smoke_with(frame_status())
        buttons = [
            {
                "name": name,
                "bounds": {
                    "x": 88 if name == "Sound settings" else 8,
                    "y": 8,
                    "width": 72,
                    "height": 72,
                },
            }
            for name in shell_checks.BAR_ICONS
        ]
        smoke.until.return_value = buttons
        smoke.until.side_effect = None
        with (
            patch.object(shell_checks, "closed"),
            self.assertRaisesRegex(ControlError, "outside its shell surface"),
        ):
            shell_checks.bar(smoke)

    def test_missing_duplicate_hidden_and_wrong_output_fail_closed(self):
        for problem in ("missing", "duplicate", "hidden", "wrong output"):
            with self.subTest(problem=problem):
                status = frame_status()
                layout = status["frame_layout"]["outputs"][0]
                if problem == "missing":
                    layout["widgets"] = []
                elif problem == "duplicate":
                    layout["widgets"].append(layout["widgets"][0].copy())
                elif problem == "hidden":
                    layout["widgets"][0]["hidden"] = True
                else:
                    layout["output"] = "another-output"
                with self.assertRaises(ControlError):
                    widget_surface(status, "notifications")

    def test_system_purpose_uses_group_rect_while_buttons_use_members(self):
        status = frame_status()
        self.assertEqual(corner_surface(status, "system")["height"], 54)
        self.assertEqual(
            widget_surface(status, CONTROL_WIDGETS["Sound settings"])["height"], 44
        )


if __name__ == "__main__":
    unittest.main()
