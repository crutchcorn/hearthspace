"""Math, native target mapping, and ownership regressions; no simulated Pan."""

import contextlib
import io
import json
import sys
import tempfile
import unittest
import xml.etree.ElementTree as ET
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import Mock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib import canvas_pan_checks as checks
from agentlib import canvas_pan_fixture as fixture
from agentlib import canvas_pan_ui as ui
from agentlib.canvas_pan_smoke import CanvasPanSmoke, pan
from agentlib.control import ControlError
from test_agent_visual import capture


class PanMathTests(unittest.TestCase):
    def setUp(self):
        self.before = {
            "viewport": {"x": 37, "y": -29, "scale": 0.8},
            "windows": [
                {"id": 7, "canvas_x": 200, "canvas_y": 100, "content_width": 300}
            ],
        }

    def test_pan_offsets_use_canvas_scale_and_round_the_absolute_result_once(self):
        for scale, expected in (
            (0.8, {"x": -17, "y": 5}),
            (1, {"x": -6, "y": -2}),
            (2, {"x": 16, "y": -16}),
        ):
            with self.subTest(scale=scale):
                view = self.before["viewport"] | {"scale": scale}
                self.assertEqual(checks.expected_offset(view, (43, -27)), expected)

    def test_halfway_rounding_is_away_from_zero_not_python_bankers_rounding(self):
        view = {"x": 0, "y": 0, "scale": 2}
        self.assertEqual(checks.expected_offset(view, (1, -1)), {"x": -1, "y": 1})
        self.assertEqual(
            checks.expected_offset(view | {"x": 1, "y": -1}, (1, -1)),
            {"x": 1, "y": -1},
        )

    def test_pan_rejects_direction_magnitude_zoom_and_window_geometry_errors(self):
        valid = self.before | {"viewport": {"x": -17, "y": 5, "scale": 0.8}}
        checks.verify_pan(self.before, valid, (43, -27))
        for broken in (
            valid | {"viewport": valid["viewport"] | {"x": 91}},
            valid | {"viewport": valid["viewport"] | {"y": 4}},
            valid | {"viewport": valid["viewport"] | {"scale": 1}},
            valid | {"windows": [self.before["windows"][0] | {"canvas_x": 201}]},
            valid | {"windows": [self.before["windows"][0] | {"content_width": 301}]},
            valid | {"windows": []},
        ):
            with self.subTest(broken=broken), self.assertRaises(ControlError):
                checks.verify_pan(self.before, broken, (43, -27))

    def test_invalid_scales_and_coordinates_fail_before_input(self):
        for scale in (0, -1, float("nan"), float("inf")):
            with self.subTest(scale=scale), self.assertRaises(ControlError):
                checks.expected_offset(
                    self.before["viewport"] | {"scale": scale}, (1, 1)
                )
        with self.assertRaises(ControlError):
            checks.expected_offset(self.before["viewport"], (float("inf"), 0))

    def test_marker_proof_rejects_stale_pixels_even_with_correct_status(self):
        metadata = {
            "width": 400,
            "height": 300,
            "scale": 2,
            "logical_x": -100,
            "logical_y": 30,
        }
        old_window = {"x": -80, "y": 45, "width": 100, "height": 100}
        new_window = old_window | {"x": -60, "y": 35}
        old = capture(metadata, [((-70, 60, -50, 70), (0, 255, 0))])
        new = capture(metadata, [((-50, 50, -30, 60), (0, 255, 0))])
        old_view = {"x": 0, "y": 0, "scale": 0.8}
        new_view = old_view | {"x": -25, "y": 12.5}
        checks.verify_marker(old, new, old_window, new_window, old_view, new_view)
        with self.assertRaisesRegex(ControlError, "follow the pan"):
            checks.verify_marker(old, old, old_window, new_window, old_view, new_view)


