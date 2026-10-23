"""CSD acceptance must reject state-only, stale-rendering and ownership false passes."""

import json
import sys
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import Mock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib.content_pixels import content_changed
from agentlib.control import ControlError, Deadline
from agentlib.gtk_titlebar import gesture, target
from agentlib.gtk_titlebar_assertions import rendered, state
from agentlib.gtk_touch import GtkTouchSmoke
from agentlib.gtk_touch_checks import local_center, presentation_scale
from test_agent_visual import capture


class CsdStateEvidenceTests(unittest.TestCase):
    def setUp(self):
        self.output = {
            "name": "native",
            "logical_x": -1280,
            "logical_y": 100,
            "logical_width": 1280,
            "logical_height": 800,
        }
        self.viewport = {"scale": 0.8}
        self.window = {
            "mapped": True,
            "visible": True,
            "focused": True,
            "focused_view": True,
            "fullscreen": False,
            "maximized": True,
            "output_coordinates": True,
            "presentation_output": "native",
            "x": -1280,
            "y": 100,
            "width": 1280,
            "height": 800,
            "content_width": 1280,
            "content_height": 800,
            "titlebar_height": 0,
            "canvas_x": 170,
            "canvas_y": 110,
            "maximized_canvas_bounds": None,
        }
        self.client = {
            "fullscreen": False,
            "maximized": True,
            "fullscreen_entries": 0,
            "header_mapped": True,
        }

    def verify(
        self, window=None, client=None, mode="focused", original=None, entries=0
    ):
        state(
            window or self.window,
            client or self.client,
            self.output,
            self.viewport,
            mode,
            original,
            entries,
        )

    def test_focused_mode_is_full_output_even_at_nonunit_canvas_zoom(self):
        self.verify()
        self.assertEqual(presentation_scale(self.window, self.viewport), 1)
        self.assertEqual(
            local_center(
                (100, 30, 80, 20),
                self.window,
                presentation_scale(self.window, self.viewport),
            ),
            (-1140, 140),
        )
        self.assertEqual(
            presentation_scale(
                self.window | {"output_coordinates": False}, self.viewport
            ),
            0.8,
        )

    def test_rejects_fullscreen_masquerading_as_focus_and_missing_client_ack(self):
        for field, value in (
            ("fullscreen", True),
            ("focused_view", False),
            ("focused", False),
            ("maximized", False),
        ):
            with self.subTest(field=field), self.assertRaises(ControlError):
                self.verify(window=self.window | {field: value})
        for change in (
            {"fullscreen": True},
            {"maximized": False},
            {"fullscreen_entries": 1},
            {"header_mapped": False},
        ):
            with self.subTest(change=change), self.assertRaises(ControlError):
                self.verify(client=self.client | change)

    def test_rejects_wrong_output_stale_committed_size_and_incomplete_bounds(self):
        for change in (
            {"presentation_output": "other"},
            {"x": 0},
            {"content_width": 520},
            {"content_height": 380},
            {"height": 798},
            {"output_coordinates": False},
            {"titlebar_height": 20, "content_height": 780},
        ):
            with self.subTest(change=change), self.assertRaises(ControlError):
                self.verify(window=self.window | change)

    def test_explicit_protocol_fullscreen_retains_focus_and_returns_without_new_entry(
        self,
    ):
        self.verify(
            window=self.window | {"fullscreen": True},
            client=self.client
            | {"fullscreen": True, "fullscreen_entries": 1, "header_mapped": False},
            mode="fullscreen",
            entries=1,
        )
        self.verify(client=self.client | {"fullscreen_entries": 1}, entries=1)
        with self.assertRaisesRegex(ControlError, "unexpected application fullscreen"):
            self.verify(client=self.client | {"fullscreen_entries": 2}, entries=1)

    def test_canvas_maximize_is_bounded_and_committed_at_canvas_scale(self):
        window = self.window | {
            "focused_view": False,
            "output_coordinates": False,
            "presentation_output": None,
            "x": -1256,
            "y": 196,
            "width": 1232,
            "height": 608,
            "content_width": 1540,
            "content_height": 760,
            "maximized_canvas_bounds": {
                "x": 30,
                "y": 120,
                "width": 1540,
                "height": 760,
            },
        }
        self.verify(window=window, mode="canvas")
        for change in (
            {"maximized_canvas_bounds": None},
            {"height": 800},
            {"focused_view": True},
            {"presentation_output": "native"},
        ):
            with self.subTest(change=change), self.assertRaises(ControlError):
                self.verify(window=window | change, mode="canvas")

    def test_restore_must_recover_the_original_home_and_client_size(self):
        original = {
            "canvas_x": 170,
            "canvas_y": 110,
            "content_width": 520,
            "content_height": 380,
        }
        window = (
            self.window
            | original
            | {
                "focused_view": False,
                "maximized": False,
                "output_coordinates": False,
                "presentation_output": None,
                "width": 416,
                "height": 304,
            }
        )
        client = self.client | {"maximized": False}
        self.verify(window, client, "floating", original)
        with self.assertRaisesRegex(ControlError, "original floating geometry"):
            self.verify(window | {"canvas_x": 175}, client, "floating", original)

    def test_content_pixels_must_change_with_the_application_counter(self):
        metadata = {
            "width": 200,
            "height": 200,
            "scale": 1,
            "logical_x": 0,
            "logical_y": 0,
        }
        window = {"x": 20, "y": 20, "width": 160, "height": 160}
        image = capture(metadata, [((40, 40, 160, 160), (49, 93, 221))])
        self.assertGreater(rendered(image, window, (49, 93, 221)), 100)
        with self.assertRaisesRegex(ControlError, "absent or stale"):
            rendered(image, window, (39, 179, 64))

    def test_generic_text_input_rejects_frozen_editor_and_caret_only_changes(self):
        metadata = {
            "width": 200,
            "height": 200,
            "scale": 1,
            "logical_x": 0,
            "logical_y": 0,
        }
        background = [((20, 20, 180, 180), (0, 204, 102))]
        before = capture(metadata, background)
        caret = capture(metadata, background + [((40, 40, 42, 60), (0, 0, 0))])
        text = capture(metadata, background + [((40, 40, 80, 60), (0, 0, 0))])
        for image in (before, caret):
            with (
                self.subTest(image=image is before),
                self.assertRaisesRegex(ControlError, "did not repaint"),
            ):
                content_changed(before, image, (20, 20, 180, 180))
        self.assertGreater(content_changed(before, text, (20, 20, 180, 180)), 100)


