"""Drawing must use one settled native popup and capture generation."""

import copy
import sys
import unittest
from pathlib import Path
from unittest.mock import Mock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib import favorite_geometry


def status(height=414, generation=6):
    return {
        "outputs": [{"name": "test", "scale": 2}],
        "shell_panels": [
            {
                "kind": "search",
                "output": "test",
                "x": 618,
                "y": 710 - height,
                "width": 650,
                "height": height,
            }
        ],
        "favorite_gesture": {
            "armed": True,
            "output": "test",
            "generation": generation,
            "bounds": [40, 160, 570, 176],
        },
    }


def native(height=414, pad_y=160):
    return {
        "elements": [
            {
                "role": "Frame",
                "bounds": {"x": 0, "y": 0, "width": 1300, "height": height * 2},
            },
            {
                "role": "Canvas",
                "bounds": {"x": 80, "y": pad_y * 2, "width": 1140, "height": 352},
            },
            {"role": "Label", "bounds": {"x": 50, "y": 714, "width": 0, "height": 88}},
            {"role": "Button", "name": "Settings"},
            {"role": "Label", "name": "Draw a gesture"},
        ]
    }


class FavoriteGeometryTests(unittest.TestCase):
    def test_requires_both_controls_in_consecutive_geometry_observations(self):
        missing_settings = native()
        missing_settings["elements"] = [
            item
            for item in missing_settings["elements"]
            if item.get("name") != "Settings"
        ]
        missing_prompt = native()
        missing_prompt["elements"] = [
            item
            for item in missing_prompt["elements"]
            if item.get("name") != "Draw a gesture"
        ]
        wrong_settings_role = native()
        wrong_settings_role["elements"][-2]["role"] = "Label"
        wrong_prompt_name = native()
        wrong_prompt_name["elements"][-1]["name"] = "Draw a gesture later"
        ready = native()
        ready["elements"][-2]["role"] = "ToggleButton"
        ready["elements"][-1]["role"] = "Static"
        for label, invalid in (
            ("missing Settings", missing_settings),
            ("missing drawing prompt", missing_prompt),
            ("Settings is not a button", wrong_settings_role),
            ("inexact drawing prompt", wrong_prompt_name),
        ):
            with self.subTest(label=label):
                smoke = Mock()
                smoke.control.status.return_value = status()
                with patch.object(favorite_geometry, "a11y") as observe:

                    def until(_name, condition, invalid=invalid):
                        for elements in (ready, invalid, ready):
                            observe.return_value = elements
                            self.assertIsNone(condition())
                        observe.return_value = ready
                        return condition()

                    smoke.until.side_effect = until
                    self.assertEqual(
                        favorite_geometry.drawing_rect(smoke), [658, 456, 570, 176]
                    )
                self.assertEqual(observe.call_count, 4)

    def test_waits_for_resize_generation_and_native_pad_before_drawing(self):
        smoke = Mock()
        observations = [
            # A response crossed a capture-generation update.
            (status(generation=5), status(), native()),
            # A native configure has not yet reached the client.
            (status(), status(), native(height=544)),
            # The client pad and compositor capture disagree.
            (status(), status(), native(pad_y=140)),
            # New content is still inside the old, larger popup allocation.
            (status(height=544), status(height=544), native(height=544)),
            (status(), status(), native()),
            (status(), status(), native()),
        ]
        with patch.object(favorite_geometry, "a11y") as observe:

            def until(_name, condition):
                for index, (before, after, elements) in enumerate(observations):
                    smoke.control.status.side_effect = [
                        copy.deepcopy(before),
                        copy.deepcopy(after),
                    ]
                    observe.return_value = elements
                    result = condition()
                    if index < len(observations) - 1:
                        self.assertIsNone(result)
                return result

            smoke.until.side_effect = until
            self.assertEqual(
                favorite_geometry.drawing_rect(smoke), [658, 456, 570, 176]
            )
        self.assertEqual(smoke.record.call_args.kwargs["generation"], 6)
        self.assertEqual(smoke.record.call_args.kwargs["surface"]["height"], 414)


if __name__ == "__main__":
    unittest.main()