class PanFixtureTests(unittest.TestCase):
    def setUp(self):
        # Actual native Foot commit after a 525x325 resize at canvas scale .8.
        self.window = {
            "canvas_x": 137,
            "canvas_y": 96,
            "content_width": 522,
            "content_height": 325,
            "maximized": False,
            "fullscreen": False,
        }

    def test_fixture_accepts_exact_size_and_bounded_cell_round_down(self):
        for width, height in ((525, 325), (522, 325), (493, 293)):
            with self.subTest(width=width, height=height):
                window = self.window | {
                    "content_width": width,
                    "content_height": height,
                }
                self.assertTrue(fixture.configured(window, 137, 96, 525, 325))

    def test_fixture_rejects_oversize_excess_rounding_and_unsettled_placement(self):
        self.assertFalse(fixture.configured(None, 137, 96, 525, 325))
        for change in (
            {"content_width": 526},
            {"content_height": 326},
            {"content_width": 492},
            {"content_height": 292},
            {"canvas_x": 138},
            {"canvas_y": 97},
            {"maximized": True},
            {"fullscreen": True},
        ):
            with self.subTest(change=change):
                self.assertFalse(
                    fixture.configured(self.window | change, 137, 96, 525, 325)
                )


class PanTargetTests(unittest.TestCase):
    def setUp(self):
        self.output = {
            "name": "native",
            "logical_x": -100,
            "logical_y": 30,
            "logical_width": 1280,
            "logical_height": 800,
            "scale": 2,
        }
        self.surface = {"x": -76, "y": 540, "width": 260, "height": 144}
        self.elements = [
            {"role": "Frame", "bounds": {"width": 520, "height": 288}},
            {
                "role": "ToggleButton",
                "name": "Pan canvas",
                "bounds": {"x": 12, "y": 100, "width": 496, "height": 88},
            },
        ]

    def test_observed_pan_row_uses_native_surface_origin_and_output_scale(self):
        self.assertEqual(
            ui.observed_point(self.elements, self.output, self.surface), (54, 612)
        )

    def test_stale_frame_resize_or_ambiguous_button_cannot_produce_pointer_input(self):
        self.assertIsNone(
            ui.observed_point(
                self.elements, self.output, self.surface | {"height": 276}
            )
        )
        self.assertIsNone(
            ui.observed_point(
                self.elements + self.elements[1:], self.output, self.surface
            )
        )
        outside = self.elements[:1] + [
            self.elements[1] | {"bounds": {"x": 0, "y": 400, "width": 44, "height": 44}}
        ]
        with self.assertRaisesRegex(ControlError, "outside"):
            ui.observed_point(outside, self.output, self.surface)

    def test_selectors_include_toggle_buttons_but_not_the_shortcut_child_label(self):
        query = ui.selector(("Pan canvas",))
        self.assertIn("@role='toggle button'", query)
        self.assertIn("@name='Pan canvas'", query)
        self.assertNotIn("@name='H'", query)
        self.assertIn("Pan canvas, choose canvas tool", ui.TRIGGERS)

    def test_blank_target_excludes_client_frame_and_transformed_desktop_tiles(self):
        status = {
            "outputs": [self.output],
            "viewport": {"x": 100, "y": -50, "scale": 2},
            "windows": [{"x": 0, "y": 40, "width": 600, "height": 500, "mapped": True}],
            "frame_layout": {
                "outputs": [
                    {
                        "widgets": [
                            {"rect": {"x": 700, "y": 550, "width": 220, "height": 100}}
                        ]
                    }
                ]
            },
        }
        desktop = {"items": [{"x": 500, "y": 140}]}
        occupied = checks.blockers(status, desktop)
        self.assertIn((788, 368, 988, 584), occupied)
        start = checks.blank_start(status, desktop, (43, -27))
        path = (start[0] - 32, start[1] - 59, start[0] + 75, start[1] + 32)
        self.assertFalse(any(checks.overlaps(path, item) for item in occupied))
        full = {"x": -100, "y": 30, "width": 1280, "height": 800}
        with self.assertRaisesRegex(ControlError, "no observed blank"):
            checks.blank_start(status | {"windows": [full]}, desktop, (43, -27))

    def test_worker_ack_is_not_a_tool_snapshot(self):
        smoke = Mock()
        smoke.control.query.side_effect = [
            {"canvas_tool": "select"},
            {"canvas_tool": "pan"},
        ]

        def until(_name, condition):
            return condition() or condition()

        smoke.until.side_effect = until
        self.assertEqual(ui.wait_tool(smoke, "pan"), {"canvas_tool": "pan"})
        self.assertEqual(smoke.control.query.call_count, 2)

    def test_trigger_requires_native_pressed_state_before_accepting_selection(self):
        for title, name in (
            ("Select desktop items", "Select desktop items, choose canvas tool"),
            ("Pan canvas", "Pan canvas, choose canvas tool"),
            ("Pan canvas", "Canvas tools"),
        ):
            with self.subTest(name=name):
                smoke = Mock()
                target = ui.selector((f"{title}, choose canvas tool", "Canvas tools"))
                # Reduced native AT-SPI snapshots: unselected toggles omit
                # pressed; selected toggles have pressed=true and no checked.
                snapshots = iter(
                    ET.fromstring(
                        f'<Frame><ToggleButton name="{name}" role="toggle button" '
                        f'showing="true" {state}/></Frame>'
                    )
                    for state in ("", 'pressed="true"')
                )

                def find(_smoke, action, query, target=target, snapshots=snapshots):
                    self.assertEqual(action, "find")
                    self.assertTrue(query.startswith(target))
                    # Evaluate the real appended state predicate against XML
                    # at the external a11y boundary, without a running desktop.
                    predicate = query[len(target) :]
                    return {
                        "elements": [
                            dict(node.attrib)
                            for node in next(snapshots).findall(
                                "./ToggleButton" + predicate
                            )
                        ]
                    }

                def until(_name, condition):
                    self.assertFalse(condition(), "unselected trigger was accepted")
                    return condition()

                smoke.until.side_effect = until
                with patch("agentlib.canvas_pan_ui.a11y", side_effect=find):
                    selected = ui.wait_trigger(smoke, title)
                self.assertEqual(len(selected), 1)
                self.assertEqual(selected[0]["name"], name)
                self.assertEqual(selected[0]["pressed"], "true")


