"""Browser chrome needs live semantics and a presented input repaint."""

import sys
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import Mock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib import firefox_focused
from agentlib.control import ControlError
from test_agent_visual import capture


class FirefoxChromeObserverTests(unittest.TestCase):
    def setUp(self):
        self.metadata = {
            "width": 200,
            "height": 160,
            "scale": 1,
            "logical_x": 0,
            "logical_y": 0,
        }
        self.window = {
            "mapped": True,
            "visible": True,
            "focused": True,
            "focused_view": True,
            "fullscreen": False,
            "maximized": True,
            "output_coordinates": True,
            "presentation_output": "native",
            "titlebar_height": 0,
            "x": 0,
            "y": 0,
            "width": 200,
            "height": 160,
            "content_width": 200,
            "content_height": 160,
        }
        self.output = {
            "name": "native",
            "logical_x": 0,
            "logical_y": 0,
            "logical_width": 200,
            "logical_height": 160,
        }
        self.original = "file:///owned/zoom.html"
        self.current = self.original
        self.entry = {"bus_name": "private-firefox", "object_path": "/address"}
        self.restore = {
            "name": "Restore Down",
            "role": "button",
            "showing": True,
            "bus_name": "private-firefox",
            "object_path": "/restore",
        }
        self.duplicate = {**self.restore, "object_path": "/duplicate"}
        self.restores = [self.restore]
        self.entries = [self.entry]
        self.restore_bounds = (153, 0, 37, 44)
        self.extents = {
            "/address": Mock(return_value=(10, 10, 170, 30)),
            "/restore": Mock(return_value=self.restore_bounds),
            "/duplicate": Mock(return_value=(153, -12, 37, 24)),
        }
        self.samples = []
        self.make_captures()

    def make_captures(self):
        self.before = capture(self.metadata, [((10, 10, 180, 40), (245, 245, 245))])
        self.after = capture(
            self.metadata,
            [
                ((10, 10, 180, 40), (245, 245, 245)),
                ((15, 15, 100, 30), (30, 30, 30)),
            ],
        )

    def until(self, _label, observe):
        for _ in range(4):
            self.samples.append(value := observe())
            if value:
                return value
        raise ControlError("bounded chrome observer expired")

    def run_chrome(self, images):
        smoke = self.smoke = SimpleNamespace(
            target=7,
            record=Mock(),
            capture=Mock(side_effect=images),
            until=self.until,
            window=lambda _id: self.window,
            control=SimpleNamespace(status=lambda: {"outputs": [self.output]}),
        )

        selectors = {
            f"{firefox_focused.FIREFOX}//*[@role='button' and @showing='true' and @name='Restore Down']": self.restores,
            f"{firefox_focused.FIREFOX}//*[@showing='true' and @editable='true']": self.entries,
        }

        def query(_smoke, action, selector):
            self.assertEqual(action, "find")
            return {"elements": selectors[selector]}

        def call(_smoke, address, owner, path, method, *args):
            self.assertEqual(address, "private-atspi")
            self.assertEqual(owner, "private-firefox")
            if method == "org.a11y.atspi.Text.GetText":
                self.assertEqual(path, "/address")
                self.assertEqual(args, ("0", "-1"))
                return self.current
            self.assertEqual(method, "org.a11y.atspi.Component.GetExtents")
            self.assertEqual(args, ("1",))
            return self.extents[path]()

        def chord(value):
            if value == "esc":
                self.current = self.original

        def type_text(value):
            self.current = value

        inputs = Mock(chord=Mock(side_effect=chord), type=Mock(side_effect=type_text))
        with (
            patch.object(firefox_focused, "a11y", side_effect=query),
            patch.object(firefox_focused, "bus", return_value="private-atspi"),
            patch.object(firefox_focused, "call", side_effect=call),
            patch.object(firefox_focused, "Input") as self.input_context,
        ):
            self.input_context.return_value.__enter__.return_value = inputs
            firefox_focused.chrome(smoke)
        return smoke, inputs

    def test_clipped_duplicate_uses_live_bounds_and_repaints_then_restores_url(self):
        self.metadata.update(width=1920, height=1080)
        self.window.update(
            width=1920, height=1080, content_width=1920, content_height=1080
        )
        self.output.update(logical_width=1920, logical_height=1080)
        self.make_captures()
        self.restore_bounds = (1843, 0, 37, 44)
        self.extents["/restore"].return_value = self.restore_bounds
        self.extents["/duplicate"].return_value = (1843, -12, 37, 24)
        # Snapshot geometry points to the wrong node; only live WINDOW bounds count.
        self.restore["bounds"] = {"x": 1843, "y": -12, "width": 37, "height": 24}
        self.duplicate["bounds"] = {"x": 1843, "y": 0, "width": 37, "height": 44}
        self.restores = [self.duplicate, self.restore]
        smoke, inputs = self.run_chrome([self.before, self.before, self.after])
        evidence = smoke.record.call_args.kwargs
        self.assertEqual(smoke.record.call_args.args, ("csd-focused-client-chrome",))
        self.assertEqual(evidence["restore"], self.restore)
        self.assertEqual(evidence["restore_bounds"], self.restore_bounds)
        self.assertEqual(evidence["entry"], self.entry)
        self.assertEqual(evidence["bounds"], (10, 10, 170, 30))
        for extents in self.extents.values():
            extents.assert_called_once_with()
        self.assertEqual(self.current, self.original)
        self.assertGreater(evidence["changed_pixels"], 100)
        self.assertEqual(smoke.capture.call_count, 3)
        self.assertEqual(
            [call.args[0] for call in inputs.type.call_args_list],
            ["iiiiiiiiiiiiiiii CSD chrome", "MMMMMMMMMMMMMMMM CSD chrome"],
        )
        self.assertEqual(inputs.chord.call_args_list[-1].args, ("esc",))
        self.assertIn(None, self.samples)

    def test_restore_extents_are_refreshed_until_exactly_one_is_contained(self):
        self.restores = [self.restore, self.duplicate]
        self.restore["bounds"] = {"x": 153, "y": 0, "width": 37, "height": 44}
        self.duplicate["bounds"] = {"x": 153, "y": -12, "width": 37, "height": 24}
        final_bounds = (150, 0, 37, 44)
        self.extents["/restore"].side_effect = [
            (153, -12, 37, 24),
            self.restore_bounds,
            final_bounds,
        ]
        self.extents["/duplicate"].side_effect = [
            (153, -12, 37, 24),
            (110, 0, 37, 44),
            (153, -12, 37, 24),
        ]
        smoke, _inputs = self.run_chrome([self.before, self.after])
        self.assertEqual(self.samples[:2], [None, None])
        self.assertEqual(self.samples[2], (self.restore, final_bounds, self.entry))
        self.assertEqual(self.extents["/restore"].call_count, 3)
        self.assertEqual(self.extents["/duplicate"].call_count, 3)
        self.assertEqual(smoke.record.call_args.kwargs["restore_bounds"], final_bounds)
        self.assertEqual(self.current, self.original)

    def assert_rejected_before_input(self):
        self.samples.clear()
        with self.assertRaisesRegex(ControlError, "bounded chrome"):
            self.run_chrome([])
        self.assertEqual(self.samples, [None] * 4)
        self.input_context.assert_not_called()
        self.smoke.capture.assert_not_called()
        self.smoke.record.assert_not_called()
        self.assertEqual(self.current, self.original)

    def test_ambiguous_or_invalid_live_restore_bounds_cannot_pass(self):
        # Outer dimensions must not permit geometry outside the client content.
        self.window.update(width=400, height=320)
        cases = {
            "multiple contained": [self.restore_bounds, (110, 0, 37, 44)],
            "only clipped": [(153, -12, 37, 24)],
            "empty extents": [()],
            "incomplete extents": [(153, 0, 37)],
            "zero width": [(153, 0, 0, 44)],
            "zero height": [(153, 0, 37, 0)],
            "negative width": [(153, 0, -37, 44)],
            "negative height": [(153, 0, 37, -44)],
            "NaN coordinate": [(float("nan"), 0, 37, 44)],
            "NaN size": [(153, 0, 37, float("nan"))],
            "infinite coordinate": [(153, float("inf"), 37, 44)],
            "infinite size": [(153, 0, float("inf"), 44)],
            "negative infinite coordinate": [(float("-inf"), 0, 37, 44)],
            "left overflow": [(-1, 0, 37, 44)],
            "right overflow": [(164, 0, 37, 44)],
            "bottom overflow": [(153, 117, 37, 44)],
        }
        for name, bounds in cases.items():
            with self.subTest(name=name):
                self.restores = [
                    {**self.restore, "object_path": f"/restore-{index}"}
                    for index in range(len(bounds))
                ]
                for node, rectangle in zip(self.restores, bounds):
                    self.extents[node["object_path"]] = Mock(return_value=rectangle)
                self.assert_rejected_before_input()

    def test_missing_or_ambiguous_editable_entry_cannot_pass(self):
        for entries in ([], [self.entry, {**self.entry, "object_path": "/other"}]):
            with self.subTest(entry_count=len(entries)):
                self.entries = entries
                self.assert_rejected_before_input()

    def test_exact_received_text_cannot_pass_with_frozen_chrome_pixels(self):
        with self.assertRaisesRegex(ControlError, "bounded chrome"):
            self.run_chrome([self.before] * 5)
        self.assertEqual(self.current, self.original)

    def test_missing_restore_control_cannot_pass_as_retained_chrome(self):
        self.restores = []
        self.assert_rejected_before_input()

    def test_initial_shell_proof_runs_before_address_bar_input(self):
        with (
            patch.object(
                firefox_focused,
                "focused_hidden",
                side_effect=ControlError("initial shell visible"),
            ),
            patch.object(firefox_focused, "chrome") as chrome,
            self.assertRaisesRegex(ControlError, "initial shell visible"),
        ):
            firefox_focused.evidence(SimpleNamespace())
        chrome.assert_not_called()


if __name__ == "__main__":
    unittest.main()
