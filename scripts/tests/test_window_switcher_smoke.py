"""Reject semantic-only switching, snap animations and unproven gesture injection."""

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
from agentlib.window_switcher_smoke import WindowSwitcherSmoke
from agentlib.window_switcher_smoke_assertions import (
    input_proof,
    previews,
    selection,
    smooth_pan,
)
from test_agent_visual import capture


def status(x, y, *, animating=False):
    return {"viewport": {"x": x, "y": y, "scale": 1, "animating": animating}}


class WindowSwitcherEvidenceTests(unittest.TestCase):
    def test_animation_requires_an_interior_view_and_unchanged_zoom(self):
        before, after = status(0, 100), status(100, -100)
        middle = status(60, -20, animating=True)
        self.assertEqual(
            smooth_pan(before, [before, middle, after], after)["intermediate_views"],
            [middle["viewport"]],
        )
        for samples in (
            [],
            [before, after],
            [status(50, 0)],
            [status(200, 0, animating=True)],
        ):
            with (
                self.subTest(samples=samples),
                self.assertRaisesRegex(ControlError, "intermediate"),
            ):
                smooth_pan(before, samples, after)
        with self.assertRaisesRegex(ControlError, "never panned"):
            smooth_pan(before, [middle], before)
        after["viewport"]["scale"] = 0.8
        with self.assertRaisesRegex(ControlError, "zoom"):
            smooth_pan(before, [middle], after)

    def test_held_selection_must_agree_with_actual_focus_and_not_open_overview(self):
        state = {
            "window_switcher": {"active": True, "selected_window_id": 1},
            "overview": {"active": False},
            "windows": [{"id": 1, "focused": True}, {"id": 2, "focused": False}],
        }
        selection(state, 1, active=True)
        for section, key, value in (
            ("window_switcher", "active", False),
            ("window_switcher", "selected_window_id", 2),
            ("overview", "active", True),
        ):
            invalid = copy.deepcopy(state)
            invalid[section][key] = value
            with self.subTest(change=(section, key)), self.assertRaises(ControlError):
                selection(invalid, 1, active=True)
        state["windows"][0]["focused"] = False
        with self.assertRaisesRegex(ControlError, "actual client focus"):
            selection(state, 1, active=True)

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
