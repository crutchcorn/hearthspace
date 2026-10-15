"""Independent gesture trace and rendered-translation acceptance regressions."""

import contextlib
import io
import json
import math
import sys
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib.control import ControlError
from agentlib.gesture_trace import gesture
from agentlib.pan import PanSmoke
from agentlib.pan_checks import expected_direction, pinned_marker, unchanged
from agentlib.pinch_assertions import verify_zoom
from agentlib.touchpad import checked_points, contact_positions
from test_agent_visual import window_capture


def event(kind, phase, detail="", fingers=3, counter=""):
    return f" event17 GESTURE_{kind}_{phase} {counter} +0.123s {fingers} {detail}\n"


class GestureTraceTests(unittest.TestCase):
    def test_pinch_uses_accelerated_translation_and_absolute_final_scale(self):
        trace = event("PINCH", "BEGIN")
        trace += event(
            "PINCH", "UPDATE", "-0.95/ 0.71 (-1.05/ 0.79 unaccelerated) 0.98 @ -0.03"
        )
        trace += event(
            "PINCH",
            "UPDATE",
            "-1.05/ 0.79 (-1.05/ 0.79 unaccelerated) 0.75 @ 0.00",
            counter="2",
        )
        proof = gesture(trace, "PINCH")
        self.assertEqual(proof["translation"], [-2, 1.5])
        self.assertEqual(proof["scale"], 0.75)
        self.assertEqual(proof["updates"], 2)
        self.assertFalse(proof["ended"])
        with self.assertRaisesRegex(ControlError, "ending"):
            gesture(trace, "PINCH", require_end=True)

    def test_swipe_cancel_is_not_confused_by_following_four_contact_gesture(self):
        trace = event("SWIPE", "BEGIN")
        trace += event("SWIPE", "UPDATE", "12.0/-8.0 (10.0/-6.0 unaccelerated)")
        trace += event("SWIPE", "END", "cancelled")
        trace += event("SWIPE", "BEGIN", fingers=4)
        trace += event("SWIPE", "UPDATE", "30/40 (30/40 unaccelerated)", fingers=4)
        proof = gesture(trace, "SWIPE", require_end=True)
        self.assertEqual(proof["translation"], [12, -8])
        self.assertTrue(proof["cancelled"])
        self.assertEqual(proof["scale"], 1)

    def test_missing_or_malformed_events_cannot_prove_pan(self):
        for trace in [
            "",
            event("SWIPE", "BEGIN"),
            event("SWIPE", "UPDATE", "1/2 (1/2 unaccelerated)"),
            event("SWIPE", "BEGIN") + event("SWIPE", "UPDATE", "bad"),
            event("PINCH", "BEGIN")
            + event("PINCH", "UPDATE", "1/2 (1/2 unaccelerated)"),
        ]:
            with self.subTest(trace=trace), self.assertRaises(ControlError):
                gesture(trace, "PINCH" if "PINCH" in trace else "SWIPE")


class PanFixtureTests(unittest.TestCase):
    def test_uniform_translation_preserves_each_finger_offset_and_radius(self):
        original = contact_positions(900, 3)
        translated = contact_positions(900, 3, (120, -80))
        self.assertEqual(
            [(b[0] - a[0], b[1] - a[1]) for a, b in zip(original, translated)],
            [(120, -80)] * 3,
        )
        self.assertEqual(len(checked_points(translated)), 3)

    def test_combined_pinch_translation_is_independent_of_width(self):
        zoomed = contact_positions(600, 3)
        moved = contact_positions(600, 3, (-90, 70))
        self.assertEqual(
            [(b[0] - a[0], b[1] - a[1]) for a, b in zip(zoomed, moved)], [(-90, 70)] * 3
        )
        for bad in ([math.nan, 0], [0, math.inf], [1], [1, 2, 3]):
            with self.subTest(value=bad), self.assertRaises(ControlError):
                contact_positions(900, 3, bad)
        with self.assertRaises(ControlError):
            checked_points(contact_positions(900, 3, (2000, 0)))

    def test_directions_require_both_axes_and_pure_pan_retains_scale(self):
        case = {"delta": (60, -40)}
        expected_direction(case, {"translation": [30, -20], "scale": 1})
        expected_direction(case, {"translation": [30, -20], "scale": 1 + 1e-15})
        for evidence in [
            {"translation": [30, 20], "scale": 1},
            {"translation": [30, -2], "scale": 1},
            {"translation": [30, -20], "scale": 0.9},
        ]:
            with self.subTest(evidence=evidence), self.assertRaises(ControlError):
                expected_direction(case, evidence)


