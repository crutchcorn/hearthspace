"""Exercise fullscreen evidence with real observers and substituted I/O boundaries."""

import json
import sys
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import Mock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib.control import ControlError
from agentlib.gtk_titlebar import evidence
from agentlib.gtk_touch_checks import accessible, content
from test_agent_visual import capture


class GtkFullscreenObserverTests(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.window = {
            "id": 7,
            "mapped": True,
            "visible": True,
            "focused": True,
            "focused_view": True,
            "maximized": True,
            "fullscreen": True,
            "output_coordinates": True,
            "presentation_output": "native",
            "x": 0,
            "y": 0,
            "width": 320,
            "height": 240,
            "content_width": 320,
            "content_height": 240,
            "titlebar_height": 0,
        }
        self.identity = {
            "fixture_id": "owned",
            "app_id": "gtk_touch.py-owned",
            "title": "owned GTK",
        }
        self.state = {
            **self.identity,
            "pid": 1234,
            "count": 1,
            "fullscreen": True,
            "maximized": True,
            "fullscreen_entries": 1,
            "header_mapped": False,
        }
        self.process = Mock(pid=1234)
        self.process.poll.return_value = None
        self.smoke = SimpleNamespace(
            kind="gtk3",
            target=7,
            gtk_count=1,
            fullscreen_entries=1,
            client_process=self.process,
            client_identity=self.identity,
            client_state=Path(directory.name) / "state.json",
            artifacts=Path(directory.name),
            results=[],
            manifest={"dbus_session_bus_address": "private-bus"},
            output={
                "name": "native",
                "logical_x": 0,
                "logical_y": 0,
                "logical_width": 320,
                "logical_height": 240,
            },
            control=SimpleNamespace(
                status=lambda: {"viewport": {"scale": 0.8}}, capture=Mock()
            ),
            window=lambda _target: self.window,
            capture=self.capture,
            record=Mock(),
            until=self.until,
        )
        self.persist()
        self.queries = []
        self.contacts = []
        self.receives_touch = True
        self.updates_status = True
        self.repaints = True
        self.a11y_count = 1
        self.presented_count = 1
        self.device = SimpleNamespace(request=self.request)
        self.enterContext(
            patch("agentlib.gtk_touch_checks.a11y", side_effect=self.a11y)
        )
        self.enterContext(
            patch("agentlib.client_accessibility.call", side_effect=self.call)
        )

    @staticmethod
    def until(label, condition):
        for _ in range(3):
            if value := condition():
                return value
        raise ControlError(f"timed out waiting for {label}")

    def persist(self):
        self.smoke.client_state.write_text(json.dumps(self.state))

    def request(self, action, **_kwargs):
        self.contacts.append(action)
        if action == "end" and self.receives_touch:
            self.a11y_count += 1
            if self.updates_status:
                self.state["count"] += 1
                self.persist()
            if self.repaints:
                self.presented_count += 1

    def capture(self, _label):
        color = (49, 93, 221) if self.presented_count % 2 else (39, 179, 64)
        return capture(
            {"width": 320, "height": 240, "scale": 1, "logical_x": 0, "logical_y": 0},
            [((40, 100, 280, 210), color)],
        )

    def a11y(self, _smoke, action, query=None):
        if action == "snapshot":
            return {"snapshot": "<application/>"}
        self.queries.append(query)
        if "fixture titlebar" in query:
            return {"elements": []}
        if "Touch count:" in query and f"Touch count: {self.a11y_count}" not in query:
            return {"elements": []}
        return {"elements": [{"bus_name": "fixture", "object_path": "/button"}]}

    @staticmethod
    def call(_smoke, address, _owner, _path, method, *_args):
        if method.endswith("GetAddress"):
            if address != "private-bus":
                raise AssertionError("accessibility escaped the private bus")
            return "private-atspi"
        if address != "private-atspi":
            raise AssertionError("wrong AT-SPI bus")
        return (30, 50, 260, 30)

    def observe(self):
        evidence(self.smoke, self.device, "fullscreen", "fullscreen", None)

    def test_gtk3_fullscreen_interacts_with_content_without_hidden_header(self):
        self.observe()
        self.assertEqual(self.contacts, ["begin", "end"])
        self.assertEqual(self.smoke.gtk_count, 2)
        self.assertEqual(self.state["count"], 2)
        self.assertFalse(any("fixture titlebar" in query for query in self.queries))
        button_query = next(query for query in self.queries if "Count touch" in query)
        self.assertIn("@name='gtk_touch.py-owned'", button_query)

    def test_gtk4_fullscreen_also_requires_new_input(self):
        self.smoke.kind = "gtk4"
        self.observe()
        self.assertEqual(self.contacts, ["begin", "end"])
        self.assertEqual(self.smoke.gtk_count, 2)

    def test_initial_focused_shell_is_observed_before_content_can_hide_it(self):
        self.window["fullscreen"] = False
        self.state.update(fullscreen=False, header_mapped=True)
        self.persist()
        with (
            patch(
                "agentlib.gtk_titlebar.focused_hidden",
                side_effect=ControlError("initial shell visible"),
            ),
            self.assertRaisesRegex(ControlError, "initial shell visible"),
        ):
            evidence(self.smoke, self.device, "focused", "focused", None)
        self.assertEqual(self.contacts, [])
        self.assertEqual(self.smoke.gtk_count, 1)

    def test_fullscreen_flags_and_old_marker_cannot_pass_without_input(self):
        self.receives_touch = False
        with self.assertRaisesRegex(ControlError, "Touch count: 2"):
            self.observe()
        self.assertEqual(self.smoke.gtk_count, 1)

    def test_fresh_counter_cannot_pass_with_preceding_fullscreen_marker(self):
        self.repaints = False
        with self.assertRaisesRegex(ControlError, "current application marker"):
            self.observe()
        self.assertEqual(self.smoke.gtk_count, 2)
        self.assertEqual(self.state["count"], 2)

    def test_a11y_increment_cannot_replace_owned_fixture_acknowledgement(self):
        self.updates_status = False
        with self.assertRaisesRegex(ControlError, "Touch count: 2"):
            content(self.smoke, self.device, "fullscreen")
        self.assertEqual(self.a11y_count, 2)
        self.assertEqual(self.smoke.gtk_count, 1)

    def test_counter_drift_fails_before_dispatching_input(self):
        self.state["count"] = 3
        self.persist()
        with self.assertRaisesRegex(ControlError, "counter disagrees"):
            content(self.smoke, self.device, "fullscreen")
        self.assertEqual(self.contacts, [])

    def test_gtk3_normal_window_still_requires_current_titlebar_geometry(self):
        self.window["fullscreen"] = False
        self.assertIsNone(accessible(self.smoke, "//*[@name='Count touch']"))
        self.assertTrue(any("fixture titlebar" in query for query in self.queries))


if __name__ == "__main__":
    unittest.main()
