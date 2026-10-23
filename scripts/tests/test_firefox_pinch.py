"""Firefox cancellation observes canvas rollback before releasing candidates."""

import sys
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import Mock, call, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib import firefox_checks
from agentlib.control import ControlError


class FirefoxPinchCancellationTests(unittest.TestCase):
    def setUp(self):
        self.output = {
            "name": "test",
            "logical_x": 0,
            "logical_y": 0,
            "logical_width": 1280,
            "logical_height": 800,
        }
        self.before = {
            "mapped": True,
            "visible": True,
            "maximized": True,
            "fullscreen": False,
            "focused_view": False,
            "presentation_output": None,
            "output_coordinates": False,
            "x": 24,
            "y": 96,
            "width": 1232,
            "height": 608,
            "canvas_x": 100000,
            "canvas_y": 100000,
            "content_width": 1232,
            "content_height": 608,
            "maximized_canvas_bounds": {
                "x": 100000,
                "y": 100000,
                "width": 1232,
                "height": 608,
            },
        }
        self.floating = self.before | {
            "maximized": False,
            "maximized_canvas_bounds": None,
            "width": 840,
            "height": 520,
            "content_width": 840,
            "content_height": 520,
        }
        self.viewport = {
            "x": 99976,
            "y": 99904,
            "scale": 1,
            "animating": False,
            "pinch_active": False,
        }
        self.phase = "before"
        self.held_view = self.viewport | {"pinch_active": True}
        self.held_switcher = False
        self.released_view = self.viewport.copy()
        self.released_switcher = False
        self.released_window = self.before.copy()
        self.release_pending_samples = 0
        self.delay_release_geometry = False
        self.samples = []
        self.device = Mock(kind="touchscreen")
        self.device.request.side_effect = self.request
        self.smoke = SimpleNamespace(
            target=7,
            floating_before_max=self.floating,
            control=SimpleNamespace(
                status=self.status,
                deadline=SimpleNamespace(remaining=lambda: 1),
            ),
            window=self.window,
            until=self.until,
            capture=Mock(),
            record=Mock(),
        )

    def request(self, operation, **_kwargs):
        self.phase = {
            "begin": "active",
            "move": "active",
            "add-finger": "held",
            "end": "released",
        }[operation]

    def status(self):
        viewport, switcher = self.viewport, False
        if self.phase == "active":
            viewport = self.viewport | {"scale": 0.65, "pinch_active": True}
        elif self.phase == "held":
            viewport, switcher = self.held_view, self.held_switcher
        elif self.phase == "released":
            viewport, switcher = self.released_view, self.released_switcher
            if self.release_pending_samples:
                self.release_pending_samples -= 1
                viewport = viewport | {"pinch_active": True}
        self.samples.append((self.phase, viewport.copy(), switcher))
        return {
            "viewport": viewport,
            "window_switcher": {"active": switcher},
            "outputs": [self.output],
        }

    def window(self, _target):
        if self.phase == "active":
            return self.before | {"width": 800, "height": 395}
        if self.phase == "released":
            if self.delay_release_geometry and self.release_pending_samples:
                return self.before
            return self.released_window
        return self.floating if self.phase == "floating" else self.before

    def until(self, label, observe):
        for _ in range(4):
            if value := observe():
                return value
        raise ControlError(f"observer did not become ready: {label}")

    def cancel(self):
        def activate(_smoke, name):
            self.assertEqual(name, "Restore Down")
            self.phase = "floating"

        with (
            patch.object(firefox_checks, "prepare_maximized", return_value=self.before),
            patch.object(firefox_checks, "activate", side_effect=activate),
            patch.object(firefox_checks, "Input"),
            patch("agentlib.pinch_touchscreen.time.sleep"),
        ):
            firefox_checks.cancel_pinch(self.smoke, self.device, self.output)

    def test_held_candidate_rolls_back_then_releases_and_restores_floating_window(self):
        self.cancel()
        self.assertIn(("held", self.held_view, False), self.samples)
        self.assertIn(("released", self.viewport, False), self.samples)
        operations = [entry.args[0] for entry in self.device.request.call_args_list]
        self.assertEqual(operations, ["begin", "move", "add-finger", "end"])
        moved = self.device.request.call_args_list[1].kwargs["points"]
        center = tuple(sum(point[axis] for point in moved) / 3 for axis in (0, 1))
        self.assertEqual(
            self.device.request.call_args_list[2], call("add-finger", point=center)
        )
        self.assertEqual(
            self.smoke.capture.call_args_list,
            [
                call("touchscreen-cancel-active"),
                call("touchscreen-cancel-restored"),
                call("touchscreen-cancel-floating-restored"),
            ],
        )
        self.assertEqual(self.smoke.record.call_args.kwargs["actual"], self.floating)

    def test_held_nonrollback_is_rejected_before_release(self):
        self.held_view = self.held_view | {"scale": 0.65}
        with self.assertRaisesRegex(ControlError, "fourth contact restores"):
            self.cancel()
        self.assertNotIn(call("end"), self.device.request.call_args_list)

    def test_held_open_switcher_is_rejected_before_release(self):
        self.held_switcher = True
        with self.assertRaisesRegex(ControlError, "fourth contact restores"):
            self.cancel()
        self.assertNotIn(call("end"), self.device.request.call_args_list)

    def test_release_waits_for_candidate_to_become_inactive(self):
        self.release_pending_samples = 3
        self.cancel()
        released = [view for phase, view, _ in self.samples if phase == "released"]
        self.assertTrue(all(view["pinch_active"] for view in released[:3]))
        self.assertTrue(all(not view["pinch_active"] for view in released[3:]))
        self.assertEqual(self.phase, "floating")

    def test_permanently_active_candidate_after_release_is_rejected(self):
        self.released_view = self.viewport | {"pinch_active": True}
        with self.assertRaisesRegex(ControlError, "inactive viewport"):
            self.cancel()
        self.assertEqual(self.phase, "released")

    def test_released_viewport_must_still_match_original_transform(self):
        self.released_view = self.viewport | {"x": self.viewport["x"] + 1}
        with self.assertRaisesRegex(ControlError, "exact inactive viewport"):
            self.cancel()

    def test_release_must_not_open_switcher(self):
        self.released_switcher = True
        with self.assertRaisesRegex(ControlError, "without a switcher"):
            self.cancel()

    def test_cancelled_world_geometry_is_still_checked_after_release(self):
        self.released_window = self.before | {"canvas_x": 100001}
        with self.assertRaisesRegex(ControlError, "world geometry"):
            self.cancel()

    def test_geometry_is_sampled_after_delayed_release_finishes(self):
        self.release_pending_samples = 3
        self.delay_release_geometry = True
        self.released_window = self.before | {"canvas_x": 100001}
        with self.assertRaisesRegex(ControlError, "world geometry"):
            self.cancel()


if __name__ == "__main__":
    unittest.main()
