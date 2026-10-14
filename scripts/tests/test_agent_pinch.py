"""Ensure native pinch acceptance rejects stale pixels and incorrect anchoring."""

import sys
import unittest
from pathlib import Path
from unittest.mock import Mock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib.control import ControlError
from agentlib.pinch_assertions import verify_removal, verify_zoom
from agentlib.touchpad import TouchDevice, contact_positions, require_owned_vt
from test_agent_visual import window_capture


class PinchAssertionTests(unittest.TestCase):
    def setUp(self):
        self.metadata = {
            "width": 320,
            "height": 240,
            "scale": 1,
            "logical_x": 0,
            "logical_y": 0,
        }
        self.before = {"x": 0, "y": 0, "scale": 1}
        self.after = {"x": -100, "y": -80, "scale": 0.5}
        self.old_window = {"x": 20, "y": 20, "width": 200, "height": 160}
        self.new_window = {"x": 60, "y": 50, "width": 100, "height": 80}
        self.old_image = window_capture(
            self.metadata, self.old_window, landmark=(40, 40, 80, 60)
        )
        self.new_image = window_capture(
            self.metadata, self.new_window, landmark=(70, 60, 90, 70)
        )

    def verify(self, *, viewport=None, image=None):
        return verify_zoom(
            self.before,
            viewport or self.after,
            (100, 80),
            self.old_window,
            self.new_window,
            self.old_image,
            image or self.new_image,
        )

    def test_asserts_real_marker_scale_and_anchor(self):
        self.assertEqual(self.verify()["scale_ratio"], 0.5)

    def test_rejects_changed_zoom_with_wrong_anchor(self):
        with self.assertRaisesRegex(ControlError, "anchor"):
            self.verify(viewport={**self.after, "x": -90})

    def test_rejects_correct_status_with_unscaled_marker(self):
        unscaled = window_capture(
            self.metadata, self.new_window, landmark=(70, 60, 110, 80)
        )
        with self.assertRaisesRegex(ControlError, "marker"):
            self.verify(image=unscaled)

    def test_injector_rejects_inactive_or_other_native_session(self):
        for manifest, active in (
            ('{"status":"stopped","vt":7}', "tty7\n"),
            ('{"status":"ready","vt":7}', "tty2\n"),
        ):
            with (
                patch.object(Path, "read_text", side_effect=[manifest, active]),
                self.assertRaisesRegex(ControlError, "owned native VT"),
            ):
                require_owned_vt(Path("/test"))

    def test_three_finger_fixture_has_no_stationary_lower_thumb(self):
        before, after = contact_positions(900, 3), contact_positions(550, 3)
        self.assertTrue(all(old != new for old, new in zip(before, after)))
        self.assertLess(before[2][1], before[0][1])
        self.assertLess(after[2][1], after[0][1])

    def test_injector_accepts_its_active_ready_session(self):
        with patch.object(
            Path, "read_text", side_effect=['{"status":"ready","vt":7}', "tty7\n"]
        ):
            self.assertEqual(require_owned_vt(Path("/test"))["vt"], 7)


class RemovalEvidenceTests(unittest.TestCase):
    BEGIN = " event17 GESTURE_PINCH_BEGIN +1.01s\t3\n"
    END = " event17 GESTURE_PINCH_END +2.02s\t3\n"
    CANCEL = " event17 GESTURE_PINCH_END +2.02s\t3 cancelled\n"
    REMOVED = " event17 DEVICE_REMOVED Hearthspace pinch acceptance touchpad\n"

    def setUp(self):
        self.BEFORE = {"x": 0, "y": 0, "scale": 1, "pinch_active": False}
        self.ACTIVE = {"x": -10, "y": -10, "scale": 0.8, "pinch_active": True}

    def test_normal_end_requires_completed_zoom(self):
        after = {**self.ACTIVE, "pinch_active": False}
        trace = self.BEGIN + self.CANCEL + self.BEGIN + self.END + self.REMOVED
        self.assertFalse(
            verify_removal(trace, self.BEFORE, self.ACTIVE, after)["end_cancelled"]
        )
        with self.assertRaises(ControlError):
            verify_removal(trace, self.BEFORE, self.ACTIVE, self.BEFORE)

    def test_cancelled_end_requires_original_view(self):
        trace = self.BEGIN + self.CANCEL + self.REMOVED
        self.assertTrue(
            verify_removal(trace, self.BEFORE, self.ACTIVE, self.BEFORE)[
                "end_cancelled"
            ]
        )
        with self.assertRaises(ControlError):
            verify_removal(
                trace, self.BEFORE, self.ACTIVE, {**self.ACTIVE, "pinch_active": False}
            )

    def test_missing_or_misordered_events_cannot_prove_removal(self):
        for trace in (
            "",
            self.BEGIN + self.END,
            self.END + self.REMOVED,
            self.BEGIN + self.REMOVED + self.END,
            self.BEGIN + self.REMOVED,
        ):
            with (
                self.subTest(trace=trace),
                self.assertRaisesRegex(ControlError, "removal trace"),
            ):
                verify_removal(trace, self.BEFORE, self.ACTIVE, self.BEFORE)

    def test_active_gesture_after_removal_is_rejected(self):
        with self.assertRaises(ControlError):
            verify_removal(
                self.BEGIN + self.END + self.REMOVED,
                self.BEFORE,
                self.ACTIVE,
                self.ACTIVE,
            )


class TouchpadCleanupTests(unittest.TestCase):
    def touchpad(self, returncode):
        device = TouchDevice(Path("/test"), None)
        device.process = Mock()
        device.process.wait.return_value = returncode
        return device

    def test_successful_worker_exit_is_accepted(self):
        self.touchpad(0).__exit__(None, None, None)

    def test_failed_worker_exit_cannot_pass_native_acceptance(self):
        with self.assertRaisesRegex(ControlError, "status 7"):
            self.touchpad(7).__exit__(None, None, None)

    def test_failed_worker_exit_preserves_original_operation_error(self):
        failure = ControlError("native assertion failed")
        self.assertIsNone(self.touchpad(7).__exit__(ControlError, failure, None))


if __name__ == "__main__":
    unittest.main()
