"""Independent device evidence for persistent switcher pinch acceptance."""

import sys
import unittest
from pathlib import Path
from unittest.mock import Mock, call, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib import window_switcher_pinch_input as pinch_input
from agentlib.control import ControlError


class SwitcherPinchInputTests(unittest.TestCase):
    def test_touchscreen_five_contacts_can_arrive_as_a_staggered_chord(self):
        device = Mock(kind="touchscreen")
        state = pinch_input.begin(device, 5, stagger=True)
        self.assertEqual(len(state["points"]), 5)
        self.assertEqual(
            device.request.call_args_list[0],
            call("begin", fingers=1, points=state["points"][:1]),
        )
        self.assertEqual(device.request.call_count, 5)
        pinch_input.move(device, state, 0.6)
        self.assertEqual(len(state["moved"]), 5)

    def test_outward_touchpad_uses_room_to_expand_all_five_contacts(self):
        device = Mock(kind="touchpad")
        state = pinch_input.begin(device, 5, outward=True)
        pinch_input.move(device, state, 1.5)
        self.assertEqual(
            device.request.call_args_list,
            [call("begin", fingers=5, distance=1000), call("move", distance=1500)],
        )

    def test_subthreshold_fixture_moves_tracked_finger_past_detection_threshold(self):
        for outward, ratio in ((False, 0.85), (True, 1.24)):
            device = Mock(kind="touchpad")
            state = pinch_input.begin(device, 5, outward=outward)
            # Five fingers require 3 mm of motion, plus room for hysteresis.
            topmost_motion_mm = abs(state["width"] * (ratio - 1)) / 3 / 20
            self.assertGreaterEqual(topmost_motion_mm, 3.9)

    def test_trace_checks_classified_scale_threshold_not_physical_contact_ratio(self):
        path = Mock()
        for requested, observed in (
            (0.6, 0.72),
            (0.85, 0.96),
            (1.24, 1.10),
            (1.5, 1.35),
        ):
            path.read_text.return_value = (
                " event17 GESTURE_PINCH_BEGIN +1s 5\n"
                f" event17 GESTURE_PINCH_UPDATE +2s 5 0/0 (0/0 unaccelerated) {observed} @ 0\n"
                " event17 GESTURE_PINCH_END +3s 5\n"
            )
            proof = pinch_input.trace_proof(path, "touchpad", 5, ratio=requested)
            self.assertEqual(proof["requested_ratio"], requested)
            self.assertEqual(proof["scale"], observed)
            wrong_request = 1.5 if requested < 1 else 0.6
            with self.assertRaisesRegex(ControlError, "threshold"):
                pinch_input.trace_proof(path, "touchpad", 5, ratio=wrong_request)

    def test_hold_only_trace_cannot_prove_a_subthreshold_pinch(self):
        path = Mock()
        path.read_text.return_value = (
            " event17 GESTURE_HOLD_BEGIN +1s 4\n event17 GESTURE_HOLD_END +2s 4\n"
        )
        with self.assertRaisesRegex(ControlError, "updated 4-finger PINCH"):
            pinch_input.trace_proof(path, "touchpad", 4, ratio=0.85)

    def test_contact_change_cancels_only_supported_fixture_count(self):
        device = Mock(kind="touchscreen")
        pinch_input.cancel(
            device, {"fingers": 4, "moved": [[0, 0], [2, 0], [2, 2], [0, 2]]}
        )
        device.request.assert_called_once_with("add-finger", point=(1, 1))
        with self.assertRaises(ControlError):
            pinch_input.cancel(device, {"fingers": 5})

    def test_touchpad_cancel_waits_for_observed_cancel_before_caller_can_end(self):
        device = Mock(kind="touchpad")
        path = Mock()
        active = (
            " event17 GESTURE_PINCH_BEGIN +1s 4\n"
            " event17 GESTURE_PINCH_UPDATE +2s 4 0/0 (0/0 unaccelerated) 0.6 @ 0\n"
        )
        path.read_text.side_effect = [
            active,
            active + " event17 GESTURE_PINCH_END +3s 4 cancelled\n",
        ]
        with patch("agentlib.window_switcher_pinch_input.time.sleep") as sleep:
            pinch_input.cancel(device, {"fingers": 4}, trace_path=path)
        self.assertEqual(path.read_text.call_count, 2)
        sleep.assert_called_once()
        device.request.assert_called_once_with("lift")
        path.read_text.side_effect = None
        path.read_text.return_value = active + " event17 GESTURE_PINCH_END +3s 4\n"
        with self.assertRaisesRegex(ControlError, "ended normally"):
            pinch_input.cancel(device, {"fingers": 4}, trace_path=path)

    def test_four_contact_cancel_proof_survives_following_three_contact_end(self):
        path = Mock()
        path.read_text.return_value = (
            " event17 GESTURE_PINCH_BEGIN +1s 4\n"
            " event17 GESTURE_PINCH_UPDATE +2s 4 0/0 (0/0 unaccelerated) 0.6 @ 0\n"
            " event17 GESTURE_PINCH_END +3s 4 cancelled\n"
            " event17 GESTURE_HOLD_BEGIN +4s 3\n"
            " event17 GESTURE_HOLD_END +5s 3\n"
        )
        proof = pinch_input.trace_proof(path, "touchpad", 4, cancelled=True, ratio=0.6)
        self.assertTrue(proof["cancelled"])
        self.assertEqual(proof["fingers"], 4)

    def test_touchpad_cancel_respects_aggregate_deadline(self):
        path = Mock()
        path.read_text.return_value = (
            " event17 GESTURE_PINCH_BEGIN +1s 4\n"
            " event17 GESTURE_PINCH_UPDATE +2s 4 0/0 (0/0 unaccelerated) 0.6 @ 0\n"
        )
        deadline = Mock()
        deadline.remaining.side_effect = ControlError("operation deadline exceeded")
        with self.assertRaisesRegex(ControlError, "deadline exceeded"):
            pinch_input.cancel(
                Mock(kind="touchpad"),
                {"fingers": 4},
                trace_path=path,
                deadline=deadline,
            )

    def test_touch_trace_requires_exact_four_or_five_contact_count(self):
        for fingers in (4, 5):
            trace = "".join(f" event5 TOUCH_DOWN +1s {i}\n" for i in range(fingers))
            trace += " event5 TOUCH_MOTION +2s 0\n event5 TOUCH_UP +3s 0\n event5 TOUCH_FRAME +3s\n"
            path = Mock()
            path.read_text.return_value = trace
            self.assertEqual(
                pinch_input.trace_proof(path, "touchscreen", fingers)["contacts"],
                fingers,
            )
            with self.assertRaisesRegex(ControlError, "contact count"):
                pinch_input.trace_proof(path, "touchscreen", fingers, cancelled=True)


if __name__ == "__main__":
    unittest.main()
