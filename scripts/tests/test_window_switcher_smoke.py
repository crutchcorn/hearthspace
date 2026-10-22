"""Reject premature focus, rearranged previews and unproven native input."""

import contextlib
import copy
import io
import json
import sys
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import Mock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib.control import ControlError
from agentlib.window_switcher_focused import retained, switched
from agentlib.window_switcher_smoke import WindowSwitcherSmoke
from agentlib.window_switcher_smoke_assertions import (
    held_motion,
    input_proof,
    previews,
    released_motion,
    selection,
)
from test_agent_visual import capture


def status(x=0, y=0, *, active=False, animating=False, focused=2, display=None):
    windows = []
    for identity, (canvas_x, canvas_y) in enumerate(
        ((-5000, -3000), (0, 0), (5000, 3000)), 1
    ):
        rect = {"x": canvas_x - x, "y": canvas_y - y, "width": 200, "height": 100}
        windows.append(
            {
                "id": identity,
                "focused": identity == focused,
                "mapped": True,
                "canvas_x": canvas_x,
                "canvas_y": canvas_y,
                "content_width": 200,
                "content_height": 100,
                "maximized": False,
                "fullscreen": False,
                "focused_view": False,
                "presentation_output": None,
                **(display or {}).get(identity, rect),
            }
        )
    return {
        "viewport": {"x": x, "y": y, "scale": 1, "animating": False},
        "window_switcher": {
            "active": active,
            "animating": animating,
            "selected_window_id": 3,
            "window_ids": [2, 3, 1] if active else [],
            "output": "test" if active else None,
            "scene": [
                {"window_id": identity, "display": rect}
                for identity, rect in (display or {}).items()
            ],
        },
        "windows": windows,
        "overview": {"active": False},
        "outputs": [
            {
                "name": "test",
                "logical_x": 0,
                "logical_y": 0,
                "logical_width": 1200,
                "logical_height": 800,
            }
        ],
    }


def preview_rects(offset=0):
    return {
        identity: {"x": identity * 200 + offset, "y": 200, "width": 180, "height": 90}
        for identity in (1, 2, 3)
    }


