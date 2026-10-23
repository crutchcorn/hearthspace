"""GTK CSD client: ordinary HeaderBar gestures and observed toolkit state."""

import json
import os
from pathlib import Path

import gi

major = os.environ.get("HEARTHSPACE_GTK_TOUCH_VERSION", "3")
gi.require_version("Gtk", f"{major}.0")
gi.require_version("Gdk", f"{major}.0")
from gi.repository import Gdk, GLib, Gtk

app_id = os.environ.get("HEARTHSPACE_GTK_TOUCH_APP_ID", "gtk_touch.py")
title_text = os.environ.get(
    "HEARTHSPACE_GTK_TOUCH_TITLE", f"Hearthspace GTK{major} touch fixture"
)
identity = {
    "fixture_id": os.environ.get("HEARTHSPACE_GTK_TOUCH_ID"),
    "pid": os.getpid(),
    "app_id": app_id,
    "title": title_text,
}
GLib.set_prgname(app_id)
GLib.set_application_name(app_id)
Gtk.init()
state_path = os.environ.get("HEARTHSPACE_GTK_TOUCH_STATE")
loop = GLib.MainLoop()
window = Gtk.Window()
window.set_default_size(520, 380)
header = Gtk.HeaderBar()
title = Gtk.Label(label=f"GTK{major} touch fixture")
if major == "3":
    header.get_accessible().set_name("GTK3 fixture titlebar")
    header.set_custom_title(title)
    header.set_show_close_button(True)
else:
    header.set_title_widget(title)
    header.set_show_title_buttons(True)
# This changes only this fixture. The normal toolkit controls/gesture handlers
# remain intact; no application callback implements titlebar maximization.
header.set_decoration_layout(":minimize,maximize,close")
window.set_titlebar(header)
# GTK3 copies HeaderBar:title into Window:title when attaching the header.
# Set this afterward: a custom title widget does not populate that property.
window.set_title(title_text)
box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=20)
for side in ("top", "bottom", "start", "end"):
    getattr(box, f"set_margin_{side}")(24)
counter = Gtk.Label(label="Touch count: 0")
button = Gtk.Button(label="Count touch")
count = 0
observed = {"fullscreen": False, "maximized": False, "fullscreen_entries": 0}


def save_state(*_):
    if state_path:
        path = Path(state_path)
        temporary = path.with_suffix(".tmp")
        temporary.write_text(
            json.dumps(
                {
                    **observed,
                    **identity,
                    "title": window.get_title(),
                    "count": count,
                    "header_mapped": header.get_mapped(),
                }
            )
        )
        temporary.replace(path)
    return True


def observe(fullscreen, maximized):
    if fullscreen and not observed["fullscreen"]:
        observed["fullscreen_entries"] += 1
    observed.update(fullscreen=fullscreen, maximized=maximized)
    save_state()


def window_state(_window, event):
    observe(
        bool(event.new_window_state & Gdk.WindowState.FULLSCREEN),
        bool(event.new_window_state & Gdk.WindowState.MAXIMIZED),
    )
    return False


def key_pressed(key):
    if key == Gdk.KEY_F11:
        if observed["fullscreen"]:
            window.unfullscreen()
        else:
            window.fullscreen()
        return True
    return False


if major == "3":
    window.connect("window-state-event", window_state)
    window.connect("key-press-event", lambda _window, event: key_pressed(event.keyval))
    window.connect("destroy", lambda *_: loop.quit())
else:

    def state_changed(*_):
        observe(window.is_fullscreen(), window.is_maximized())

    window.connect("notify::fullscreened", state_changed)
    window.connect("notify::maximized", state_changed)
    keys = Gtk.EventControllerKey()
    keys.set_propagation_phase(Gtk.PropagationPhase.CAPTURE)
    keys.connect("key-pressed", lambda _keys, key, _code, _mods: key_pressed(key))
    window.add_controller(keys)
    window.connect("close-request", lambda *_: loop.quit())


def append(widget, expand=False):
    if major == "3":
        box.pack_start(widget, expand, expand, 0)
    else:
        widget.set_vexpand(expand)
        box.append(widget)


def clicked(_button):
    global count
    count += 1
    counter.set_text(f"Touch count: {count}")
    context = marker.get_style_context()
    if count % 2:
        context.add_class("touch-odd")
    else:
        context.remove_class("touch-odd")
    save_state()


button.connect("clicked", clicked)
append(counter)
append(button)
marker = Gtk.Grid(column_spacing=6, row_spacing=6)
marker.set_column_homogeneous(True)
marker.set_row_homogeneous(True)
style = Gtk.CssProvider()
css = (
    ".touch-light { background-color: #27b340; color: #fff; }"
    ".touch-dark { background-color: #194d26; color: #fff; }"
    ".touch-odd .touch-light { background-color: #315ddd; }"
)
if major == "3":
    style.load_from_data(css.encode())
else:
    style.load_from_string(css)
for row in range(4):
    for column in range(8):
        cell = Gtk.Label(label=f"{chr(65 + column)}{row + 1}")
        cell.set_size_request(44, 32)
        context = cell.get_style_context()
        context.add_class("touch-light" if (row + column) % 2 else "touch-dark")
        context.add_provider(style, Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION)
        marker.attach(cell, column, row, 1, 1)
append(marker, True)
GLib.timeout_add(25, save_state)
if major == "3":
    window.add(box)
    window.show_all()
else:
    window.set_child(box)
    window.present()
loop.run()
