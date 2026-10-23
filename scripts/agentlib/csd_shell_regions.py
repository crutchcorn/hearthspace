"""Collect live shell regions while the caller deliberately reveals the shell."""

import math

from .control import ControlError
from .daily_checks import a11y
from .shell_surfaces import (
    CONTROL_WIDGETS,
    WIDGET_CORNERS,
    corner_surface,
    observed_status,
    output_layout,
    widget_surface,
)

SHELL = "//*[@role='application' and @name='hearthspace-shell']"
CORNERS = ("notifications", "system", "canvas", "apps")
SYSTEM_WIDGETS = {name for name, corner in WIDGET_CORNERS.items() if corner == "system"}


def _usable(rect):
    return (
        isinstance(rect, dict)
        and all(
            isinstance(rect.get(key), (int, float)) and math.isfinite(rect[key])
            for key in ("x", "y", "width", "height")
        )
        and rect["width"] > 0
        and rect["height"] > 0
    )


def _inside(rect, owner):
    return (
        _usable(rect)
        and owner["x"]
        <= rect["x"]
        < rect["x"] + rect["width"]
        <= owner["x"] + owner["width"]
        and owner["y"]
        <= rect["y"]
        < rect["y"] + rect["height"]
        <= owner["y"] + owner["height"]
    )


def _members(status):
    regions, seen = {}, set()
    for item in output_layout(status)["widgets"]:
        widget = item["id"]
        if widget not in SYSTEM_WIDGETS:
            continue
        if widget in seen:
            raise ControlError(f"duplicate {widget} system widget")
        seen.add(widget)
        # Unavailable optional devices have no mapped rectangle. A member may
        # also be deliberately hidden; neither supplies live pixel evidence.
        if item.get("hidden") or not _usable(item.get("rect")):
            continue
        # Use widget identity, not group membership: users can place a system
        # widget independently, outside the default system group's rectangle.
        regions[f"system-{widget}"] = widget_surface(status, widget)
    return regions


def _legacy(smoke, status, system):
    output = status["outputs"][0]
    scale = output["scale"]
    if not math.isfinite(scale) or scale <= 0:
        raise ControlError("system detail requires a usable output scale")
    # Native frame titles are optional. Identify the system owner by its volume
    # control, then include its controls and frame in the same accessibility read.
    target = (
        f"{SHELL}//*[(@role='button' or @role='toggle button') "
        "and @showing='true' and @name='Sound settings']"
    )
    owner = f"{target}/ancestor::*[@role='frame'][1]"
    names = " or ".join(
        f"@name='{name}'"
        for name, widget in CONTROL_WIDGETS.items()
        if widget in SYSTEM_WIDGETS
    )
    query = (
        f"{owner} | {owner}//*[@showing='true' and "
        f"(@role='button' or @role='toggle button' or {names} "
        "or starts-with(@name,'Battery:'))]"
    )
    elements = a11y(smoke, "find", query)["elements"]
    after = observed_status(smoke)
    if (
        "frame_layout" in after
        or after["outputs"] != status["outputs"]
        or any(
            corner_surface(after, name) != corner_surface(status, name)
            for name in CORNERS
        )
    ):
        raise ControlError("shell geometry changed while collecting system detail")
    frames = [item for item in elements if item.get("role") == "Frame"]
    if len(frames) != 1 or not _usable(frames[0].get("bounds")):
        raise ControlError("system detail requires one accessible owning frame")
    frame = frames[0]["bounds"]
    if any(frame[key] != system[key] * scale for key in ("width", "height")):
        raise ControlError("system detail has stale accessible frame bounds")
    regions = {}
    for index, item in enumerate(elements):
        if item.get("role") == "Frame":
            continue
        name = item.get("name", "")
        widget = CONTROL_WIDGETS.get(name)
        if name.startswith("Battery:"):
            widget = "battery"
        # Tray application labels are dynamic and absent from CONTROL_WIDGETS.
        key = (
            f"system-{widget}"
            if widget in SYSTEM_WIDGETS
            else f"system-control-{name or index}"
        )
        if key in regions:
            raise ControlError(f"duplicate {key} accessible system control")
        bounds = item.get("bounds")
        if not _usable(bounds):
            raise ControlError(f"{key} has no usable accessible bounds")
        rect = {
            "x": system["x"] + bounds["x"] / scale,
            "y": system["y"] + bounds["y"] / scale,
            "width": bounds["width"] / scale,
            "height": bounds["height"] / scale,
            "output": system["output"],
            "corner": "system",
        }
        if not _inside(rect, system):
            raise ControlError(
                f"{key} accessible bounds lie outside its system surface"
            )
        regions[key] = rect
    return regions


def surfaces(smoke):
    """Return global logical rectangles; fail rather than accept group-only evidence.

    The caller owns reveal/hide input and must call this while the shell is
    revealed. Keys retain the four corner purposes plus ``system-<widget>``
    members (or ``system-control-<label>`` for legacy tray application controls).
    """
    status = observed_status(smoke)
    regions = {name: corner_surface(status, name) for name in CORNERS}
    output = status["outputs"][0]
    extent = {
        "x": output["logical_x"],
        "y": output["logical_y"],
        "width": output["logical_width"],
        "height": output["logical_height"],
    }
    for name, rect in regions.items():
        if not _inside(rect, extent):
            raise ControlError(
                f"{name} shell region has no usable live output rectangle"
            )
    members = (
        _members(status)
        if "frame_layout" in status
        else _legacy(smoke, status, regions["system"])
    )
    if "system-volume" not in members:
        raise ControlError(
            "shell system detail requires a live individual volume region"
        )
    for name, rect in members.items():
        if not _inside(rect, extent):
            raise ControlError(f"{name} shell region lies outside its live output")
    return regions | members