class WindowSwitcherEvidenceTests(unittest.TestCase):
    def test_focused_swipe_retains_focus_until_cancel_or_commit(self):
        before = status(active=True, display=preview_rects())
        focused = before["windows"][1]
        focused.update(
            focused_view=True,
            maximized=True,
            content_height=756,
            content_width=1200,
            presentation_output="test",
        )
        retained(before, copy.deepcopy(before), 2)
        for mutation in ("focused_view", "focused", "maximized", "fullscreen"):
            invalid = copy.deepcopy(before)
            invalid["windows"][1][mutation] = not focused[mutation]
            with self.subTest(mutation=mutation), self.assertRaises(ControlError):
                retained(before, invalid, 2)
        invalid = copy.deepcopy(before)
        invalid["viewport"]["x"] = 1
        with self.assertRaises(ControlError):
            retained(before, invalid, 2)

        after = status(4500, 2650, focused=3)
        after["windows"][1].update(
            maximized=True, content_width=1152, content_height=564
        )
        switched(before, after, 2, 3)
        for mutation in ("focused_view", "fullscreen", "maximized", "canvas_x"):
            invalid = copy.deepcopy(after)
            invalid["windows"][1][mutation] = (
                1 if mutation == "canvas_x" else not invalid["windows"][1][mutation]
            )
            with self.subTest(mutation=mutation), self.assertRaises(ControlError):
                switched(before, invalid, 2, 3)
        invalid = copy.deepcopy(after)
        invalid["windows"][0]["content_width"] += 1
        with self.assertRaises(ControlError):
            switched(before, invalid, 2, 3)

    def test_opening_requires_live_preview_motion_while_focus_and_viewport_stay_put(
        self,
    ):
        before = status()
        after = status(active=True, display=preview_rects())
        middle = status(active=True, animating=True, display=preview_rects(100))
        proof = held_motion(before, [before, middle, after], after)
        self.assertEqual(len(proof["intermediate_previews"]), 1)
        for samples in (
            [],
            [before, after],
            [status(active=True, display=preview_rects(100))],
        ):
            with (
                self.subTest(samples=samples),
                self.assertRaisesRegex(ControlError, "intermediate"),
            ):
                held_motion(before, samples, after)
        for mutation in ("viewport", "focus", "geometry", "order"):
            invalid = copy.deepcopy(middle)
            if mutation == "viewport":
                invalid["viewport"]["x"] = 50
            elif mutation == "focus":
                invalid["windows"][1]["focused"] = False
                invalid["windows"][2]["focused"] = True
            elif mutation == "geometry":
                invalid["windows"][0]["canvas_x"] += 1
            else:
                invalid["window_switcher"]["window_ids"] = [3, 2, 1]
            with self.subTest(mutation=mutation), self.assertRaises(ControlError):
                held_motion(before, [invalid], after)

    def test_cycling_changes_only_selection_in_the_same_fixed_grid(self):
        before = status(active=True, display=preview_rects())
        after = copy.deepcopy(before)
        after["window_switcher"]["selected_window_id"] = 1
        self.assertIn("stationary_grid", held_motion(before, [before, after], after))
        invalid = status(active=True, animating=True, display=preview_rects(20))
        with self.assertRaisesRegex(ControlError, "rearranged"):
            held_motion(before, [invalid], after)

    def test_release_centers_and_focuses_selected_window_with_live_reveal_motion(self):
        before = status(active=True, display=preview_rects())
        after = status(4500, 2650, focused=3)
        middle = status(
            4500, 2650, focused=3, animating=True, display=preview_rects(70)
        )
        self.assertTrue(
            released_motion(before, [middle, after], after, 3)["intermediate_previews"]
        )
        with self.assertRaisesRegex(ControlError, "intermediate"):
            released_motion(before, [after], after, 3)
        for mutation in ("center", "zoom", "focus"):
            invalid = copy.deepcopy(after)
            if mutation == "center":
                invalid["windows"][2]["x"] += 30
            elif mutation == "zoom":
                invalid["viewport"]["scale"] = 0.8
            else:
                invalid["windows"][2]["focused"] = False
            with self.subTest(mutation=mutation), self.assertRaises(ControlError):
                released_motion(before, [middle], invalid, 3)

    def test_held_selection_is_separate_from_actual_focus_and_never_opens_overview(
        self,
    ):
        state = status(active=True)
        selection(state, 3, active=True, focused=2)
        for section, key, value in (
            ("window_switcher", "active", False),
            ("window_switcher", "selected_window_id", 2),
            ("overview", "active", True),
        ):
            invalid = copy.deepcopy(state)
            invalid[section][key] = value
            with self.subTest(change=(section, key)), self.assertRaises(ControlError):
                selection(invalid, 3, active=True, focused=2)
        state["windows"][1]["focused"] = False
        state["windows"][2]["focused"] = True
        with self.assertRaisesRegex(ControlError, "actual client focus"):
            selection(state, 3, active=True, focused=2)

    def test_native_preview_requires_each_clients_own_pixels_inside_its_card(self):
        metadata = {
            "width": 400,
            "height": 300,
            "scale": 1,
            "logical_x": 0,
            "logical_y": 0,
            "output": "test",
        }
        item = {
            "window_id": 1,
            "x": 50,
            "y": 50,
            "width": 100,
            "height": 80,
            "preview_x": 60,
            "preview_y": 60,
            "preview_width": 80,
            "preview_height": 50,
        }
        state = {
            "window_switcher": {"active": True, "output": "test", "items": [item]},
            "outputs": [
                {
                    "name": "test",
                    "logical_x": 0,
                    "logical_y": 0,
                    "logical_width": 400,
                    "logical_height": 300,
                }
            ],
            "windows": [{"id": 1, "mapped": True}],
        }
        color = (0, 255, 0)
        image = capture(metadata, [((60, 60, 80, 70), color)])
        self.assertGreater(previews(state, image, {1: color})["1"]["marker_pixels"], 20)
        chrome_only = capture(metadata, [((50, 50, 60, 60), color)])
        with self.assertRaisesRegex(ControlError, "marker"):
            previews(state, chrome_only, {1: color})

    def test_touchpad_proof_requires_the_requested_single_step_and_successful_end(self):
        path = Mock()
        for dx, ending, accepted in (
            (-64, "", True),
            (64, "", False),
            (-5, "", False),
            (-140, "", False),
            (-64, " cancelled", False),
        ):
            path.read_text.return_value = (
                " event17 GESTURE_SWIPE_BEGIN +1s 4\n"
                f" event17 GESTURE_SWIPE_UPDATE +2s 4 {dx}/0 ({dx}/0 unaccelerated)\n"
                f" event17 GESTURE_SWIPE_END +3s 4{ending}\n"
            )
            with self.subTest(dx=dx, ending=ending):
                if accepted:
                    self.assertEqual(
                        input_proof(path, "touchpad", 4, -1)["translation"], [-64, 0]
                    )
                else:
                    with self.assertRaises(ControlError):
                        input_proof(path, "touchpad", 4, -1)

    def test_touchscreen_proof_requires_matching_contact_count_and_device_cleanup(self):
        for fingers in (4, 5):
            trace = "".join(f" event5 TOUCH_DOWN +1s {i}\n" for i in range(fingers))
            trace += " event5 TOUCH_MOTION +2s 0\n event5 TOUCH_UP +3s 0\n event5 TOUCH_FRAME +3s\n"
            path = Mock()
            path.read_text.return_value = trace
            with self.assertRaisesRegex(ControlError, "DEVICE_REMOVED"):
                input_proof(path, "touchscreen", fingers, -1)
            path.read_text.return_value += " event5 DEVICE_REMOVED +3s\n"
            self.assertEqual(
                input_proof(path, "touchscreen", fingers, -1)["contacts"], fingers
            )
            with self.assertRaisesRegex(ControlError, "contact count"):
                input_proof(path, "touchscreen", fingers + 1, -1)

    def test_failure_still_stops_owned_session_and_verifies_restoration(self):
        with (
            tempfile.TemporaryDirectory() as directory,
            contextlib.redirect_stdout(io.StringIO()),
        ):
            smoke = WindowSwitcherSmoke(
                SimpleNamespace(
                    session=Path(directory),
                    timeout=30,
                    total_timeout=120,
                    log_level="info",
                    device="both",
                )
            )
            operations = []

            def launcher(operation, *extra):
                operations.append(operation)
                (smoke.directory / "session.json").write_text(
                    json.dumps({"cleanup": {"complete": operation == "stop"}})
                )

            with (
                patch("agentlib.daily.shutil.which", return_value="/test"),
                patch.object(smoke, "launcher", side_effect=launcher),
                patch.object(
                    smoke, "workflow", side_effect=ControlError("switcher proof failed")
                ),
                patch.object(smoke, "verify_restored") as restored,
                self.assertRaisesRegex(ControlError, "switcher proof failed"),
            ):
                smoke.run()
            self.assertEqual(operations, ["start", "stop"])
            restored.assert_called_once()


if __name__ == "__main__":
    unittest.main()
