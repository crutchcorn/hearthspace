"""Fixture identity must reach toolkit metadata, not only its status JSON."""

import json
import os
import runpy
import sys
import tempfile
import unittest
from pathlib import Path
from types import ModuleType, SimpleNamespace
from unittest.mock import Mock, patch

FIXTURE = Path(__file__).parent / "fixtures" / "gtk_touch.py"


def toolkit(major, mapped):
    """GI boundary model: GTK3 copies HeaderBar:title on attachment.

    Custom title widgets do not populate HeaderBar:title. Missing native titles
    fall back to GLib's application name. Real Wayland publication is covered
    by the separately owned native acceptance run.
    """
    names, pins, callbacks = {}, {}, []

    class Widget:
        def __init__(self, **_kwargs):
            self.title = None
            self.header = None
            self.mapped = False

        def __getattr__(self, _name):
            return Mock()

        def get_mapped(self):
            return self.mapped

        def set_title(self, value):
            self.title = value
            if major == "3" and self.header:
                self.header.title = value

        def get_title(self):
            return self.title

        def set_titlebar(self, header):
            self.header = header
            if major == "3":
                self.title = header.title

        def present(self):
            self.header.mapped = True
            mapped.append(
                {
                    "app_id": names["prgname"],
                    "title": self.title or names["application"],
                }
            )

        show_all = present

    class Window(Widget):
        def __init__(self, *, title=None):
            super().__init__()
            self.title = title

    gtk = SimpleNamespace(
        Window=Window,
        HeaderBar=Widget,
        Label=Widget,
        Box=Widget,
        Button=Widget,
        Grid=Widget,
        CssProvider=Widget,
        EventControllerKey=Widget,
        Orientation=SimpleNamespace(VERTICAL=1),
        PropagationPhase=SimpleNamespace(CAPTURE=1),
        STYLE_PROVIDER_PRIORITY_APPLICATION=600,
        init=lambda: None,
    )
    glib = SimpleNamespace(
        set_prgname=lambda value: names.update(prgname=value),
        set_application_name=lambda value: names.update(application=value),
        timeout_add=lambda _ms, callback: callbacks.append(callback),
        MainLoop=lambda: SimpleNamespace(
            run=lambda: [callback() for callback in callbacks]
        ),
    )
    modules = {"Gtk": gtk, "Gdk": SimpleNamespace(), "GLib": glib}

    class Repository(ModuleType):
        def __getattr__(self, name):
            if name not in modules:
                raise AttributeError(name)
            if pins != {"Gtk": f"{major}.0", "Gdk": f"{major}.0"}:
                raise AssertionError(
                    "Gtk and Gdk versions must be pinned before import"
                )
            return modules[name]

    gi = ModuleType("gi")
    gi.require_version = lambda name, version: pins.update({name: version})
    gi.repository = Repository("gi.repository")
    return {"gi": gi, "gi.repository": gi.repository}


class GtkFixtureIdentityTests(unittest.TestCase):
    def check_identity(self, major):
        mapped = []
        app_id = f"gtk_touch.py-unique-gtk{major}"
        title = f"Hearthspace GTK{major} touch fixture unique-token"
        with tempfile.TemporaryDirectory() as directory:
            state_path = Path(directory) / "state.json"
            env = {
                "HEARTHSPACE_GTK_TOUCH_VERSION": major,
                "HEARTHSPACE_GTK_TOUCH_ID": "unique-token",
                "HEARTHSPACE_GTK_TOUCH_APP_ID": app_id,
                "HEARTHSPACE_GTK_TOUCH_TITLE": title,
                "HEARTHSPACE_GTK_TOUCH_STATE": str(state_path),
            }
            with (
                patch.dict(os.environ, env),
                patch.dict(sys.modules, toolkit(major, mapped)),
            ):
                fixture = runpy.run_path(str(FIXTURE))
            self.assertEqual(mapped, [{"app_id": app_id, "title": title}])
            state = json.loads(state_path.read_text())
            self.assertEqual(state["app_id"], app_id)
            self.assertEqual(state["title"], title)
            # Status must reflect actual toolkit state, not environment intent.
            fixture["window"].set_title("unexpected toolkit title")
            fixture["save_state"]()
            self.assertEqual(
                json.loads(state_path.read_text())["title"], "unexpected toolkit title"
            )

    def test_gtk3_header_attachment_preserves_unique_mapped_title(self):
        self.check_identity("3")

    def test_gtk4_preserves_unique_mapped_title(self):
        self.check_identity("4")


if __name__ == "__main__":
    unittest.main()
