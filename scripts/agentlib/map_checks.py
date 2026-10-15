"""Observed shell and canvas-map pointer targets in compositor coordinates."""

import math

from .control import ControlError
from .daily_checks import a11y
from .input import BUTTONS, Input

SHELL = "//*[@role='application' and @name='hearthspace-shell']"
MINIMAP = f"({SHELL}//*[@showing='true' and starts-with(@name,'Canvas minimap:')])[1]"
WINDOW_FIELDS = (
    "id",
    "canvas_x",
    "canvas_y",
    "content_width",
    "content_height",
    "maximized",
    "fullscreen",
    "presentation_output",
)


def window_geometry(status):
    return {
        item["id"]: {key: item.get(key) for key in WINDOW_FIELDS}
        for item in status["windows"]
    }


def map_image(smoke):
    return smoke.until(
        "accessible live canvas minimap",
        lambda: a11y(smoke, "find", MINIMAP)["elements"],
    )[0]


def selector(name, *, role=None):
    role_filter = f"@role='{role}' and " if role else ""
    return f"({SHELL}//*[{role_filter}@showing='true' and @name='{name}'])[1]"


def button_center(bounds, output, surface=None):
    # AccessKit reports physical coordinates local to each Wayland surface.
    # Compositor status supplies the actual global logical surface placement.
    if not bounds or any(
        not math.isfinite(bounds.get(key, float("nan")))
        for key in ("x", "y", "width", "height")
    ):
        raise ControlError("shell button has no finite observed bounds")
    if bounds["width"] <= 0 or bounds["height"] <= 0:
        raise ControlError("shell button has empty observed bounds")
    if not math.isfinite(output["scale"]) or output["scale"] <= 0:
        raise ControlError("output has no usable scale")
    surface = surface or {
        "x": output["logical_x"],
        "y": output["logical_y"],
        "width": output["logical_width"],
        "height": 48,
    }
    x = (bounds["x"] + bounds["width"] / 2) / output["scale"]
    y = (bounds["y"] + bounds["height"] / 2) / output["scale"]
    if not (0 <= x < surface["width"] and 0 <= y < surface["height"]):
        raise ControlError("observed button center lies outside its shell surface")
    return x + surface["x"], y + surface["y"]


def map_surface(smoke):
    status = smoke.control.status()
    maps = status.get("canvas_maps", [])
    if len(maps) != 1:
        raise ControlError("canvas-map acceptance requires one mapped popover")
    surface = maps[0]
    output = next(
        (item for item in status["outputs"] if item["name"] == surface["output"]),
        None,
    )
    if output is None:
        raise ControlError("canvas map has no matching output")
    return surface, output


def click(smoke, name, *, on_press=None):
    target = selector(name, role="button")
    elements = smoke.until(
        f"observed shell {name} button",
        lambda: a11y(smoke, "find", target)["elements"],
    )
    if name == "Canvas map":
        outputs = smoke.control.status()["outputs"]
        if len(outputs) != 1:
            raise ControlError("canvas-map acceptance requires one active output")
        output, surface = outputs[0], None
    else:
        surface, output = map_surface(smoke)
    point = button_center(elements[0].get("bounds"), output, surface)
    smoke.record(
        "shell-pointer-target", selector=target, elements=elements, point=point
    )
    with Input(smoke.control) as inputs:
        inputs.motion(*point)
        inputs.down("pointer-button", BUTTONS["left"])
        if on_press:
            on_press()
        inputs.up("pointer-button", BUTTONS["left"])


def opened(smoke):
    return smoke.until(
        "canvas map mapped", lambda: smoke.control.status().get("canvas_maps")
    )


def closed(smoke):
    smoke.until(
        "canvas map dismissed", lambda: not smoke.control.status().get("canvas_maps")
    )


def close(smoke):
    click(smoke, "Close canvas map")
    closed(smoke)
