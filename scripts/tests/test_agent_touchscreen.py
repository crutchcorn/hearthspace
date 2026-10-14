"""Coordinate and event-evidence checks for the native touchscreen fixture."""

import math
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib.control import ControlError
from agentlib.pinch_assertions import verify_touch_removal
from agentlib.pinch_touchscreen import centroid, from_device, scaled, to_device
from agentlib.touchpad import checked_points, contact_positions


class TouchCoordinatesTests(unittest.TestCase):
    def test_device_mapping_uses_logical_dimensions_and_output_origin(self):
        output = {
            "logical_x": -100,
            "logical_y": 50,
            "logical_width": 1280,
            "logical_height": 800,
            "scale": 2,
        }
        points = [(-100, 50), (540, 450), (1180, 850)]
        raw = to_device(points, output)
        self.assertEqual(raw, [[0, 0], [1000, 700], [2000, 1400]])
        self.assertEqual(from_device(raw, output), points)

    def test_scale_keeps_centroid_and_applies_absolute_radius_ratio(self):
        points = [[120, 200], [900, 400], [600, 1000]]
        center = centroid(points)
        zoomed = scaled(points, 0.7)
        self.assertEqual(centroid(zoomed), center)

        def radius(values):
            return math.sqrt(
                sum(
                    sum((axis - anchor) ** 2 for axis, anchor in zip(point, center))
                    for point in values
                )
                / len(values)
            )

        self.assertAlmostEqual(radius(zoomed) / radius(points), 0.7)

    def test_virtual_device_rejects_invalid_contacts_before_injection(self):
        for points in (
            [],
            [[1, 2]] * 5,
            [[math.nan, 0]],
            [[0, math.inf]],
            [[2001, 0]],
            [[0, -1]],
            [[1]],
        ):
            with self.subTest(points=points), self.assertRaises(ControlError):
                checked_points(points)

    def test_four_contact_touchpad_can_cancel_three_finger_gesture(self):
        points = contact_positions(900, 4)
        self.assertEqual(len(set(points)), 4)
        self.assertTrue(all(0 < x < 2000 and 0 < y < 1400 for x, y in points))


class TouchRemovalTests(unittest.TestCase):
    DOWN = "".join(
        f" event17 TOUCH_DOWN +1.00s {slot} ({slot}) 25/30\n" for slot in range(3)
    )
    UP = " event17 TOUCH_UP +2.00s 0 (0)\n"
    CANCEL = " event17 TOUCH_CANCEL +2.00s 0 (0)\n"
    REMOVED = " event17 DEVICE_REMOVED Hearthspace touchscreen\n"

    def setUp(self):
        self.before = {"x": 0, "y": 0, "scale": 1, "pinch_active": False}
        self.active = {"x": -30, "y": -20, "scale": 0.7, "pinch_active": True}
        self.completed = {**self.active, "pinch_active": False}

    def test_first_normal_up_retains_completed_zoom(self):
        trace = self.DOWN + self.UP + self.REMOVED
        self.assertEqual(
            verify_touch_removal(trace, self.before, self.active, self.completed)[
                "first_ending"
            ],
            "TOUCH_UP",
        )
        with self.assertRaises(ControlError):
            verify_touch_removal(trace, self.before, self.active, self.before)

    def test_cancellation_and_direct_removal_restore_active_gesture(self):
        for end in (self.CANCEL, ""):
            trace = self.DOWN + end + self.REMOVED
            verify_touch_removal(trace, self.before, self.active, self.before)
            with self.assertRaises(ControlError):
                verify_touch_removal(trace, self.before, self.active, self.completed)

    def test_missing_misordered_and_cross_device_evidence_is_rejected(self):
        for trace in (
            self.UP + self.REMOVED,
            self.DOWN + self.UP,
            self.DOWN + self.REMOVED + self.UP,
            self.DOWN + self.UP + self.REMOVED.replace("event17", "event18"),
        ):
            with self.subTest(trace=trace), self.assertRaises(ControlError):
                verify_touch_removal(trace, self.before, self.active, self.completed)

    def test_completed_viewport_with_stuck_active_flag_is_rejected(self):
        with self.assertRaises(ControlError):
            verify_touch_removal(
                self.DOWN + self.UP + self.REMOVED,
                self.before,
                self.active,
                self.active,
            )


if __name__ == "__main__":
    unittest.main()