class CsdHarnessEvidenceTests(unittest.TestCase):
    def test_targets_are_recaptured_and_rediscovered_after_resize(self):
        events = []
        floating = {
            "x": 100,
            "y": 80,
            "content_width": 520,
            "content_height": 380,
            "maximized": False,
            "output_coordinates": False,
        }
        focused = {
            "x": 0,
            "y": 0,
            "content_width": 1280,
            "content_height": 800,
            "maximized": True,
            "output_coordinates": True,
        }
        smoke = SimpleNamespace(
            kind="gtk4",
            target=1,
            manifest={"dbus_session_bus_address": "private"},
            capture=lambda name: events.append("capture"),
            record=Mock(),
            window=Mock(side_effect=[floating, floating, focused, focused]),
            control=SimpleNamespace(status=lambda: {"viewport": {"scale": 0.8}}),
            until=lambda _label, operation: operation(),
        )
        bounds = iter([(430, 8, 32, 28), (1100, 8, 32, 28)])

        def find(_smoke, _action, query):
            events.append(query)
            return {"elements": [{"bus_name": "fixture", "object_path": "/button"}]}

        def call(_smoke, _address, _owner, _path, method, *_args):
            return "private-atspi" if method.endswith("GetAddress") else next(bounds)

        with (
            patch("agentlib.gtk_touch_checks.a11y", side_effect=find),
            patch("agentlib.client_accessibility.call", side_effect=call),
        ):
            self.assertEqual(target(smoke, "maximize", False), (456.8, 97.6))
            self.assertEqual(target(smoke, "restore", False), (1116, 22))
        self.assertEqual(events[::2], ["capture", "capture"])
        self.assertIn("@name='Maximize'", events[1])
        self.assertIn("@name='Restore'", events[3])

    def test_double_tap_uses_two_kernel_contact_sequences(self):
        device = Mock()
        smoke = SimpleNamespace(
            output={
                "logical_x": 0,
                "logical_y": 0,
                "logical_width": 1280,
                "logical_height": 800,
            },
            control=SimpleNamespace(
                deadline=Deadline(5), path="/unused-control", request=Mock()
            ),
            record=Mock(),
        )
        with (
            patch("agentlib.gtk_titlebar.target", return_value=(400, 200)),
            patch("agentlib.gtk_titlebar.time.sleep"),
        ):
            gesture(smoke, device, "double-tap", True, True)
        self.assertEqual(
            [call.args[0] for call in device.request.call_args_list],
            ["begin", "end", "begin", "end"],
        )
        self.assertEqual(device.request.call_args_list[0].kwargs["fingers"], 1)

    def test_attach_never_starts_or_stops_session_even_after_failure(self):
        with tempfile.TemporaryDirectory() as directory:
            session = Path(directory)
            manifest = session / "session.json"
            manifest.write_text(json.dumps({"owner": "root"}))
            (session / "artifacts").mkdir()
            prior = session / "artifacts/results.json"
            prior.write_text("root evidence")
            args = SimpleNamespace(
                session=session, titlebar_maximize=True, timeout=10, total_timeout=20
            )
            first, second = GtkTouchSmoke(args), GtkTouchSmoke(args)
            self.assertNotEqual(first.artifacts, second.artifacts)
            with (
                patch.object(
                    first, "workflow", side_effect=ControlError("action failed")
                ),
                patch.object(
                    first, "launcher", side_effect=AssertionError("root owns lifecycle")
                ) as launcher,
            ):
                with self.assertRaisesRegex(ControlError, "action failed"):
                    first.run()
                launcher.assert_not_called()
            self.assertEqual(json.loads(manifest.read_text()), {"owner": "root"})
            self.assertEqual(prior.read_text(), "root evidence")


if __name__ == "__main__":
    unittest.main()
