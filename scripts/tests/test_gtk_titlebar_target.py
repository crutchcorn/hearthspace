"""GTK4's static maximize label must still resolve a physical restore target."""

import re
import sys
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import Mock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib import gtk_titlebar
from agentlib.control import ControlError, Deadline


class GtkRestoreTargetTests(unittest.TestCase):
    def check_restore(self, kind, button_name, expected_names):
        events, queries = [], []
        window = {
            "x": 0,
            "y": 0,
            "content_width": 1920,
            "content_height": 1080,
            "maximized": True,
            "fullscreen": False,
            "output_coordinates": True,
        }
        output = {
            "logical_x": 0,
            "logical_y": 0,
            "logical_width": 1920,
            "logical_height": 1080,
        }

        def until(_label, observe):
            found = observe()
            if not found:
                raise ControlError("owned restore target was not found")
            return found

        smoke = SimpleNamespace(
            kind=kind,
            target=1,
            client_identity={"app_id": "gtk_touch.py-owned"},
            manifest={"dbus_session_bus_address": "private-session"},
            output=output,
            window=lambda _target: window,
            until=until,
            capture=lambda label: events.append(label),
            record=Mock(),
            control=SimpleNamespace(
                path="/unused-control",
                deadline=Deadline(5),
                status=lambda: {"viewport": {"scale": 0.8}},
                request=events.append,
            ),
        )

        def find(_smoke, action, query):
            self.assertEqual(action, "find")  # No accessibility activation shortcut.
            self.assertIn("@name='gtk_touch.py-owned'", query)
            if "GTK3 fixture titlebar" in query:
                path = "/header"
            else:
                names = re.findall(r"@name='([^']+)'", query)[1:]
                queries.append(tuple(names))
                if button_name not in names:
                    return {"elements": []}
                path = "/button"
            return {"elements": [{"bus_name": "owned", "object_path": path}]}

        def call(_smoke, _address, _owner, path, method, *_args):
            if method.endswith("GetAddress"):
                return "private-atspi"
            # WINDOW-relative bounds match the captured maximized GTK4 header.
            return (0, 0, 1920, 47) if path == "/header" else (1827, 0, 40, 46)

        with (
            patch("agentlib.gtk_touch_checks.a11y", side_effect=find),
            patch("agentlib.client_accessibility.call", side_effect=call),
            patch.object(gtk_titlebar.time, "sleep"),
        ):
            gtk_titlebar.gesture(smoke, None, "restore", header=False, touch=False)
        self.assertEqual(queries, [expected_names])
        self.assertEqual(events[1], "restore-before")
        self.assertEqual(
            events[-3:],
            [
                "pointer-motion-abs 1847.0 23.0",
                "pointer-button-down 272",
                "pointer-button-up 272",
            ],
        )

    def test_gtk4_static_maximize_label_resolves_physical_restore_button(self):
        self.check_restore(
            "gtk4", "Maximize", ("Restore", "Unmaximize", "Restore Down", "Maximize")
        )

    def test_gtk3_keeps_its_existing_restore_names(self):
        self.check_restore("gtk3", "Restore", ("Restore", "Unmaximize", "Restore Down"))


if __name__ == "__main__":
    unittest.main()