class PanPixelTests(unittest.TestCase):
    def setUp(self):
        self.metadata = {
            "width": 320,
            "height": 240,
            "scale": 1,
            "logical_x": 0,
            "logical_y": 0,
        }
        self.window = {"x": 20, "y": 20, "width": 200, "height": 160}
        self.before = window_capture(
            self.metadata, self.window, landmark=(40, 40, 80, 60)
        )

    def verify(self, translation=(20, -10), landmark=(60, 30, 100, 50)):
        after_window = self.window | {"x": 40, "y": 10}
        return verify_zoom(
            {"x": 0, "y": 0, "scale": 1},
            {"x": -20, "y": 10, "scale": 1},
            (100, 80),
            self.window,
            after_window,
            self.before,
            window_capture(self.metadata, after_window, landmark=landmark),
            translation=translation,
        )

    def test_pan_requires_marker_translation_not_only_viewport_metadata(self):
        self.assertEqual(self.verify()["translation"], (20, -10))
        with self.assertRaisesRegex(ControlError, "marker"):
            self.verify(landmark=(40, 40, 80, 60))
        with self.assertRaisesRegex(ControlError, "anchor"):
            self.verify(translation=(-20, 10))

    def test_combined_transform_checks_all_marker_edges(self):
        after_window = {"x": 70, "y": 55, "width": 100, "height": 80}
        after = window_capture(self.metadata, after_window, landmark=(80, 65, 100, 75))
        verify_zoom(
            {"x": 0, "y": 0, "scale": 1},
            {"x": -120, "y": -90, "scale": 0.5},
            (100, 80),
            self.window,
            after_window,
            self.before,
            after,
            translation=(10, 5),
        )
        stale = window_capture(self.metadata, after_window, landmark=(70, 60, 90, 70))
        with self.assertRaisesRegex(ControlError, "translation"):
            verify_zoom(
                {"x": 0, "y": 0, "scale": 1},
                {"x": -120, "y": -90, "scale": 0.5},
                (100, 80),
                self.window,
                after_window,
                self.before,
                stale,
                translation=(10, 5),
            )

    def test_cancelled_or_fullscreen_window_must_keep_geometry_and_flags(self):
        window = self.window | {
            "canvas_x": 100000,
            "canvas_y": 100000,
            "maximized": True,
            "fullscreen": False,
        }
        unchanged(window, window.copy())
        for changed in ({"x": 21}, {"maximized": False}, {"canvas_y": 100001}):
            with self.subTest(changed=changed), self.assertRaises(ControlError):
                unchanged(window, window | changed)

    def test_fullscreen_requires_marker_position_not_only_size_and_window_bounds(self):
        pinned_marker("fullscreen", self.before, self.before, self.window, self.window)
        shifted = window_capture(self.metadata, self.window, landmark=(50, 40, 90, 60))
        with self.assertRaisesRegex(ControlError, "fullscreen marker"):
            pinned_marker("fullscreen", self.before, shifted, self.window, self.window)


class PanLifecycleTests(unittest.TestCase):
    def test_failure_stops_owned_session_after_no_build_start(self):
        with (
            tempfile.TemporaryDirectory() as directory,
            contextlib.redirect_stdout(io.StringIO()),
        ):
            smoke = PanSmoke(
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
                operations.append((operation, extra))
                (smoke.directory / "session.json").write_text(
                    json.dumps({"cleanup": {"complete": operation == "stop"}})
                )

            with (
                patch("agentlib.daily.shutil.which", return_value="/test"),
                patch.object(smoke, "launcher", side_effect=launcher),
                patch.object(
                    smoke, "workflow", side_effect=ControlError("pan proof failed")
                ),
                patch.object(smoke, "verify_restored") as restored,
                self.assertRaisesRegex(ControlError, "pan proof failed"),
            ):
                smoke.run()
            self.assertEqual([entry[0] for entry in operations], ["start", "stop"])
            self.assertIn("--no-build", operations[0][1])
            restored.assert_called_once()


if __name__ == "__main__":
    unittest.main()
