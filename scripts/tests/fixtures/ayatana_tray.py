"""Private-session tray client using the installed Ayatana/GTK libraries.

This exercises an independent deployed implementation, in addition to the
deterministic Rust protocol fixture. No introspection typelib is required.
"""

import argparse
import ctypes
import json
import os
from pathlib import Path


def function(library, name, result, *arguments):
    value = getattr(library, name)
    value.restype = result
    value.argtypes = arguments
    return value


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--log", type=Path, required=True)
    args = parser.parse_args()
    bus = os.environ.get("HEARTHSPACE_SESSION_BUS_ADDRESS")
    if not bus or os.environ.get("DBUS_SESSION_BUS_ADDRESS") != bus:
        parser.error("requires the explicit, matching Hearthspace private bus")
    pointer = ctypes.c_void_p
    string = ctypes.c_char_p
    integer = ctypes.c_int
    gtk = ctypes.CDLL("libgtk-3.so.0")
    ayatana = ctypes.CDLL("libayatana-appindicator3.so.1")
    glib = ctypes.CDLL("libglib-2.0.so.0")
    gobject = ctypes.CDLL("libgobject-2.0.so.0")
    initialize = function(gtk, "gtk_init_check", integer, pointer, pointer)
    if not initialize(None, None):
        raise RuntimeError("GTK could not connect to the owned display")
    menu = function(gtk, "gtk_menu_new", pointer)()
    item = function(gtk, "gtk_menu_item_new_with_label", pointer, string)(
        b"Ayatana library action"
    )
    function(gtk, "gtk_menu_shell_append", None, pointer, pointer)(menu, item)
    function(gtk, "gtk_widget_show_all", None, pointer)(menu)
    indicator = function(
        ayatana, "app_indicator_new", pointer, string, string, integer
    )(b"hearthspace-ayatana-fixture", b"dialog-information", 0)
    function(ayatana, "app_indicator_set_title", None, pointer, string)(
        indicator, b"Ayatana tray fixture"
    )
    function(ayatana, "app_indicator_set_menu", None, pointer, pointer)(indicator, menu)
    function(ayatana, "app_indicator_set_status", None, pointer, integer)(indicator, 1)

    def record(event):
        with args.log.open("a") as stream:
            stream.write(json.dumps({"event": event}) + "\n")

    @ctypes.CFUNCTYPE(None, pointer, pointer)
    def activated(_widget, _data):
        record("activated")

    function(
        gobject,
        "g_signal_connect_data",
        ctypes.c_ulong,
        pointer,
        string,
        pointer,
        pointer,
        pointer,
        integer,
    )(item, b"activate", activated, None, None, 0)
    loop = function(glib, "g_main_loop_new", pointer, pointer, integer)(None, 0)
    record("ready")
    function(glib, "g_main_loop_run", None, pointer)(loop)


if __name__ == "__main__":
    main()
