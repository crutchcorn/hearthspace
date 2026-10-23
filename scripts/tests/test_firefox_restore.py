"""Firefox restore observers must wait beyond flags and recheck after capture."""

import sys
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import Mock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib import firefox_assertions as verify
from agentlib import firefox_checks, firefox_geometry
from agentlib.control import ControlError
from test_agent_visual import window_capture


class FirefoxRestoreObserverTests(unittest.TestCase):
    def setUp(self):
        self.window = {
            "id": 7,
            "mapped": True,
            "visible": True,
            "focused_view": False,
            "fullscreen": False,
            "maximized": True,
            "presentation_output": None,
            "output_coordinates": False,
            "x": 20,
            "y": 20,
            "width": 200,
            "height": 160,
            "canvas_x": 20,
            "canvas_y": 20,
            "content_width": 200,
            "content_height": 160,
            "maximized_canvas_bounds": {"x": 20, "y": 20, "width": 200, "height": 160},
        }
        self.viewport = {"x": 0, "y": 0, "scale": 1}
        self.metadata = {
            "width": 320,
            "height": 240,
            "scale": 1,
            "logical_x": 0,
            "logical_y": 0,
        }
        self.observed = []

    def until(self, _label, observe):
        for _ in range(8):
            self.observed.append(value := observe())
            if value:
                return value
        raise ControlError("bounded observer did not become ready")

    def test_resize_rejects_flag_only_restore_then_refreshes_after_capture(self):
        saved = self.window | {"maximized": False, "maximized_canvas_bounds": None}
        stale = saved | {"content_width": 320, "width": 320}
        moved = saved | {"x": 40, "canvas_x": 40}
        events = []
        windows = iter([stale, saved, moved])

        def window(_id):
            events.append("status")
            return next(windows)

        smoke = SimpleNamespace(
            target=7,
            floating_before_max=saved,
            window=window,
            until=self.until,
            capture=lambda _label: events.append("capture"),
        )
        result = firefox_geometry.restored_for_resize(smoke)
        self.assertEqual(result, moved)
        self.assertEqual(self.observed, [None, saved, moved])
        self.assertEqual(events, ["status", "status", "capture", "status"])

    def test_restore_does_not_accept_wrong_height_or_another_presentation_mode(self):
        saved = self.window | {"maximized": False}
        self.assertTrue(verify.restored_client_size(saved, saved))
        for change in (
            {"content_height": 240},
            {"focused_view": True},
            {"fullscreen": True},
            {"maximized": True},
            {"presentation_output": "native"},
        ):
            with self.subTest(change=change):
                self.assertFalse(verify.restored_client_size(saved | change, saved))

    def test_real_resize_flow_targets_only_post_capture_committed_edge(self):
        saved = self.window | {"maximized": False, "maximized_canvas_bounds": None}
        stale = saved | {"content_width": 320, "width": 320}
        refreshed = saved | {"x": 40, "canvas_x": 40}
        resized = refreshed | {"width": 40, "content_width": 40}
        output = {
            "logical_x": 0,
            "logical_y": 0,
            "logical_width": 320,
            "logical_height": 240,
        }
        smoke = SimpleNamespace(
            target=7,
            floating_before_max=saved,
            until=self.until,
            record=Mock(),
            control=SimpleNamespace(status=lambda: {"outputs": [output]}),
            window=Mock(side_effect=[stale, saved, refreshed, resized]),
            capture=Mock(return_value=window_capture(self.metadata, resized)),
        )
        with (
            patch.object(firefox_checks, "prepare_maximized"),
            patch.object(firefox_checks, "activate"),
            patch.object(firefox_checks, "Input") as input_context,
        ):
            firefox_checks.controls_and_resize(smoke)
        input_context.return_value.__enter__.return_value.drag.assert_called_once_with(
            239, 100, 79, 100, duration=0.5
        )
        self.assertEqual(smoke.record.call_args.kwargs["before"], refreshed)

    def test_f11_exit_waits_for_canvas_mode_and_committed_size_at_new_viewport(self):
        view = {"x": -60, "y": -140 / 3, "scale": 0.75}
        restored = self.window | {"x": 60, "y": 50, "width": 150, "height": 120}
        samples = iter(
            [
                restored | {"maximized": False},
                restored | {"focused_view": True},
                restored | {"content_width": 320, "content_height": 240},
                restored,
                restored,
            ]
        )
        smoke = SimpleNamespace(
            target=7,
            control=SimpleNamespace(
                status=lambda: {"viewport": view, "windows": [next(samples)]}
            ),
            until=self.until,
            record=Mock(),
            capture=Mock(
                return_value=window_capture(
                    self.metadata, restored, landmark=(75, 65, 105, 80)
                )
            ),
        )
        firefox_geometry.fullscreen_exit(smoke, self.window, self.viewport)
        self.assertEqual(self.observed[:3], [None, None, None])
        self.assertEqual(len(self.observed), 5)
        record = smoke.record.call_args.kwargs
        self.assertEqual(record["after"], restored)
        self.assertEqual(record["before"], self.window)
        self.assertEqual(record["restore_scale_ratio"], 0.75)
        smoke.capture.assert_called_once_with("fullscreen-exit")

    def test_f11_exit_rejects_changed_saved_bounds_and_bad_projection(self):
        verify.verify_fullscreen_exit(
            self.viewport, self.viewport, self.window, self.window
        )
        for change in (
            {"maximized_canvas_bounds": None},
            {"maximized_canvas_bounds": {"x": 0, "y": 0, "width": 320, "height": 240}},
            {"canvas_x": 0},
            {"content_height": 240},
            {"x": 40},
            {"height": 240},
        ):
            with self.subTest(change=change), self.assertRaises(ControlError):
                verify.verify_fullscreen_exit(
                    self.viewport, self.viewport, self.window, self.window | change
                )

    def test_real_f11_flow_remembers_pre_fullscreen_canvas_geometry(self):
        view = {"x": -60, "y": -140 / 3, "scale": 0.75}
        restored = self.window | {"x": 60, "y": 50, "width": 150, "height": 120}
        full = self.window | {
            "fullscreen": True,
            "focused_view": True,
            "output_coordinates": True,
            "presentation_output": "native",
            "x": 0,
            "y": 0,
            "width": 320,
            "height": 240,
            "content_width": 320,
            "content_height": 240,
        }
        output = {
            "name": "native",
            "logical_x": 0,
            "logical_y": 0,
            "logical_width": 320,
            "logical_height": 240,
        }
        runtime = {"mode": "canvas", "viewport": self.viewport}
        exits = iter(
            [
                restored | {"maximized": False},
                restored | {"content_width": 320},
                restored,
                restored,
            ]
        )

        def status():
            window = next(exits) if runtime["mode"] == "exiting" else full
            return {
                "viewport": runtime["viewport"],
                "outputs": [output],
                "windows": [window],
            }

        def chord(_name):
            runtime["mode"] = "fullscreen" if runtime["mode"] == "canvas" else "exiting"

        def scroll(*_args, **_kwargs):
            runtime["viewport"] = view

        full_image = window_capture(self.metadata, full)
        exit_image = window_capture(self.metadata, restored)
        smoke = SimpleNamespace(
            target=7,
            until=self.until,
            record=Mock(),
            window=lambda _id: full,
            control=SimpleNamespace(status=status),
            capture=lambda name: (
                exit_image if name == "fullscreen-exit" else full_image
            ),
        )
        with (
            patch.object(firefox_checks, "prepare_maximized", return_value=self.window),
            patch.object(firefox_checks, "settled", return_value=view),
            patch.object(firefox_checks, "Input") as input_context,
        ):
            inputs = input_context.return_value.__enter__.return_value
            inputs.chord.side_effect = chord
            inputs.scroll.side_effect = scroll
            firefox_checks.fullscreen(smoke)
        self.assertEqual(smoke.record.call_args.args[0], "fullscreen-exit-restored")
        self.assertEqual(smoke.record.call_args.kwargs["before"], self.window)
        self.assertEqual(smoke.record.call_args.kwargs["after"], restored)
        self.assertEqual(self.observed.count(None), 2)


if __name__ == "__main__":
    unittest.main()
