"""Private client accessibility transport and window-relative target geometry."""

import ast
import subprocess

from .control import ControlError


def call(smoke, address, owner, path, method, *arguments):
    result = subprocess.run(
        [
            "gdbus",
            "call",
            "--address",
            address,
            "--dest",
            owner,
            "--object-path",
            path,
            "--method",
            method,
            "--",
            *arguments,
        ],
        capture_output=True,
        text=True,
        check=True,
        timeout=smoke.control.deadline.remaining(),
    )
    return ast.literal_eval(result.stdout)[0]


def bus(smoke):
    return call(
        smoke,
        smoke.manifest["dbus_session_bus_address"],
        "org.a11y.Bus",
        "/org/a11y/bus",
        "org.a11y.Bus.GetAddress",
    )


def local_center(bounds, window, scale, origin=(0, 0)):
    x, y, width, height = bounds
    x, y = x - origin[0], y - origin[1]
    if width <= 0 or height <= 0 or x < 0 or y < 0:
        raise ControlError("client control has no window-relative accessible bounds")
    if (
        x + width > window["content_width"] + 1
        or y + height > window["content_height"] + 1
    ):
        raise ControlError("accessible control lies outside its client window")
    return window["x"] + (x + width / 2) * scale, window["y"] + (y + height / 2) * scale


def presentation_scale(window, viewport):
    return 1 if window.get("output_coordinates") else viewport["scale"]
