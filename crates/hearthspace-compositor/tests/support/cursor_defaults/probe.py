"""Read the real libXcursor preference without creating a window or cursor.

Run in a fresh process for each case: Xlib and libXcursor cache display defaults.
Only the requested size is asserted, so the installed cursor theme is irrelevant.
"""

import argparse
import ctypes
import json
import signal


def function(library, name, result, *arguments):
    value = getattr(library, name)
    value.restype = result
    value.argtypes = arguments
    return value


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("display", help="explicit owned Xwayland display")
    args = parser.parse_args()
    signal.alarm(10)
    pointer = ctypes.c_void_p
    string = ctypes.c_char_p
    integer = ctypes.c_int
    x11 = ctypes.CDLL("libX11.so.6")
    xcursor = ctypes.CDLL("libXcursor.so.1")
    close_display = function(x11, "XCloseDisplay", integer, pointer)
    default_screen = function(x11, "XDefaultScreen", integer, pointer)
    width = function(x11, "XDisplayWidth", integer, pointer, integer)
    height = function(x11, "XDisplayHeight", integer, pointer, integer)
    get_default = function(x11, "XGetDefault", string, pointer, string, string)
    get_size = function(xcursor, "XcursorGetDefaultSize", integer, pointer)
    display = function(x11, "XOpenDisplay", pointer, string)(args.display.encode())
    if not display:
        raise RuntimeError(f"cannot connect to owned display {args.display}")
    try:
        screen = default_screen(display)
        resource = get_default(display, b"Xcursor", b"size")
        print(
            json.dumps(
                {
                    "requested_size": get_size(display),
                    "resource_size": resource.decode() if resource else None,
                    "screen_size": [width(display, screen), height(display, screen)],
                }
            ),
            flush=True,
        )
    finally:
        close_display(display)


if __name__ == "__main__":
    main()
