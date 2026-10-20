"""Coordinate and cleanup regressions for the real canvas-map acceptance run."""

import contextlib
import io
import json
import sys
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import Mock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib import map_checks, map_drag, map_touch
from agentlib.control import ControlError
from agentlib.map_smoke import MapSmoke, map_pixels, percentage
from test_agent_visual import capture


class MapCoordinateTests(unittest.TestCase):
    def setUp(self):
        self.output = {
            "name": "test",
            "logical_x": -100,
            "logical_y": 30,
            "logical_width": 1280,
            "scale": 2,
        }
        self.surface = {
            "output": "test",
            "x": 800,
            "y": 90,
            "width": 320,
            "height": 300,
        }
        self.bounds = {"x": 40, "y": 480, "width": 80, "height": 60}

    def test_popover_buttons_use_actual_surface_origin_and_output_scale(self):
        self.assertEqual(
            map_checks.button_center(self.bounds, self.output, self.surface), (840, 345)
        )
        moved = self.surface | {"x": -90, "y": 42}
        self.assertEqual(
            map_checks.button_center(self.bounds, self.output, moved), (-50, 297)
        )
        with self.assertRaisesRegex(ControlError, "outside"):
            map_checks.button_center(
                self.bounds | {"y": 610}, self.output, self.surface
            )

    def test_map_pointer_requires_unambiguous_mapped_surface_and_output(self):
        smoke = Mock()
        for maps, outputs in [
            ([], [self.output]),
            ([self.surface] * 2, [self.output]),
            ([self.surface], []),
        ]:
            with self.subTest(maps=maps, outputs=outputs):
                smoke.control.status.return_value = {
                    "canvas_maps": maps,
                    "outputs": outputs,
                }
                with self.assertRaises(ControlError):
                    map_checks.map_surface(smoke)

    def test_zoom_percentage_waits_for_animation_before_choosing_label(self):
        smoke = Mock()
        smoke.control.status.side_effect = [
            {"viewport": {"animating": True, "scale": 0.91}},
            {"viewport": {"animating": False, "scale": 0.85}},
        ]

        def until(_name, condition):
            return condition() or condition()

        smoke.until.side_effect = until
        with patch("agentlib.map_smoke.a11y", return_value={"elements": [{}]}) as a11y:
            percentage(smoke)
        self.assertIn("@name='85%'", a11y.call_args.args[2])
        self.assertEqual(smoke.control.status.call_count, 2)

    def test_map_button_selector_excludes_popover_heading_with_same_name(self):
        self.assertIn(
            "@role='button'", map_checks.selector("Canvas map", role="button")
        )

    def test_minimap_pixel_proof_excludes_unrelated_client_changes(self):
        metadata = {
            "width": 400,
            "height": 300,
            "scale": 2,
            "logical_x": -100,
            "logical_y": 30,
        }
        surface = {"x": 0, "y": 50}
        bounds = {"x": 10, "y": 20, "width": 60, "height": 40}
        first = capture(metadata, [((5, 60, 15, 70), (233, 100, 42))])
        elsewhere = capture(
            metadata,
            [((5, 60, 15, 70), (233, 100, 42)), ((-90, 35, -80, 45), (255, 0, 0))],
        )
        changed = capture(metadata, [((15, 60, 25, 70), (233, 100, 42))])
        self.assertEqual(
            map_pixels(first, surface, bounds), map_pixels(elsewhere, surface, bounds)
        )
        self.assertNotEqual(
            map_pixels(first, surface, bounds), map_pixels(changed, surface, bounds)
        )
        with self.assertRaisesRegex(ControlError, "outside"):
            map_pixels(first, {"x": 200, "y": 50}, bounds)


