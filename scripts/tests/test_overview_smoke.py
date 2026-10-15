"""Prevent semantic-only overview success and retain safe native cleanup."""

import contextlib
import copy
import io
import json
import sys
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import Mock, call, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib import overview_input
from agentlib.control import ControlError
from agentlib.overview import OverviewSmoke
from agentlib.overview_assertions import COLORS, marker_count, presented, preserved
from test_agent_visual import capture


class OverviewPixelTests(unittest.TestCase):
    def setUp(self):
        self.metadata = {
            "width": 400,
            "height": 300,
            "scale": 2,
            "logical_x": -20,
            "logical_y": 10,
            "output": "test",
        }
        self.items = [
            {"window_id": 1, "x": 0, "y": 30, "width": 60, "height": 80},
            {"window_id": 2, "x": 80, "y": 30, "width": 60, "height": 80},
        ]
        for item in self.items:
            item.update(
                preview_x=item["x"], preview_y=40, preview_width=60, preview_height=50
            )
        self.status = {
            "viewport": {"x": 15, "y": -20, "scale": 0.7},
            "outputs": [
                {
                    "name": "test",
                    "logical_x": -20,
                    "logical_y": 10,
                    "logical_width": 200,
                    "logical_height": 150,
                }
            ],
            "overview": {"active": True, "output": "test", "items": self.items},
            "windows": [
                {
                    "id": n,
                    "mapped": True,
                    "canvas_x": 100 * n,
                    "canvas_y": 300,
                    "content_width": 900,
                    "content_height": 600,
                    "maximized": False,
                    "fullscreen": n == 1,
                    "presentation_output": "test" if n == 1 else None,
                }
                for n in (1, 2)
            ],
        }
        self.targets = {1: COLORS[0], 2: COLORS[1]}
        self.image = capture(
            self.metadata,
            [((10, 50, 30, 60), COLORS[0]), ((90, 50, 110, 60), COLORS[1])],
        )

    def test_each_thumbnail_requires_its_own_presented_client_marker(self):
        self.assertEqual(
            set(presented(self.status, self.image, self.targets)), {"1", "2"}
        )
        stale = capture(self.metadata, [((10, 50, 30, 60), COLORS[0])])
        with self.assertRaisesRegex(ControlError, "marker"):
            presented(self.status, stale, self.targets)
        swapped = capture(
            self.metadata,
            [((10, 50, 30, 60), COLORS[1]), ((90, 50, 110, 60), COLORS[0])],
        )
        with self.assertRaisesRegex(ControlError, "marker"):
            presented(self.status, swapped, self.targets)

    def test_marker_measurement_respects_output_origin_scale_and_bounds(self):
        self.assertEqual(marker_count(self.image, (10, 50, 30, 60), COLORS[0]), 800)
        self.assertEqual(marker_count(self.image, (90, 50, 110, 60), COLORS[0]), 0)

    def test_color_in_card_chrome_cannot_prove_client_preview(self):
        self.items[0]["preview_y"] = 70
        self.items[0]["preview_height"] = 20
        with self.assertRaisesRegex(ControlError, "marker"):
            presented(self.status, self.image, self.targets)

    def test_overlapping_off_output_or_duplicate_thumbnails_cannot_pass(self):
        for change in ({"x": 59}, {"x": -21}, {"width": 0}, {"window_id": 1}):
            state = copy.deepcopy(self.status)
            state["overview"]["items"][1].update(change)
            with self.subTest(change=change), self.assertRaises(ControlError):
                presented(state, self.image, self.targets)

    def test_missing_window_and_wrong_output_cannot_pass(self):
        for change in (
            {"items": self.items[:1]},
            {"active": False},
            {"output": "other"},
        ):
            state = copy.deepcopy(self.status)
            state["overview"].update(change)
            with self.subTest(change=change), self.assertRaises(ControlError):
                presented(state, self.image, self.targets)

    def test_presentation_geometry_and_viewport_survive_open_and_cancel(self):
        preserved(self.status, copy.deepcopy(self.status))
        for field, value in (
            ("canvas_x", 50),
            ("fullscreen", False),
            ("content_width", 300),
        ):
            state = copy.deepcopy(self.status)
            state["windows"][0][field] = value
            with (
                self.subTest(field=field),
                self.assertRaisesRegex(ControlError, "mutated"),
            ):
                preserved(self.status, state)
        state = copy.deepcopy(self.status)
        state["viewport"]["scale"] = 1
        with self.assertRaisesRegex(ControlError, "viewport"):
            preserved(self.status, state)
        preserved(self.status, state, viewport=False)


class OverviewInputTests(unittest.TestCase):
    def test_touchscreen_five_contacts_can_arrive_as_a_staggered_chord(self):
        device = Mock(kind="touchscreen")
        state = overview_input.begin(device, 5, stagger=True)
        self.assertEqual(len(state["points"]), 5)
        self.assertEqual(
            device.request.call_args_list[0],
            call("begin", fingers=1, points=state["points"][:1]),
        )
        self.assertEqual(device.request.call_count, 5)
        overview_input.move(device, state, 0.6)
        self.assertEqual(len(state["moved"]), 5)

    def test_outward_touchpad_uses_room_to_expand_all_five_contacts(self):
        device = Mock(kind="touchpad")
        state = overview_input.begin(device, 5, outward=True)
        overview_input.move(device, state, 1.5)
        self.assertEqual(
            device.request.call_args_list,
            [call("begin", fingers=5, distance=600), call("move", distance=900)],
        )

    def test_contact_change_cancels_only_supported_fixture_count(self):
        device = Mock(kind="touchscreen")
        overview_input.cancel(
            device, {"fingers": 4, "moved": [[0, 0], [2, 0], [2, 2], [0, 2]]}
        )
        device.request.assert_called_once_with("add-finger", point=(1, 1))
        with self.assertRaises(ControlError):
            overview_input.cancel(device, {"fingers": 5})

    def test_touch_trace_requires_exact_four_or_five_contact_count(self):
        for fingers in (4, 5):
            trace = "".join(f" event5 TOUCH_DOWN +1s {i}\n" for i in range(fingers))
            trace += " event5 TOUCH_MOTION +2s 0\n event5 TOUCH_UP +3s 0\n event5 TOUCH_FRAME +3s\n"
            path = Mock()
            path.read_text.return_value = trace
            self.assertEqual(
                overview_input.trace_proof(path, "touchscreen", fingers)["contacts"],
                fingers,
            )
            with self.assertRaisesRegex(ControlError, "contact count"):
                overview_input.trace_proof(path, "touchscreen", fingers, cancelled=True)


class OverviewLifecycleTests(unittest.TestCase):
    def test_failure_stops_and_verifies_only_the_owned_session(self):
        with (
            tempfile.TemporaryDirectory() as directory,
            contextlib.redirect_stdout(io.StringIO()),
        ):
            smoke = OverviewSmoke(
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
                    smoke, "workflow", side_effect=ControlError("overview proof failed")
                ),
                patch.object(smoke, "verify_restored") as restored,
                self.assertRaisesRegex(ControlError, "overview proof failed"),
            ):
                smoke.run()
            self.assertEqual([entry[0] for entry in operations], ["start", "stop"])
            self.assertIn("--no-build", operations[0][1])
            restored.assert_called_once()


if __name__ == "__main__":
    unittest.main()