class PanLifecycleTests(unittest.TestCase):
    def smoke(self, directory):
        return CanvasPanSmoke(
            SimpleNamespace(
                session=Path(directory), timeout=30, total_timeout=120, log_level="info"
            )
        )

    def test_launcher_always_isolates_start_without_changing_stop(self):
        with tempfile.TemporaryDirectory() as directory:
            smoke = self.smoke(directory)
            with patch("agentlib.daily.DailySmoke.launcher") as launcher:
                smoke.launcher("start", "--no-build")
                smoke.launcher("stop")
            self.assertEqual(
                launcher.call_args_list[0].args,
                ("start", "--no-build", "--isolated-profile"),
            )
            self.assertEqual(launcher.call_args_list[1].args, ("stop",))

    def test_failed_press_capture_releases_the_real_input_sequence(self):
        smoke = Mock()
        smoke.control.path = "/test-owned-control"
        smoke.capture.side_effect = [
            ({"scale": 1}, b""),
            ControlError("page flip failed"),
        ]
        with (
            patch("agentlib.canvas_pan_fixture.blank", return_value=(600, 500)),
            patch("agentlib.input.Control") as cleanup,
            self.assertRaisesRegex(ControlError, "page flip failed"),
        ):
            pan(smoke, "capture-fails")
        cleanup.return_value.request.assert_called_once_with("pointer-button-up 272")

    def test_startup_failure_and_workflow_interruption_both_restore_owned_session(self):
        for phase in ("start", "workflow"):
            with (
                self.subTest(phase=phase),
                tempfile.TemporaryDirectory() as directory,
                contextlib.redirect_stdout(io.StringIO()),
            ):
                smoke = self.smoke(directory)
                operations = []

                def launcher(
                    operation,
                    *_args,
                    operations=operations,
                    smoke=smoke,
                    phase=phase,
                    **_kwargs,
                ):
                    operations.append(operation)
                    (smoke.directory / "session.json").write_text(
                        json.dumps({"owned": True})
                    )
                    if phase == "start" and operation == "start":
                        raise ControlError("partial startup failed")

                with (
                    patch("agentlib.daily.shutil.which", return_value="/test"),
                    patch.object(smoke, "launcher", side_effect=launcher),
                    patch.object(smoke, "workflow", side_effect=KeyboardInterrupt),
                    patch.object(smoke, "verify_restored") as restored,
                    self.assertRaises(
                        ControlError if phase == "start" else KeyboardInterrupt
                    ),
                ):
                    smoke.run()
                self.assertEqual(operations, ["start", "stop"])
                restored.assert_called_once()


if __name__ == "__main__":
    unittest.main()