class MapLifecycleTests(unittest.TestCase):
    def test_failure_stops_and_verifies_owned_native_session(self):
        with (
            tempfile.TemporaryDirectory() as directory,
            contextlib.redirect_stdout(io.StringIO()),
        ):
            smoke = MapSmoke(
                SimpleNamespace(
                    session=Path(directory),
                    timeout=30,
                    total_timeout=120,
                    log_level="info",
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
                    smoke, "workflow", side_effect=ControlError("map proof failed")
                ),
                patch.object(smoke, "verify_restored") as restored,
                self.assertRaisesRegex(ControlError, "map proof failed"),
            ):
                smoke.run()
            self.assertEqual([entry[0] for entry in operations], ["start", "stop"])
            self.assertIn("--no-build", operations[0][1])
            restored.assert_called_once()


class MapDragTests(unittest.TestCase):
    def test_viewport_frame_detection_uses_only_observed_map_bounds(self):
        metadata = {
            "width": 400,
            "height": 300,
            "scale": 2,
            "logical_x": -100,
            "logical_y": 30,
        }
        surface = {"x": 0, "y": 50}
        bounds = {"x": 10, "y": 20, "width": 100, "height": 80}
        orange = (233, 100, 42)
        image = capture(
            metadata, [((10, 65, 30, 75), orange), ((-90, 35, -80, 45), orange)]
        )
        self.assertEqual(
            map_drag.accent_viewport(image, surface, bounds), (220, 70, 260, 90)
        )
        unrelated = capture(
            metadata,
            [((10, 65, 30, 75), (47, 118, 194)), ((-90, 35, -80, 45), orange)],
        )
        with self.assertRaisesRegex(ControlError, "orange viewport"):
            map_drag.accent_viewport(unrelated, surface, bounds)

    def test_drag_checks_sign_magnitude_and_preserves_zoom(self):
        before = {"x": -100, "y": 50, "scale": 1}
        after = before | {"x": 380, "y": 410}
        output = {"logical_width": 1280, "logical_height": 800}
        expected = map_drag.verify_drag(before, after, (12, 9), (34, 22), output)
        self.assertEqual(expected, {"x": 480, "y": 360})
        for change in ({"scale": 0.8}, {"x": -580}, {"y": 59}):
            with self.subTest(change=change), self.assertRaises(ControlError):
                map_drag.verify_drag(before, after | change, (12, 9), (34, 22), output)

    def test_native_touch_drag_proves_release_and_removes_owned_device(self):
        with tempfile.TemporaryDirectory() as directory:
            smoke = Mock(directory=Path(directory), artifacts=Path(directory))
            output = {
                "logical_x": 0,
                "logical_y": 0,
                "logical_width": 1280,
                "logical_height": 800,
            }
            with patch("agentlib.map_touch.TouchDevice") as device_type:
                device = device_type.return_value.__enter__.return_value
                device.device = {"device": str(Path(directory) / "removed-event")}

                def request(operation, **_):
                    if operation == "end":
                        (
                            Path(directory) / "libinput-map-drag-touchscreen.log"
                        ).write_text(
                            "TOUCH_DOWN\nTOUCH_MOTION\nTOUCH_UP\nTOUCH_FRAME\n"
                        )

                device.request.side_effect = request
                delta = map_touch.drag(smoke, output, (100, 100), (12, 9))
                self.assertAlmostEqual(delta[0], 12, delta=0.7)
                self.assertAlmostEqual(delta[1], 9, delta=0.7)
                self.assertEqual(device.request.call_args.args, ("end",))
                device_type.return_value.__exit__.assert_called_once()

    def test_failed_native_touch_motion_still_releases_contact(self):
        with tempfile.TemporaryDirectory() as directory:
            smoke = Mock(directory=Path(directory), artifacts=Path(directory))
            output = {
                "logical_x": 0,
                "logical_y": 0,
                "logical_width": 1280,
                "logical_height": 800,
            }
            with patch("agentlib.map_touch.TouchDevice") as device_type:
                device = device_type.return_value.__enter__.return_value
                device.device = {"device": "/not-used"}
                device.request.side_effect = [None, ControlError("motion failed"), None]
                with self.assertRaisesRegex(ControlError, "motion failed"):
                    map_touch.drag(smoke, output, (100, 100), (12, 9))
                self.assertEqual(device.request.call_args.args, ("end",))
                device_type.return_value.__exit__.assert_called_once()


if __name__ == "__main__":
    unittest.main()
