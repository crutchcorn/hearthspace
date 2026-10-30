"""Independent geometry and presented-pixel assertions for mouse canvas pan."""

import math

from .control import ControlError
from .map_checks import window_geometry
from .visual import marker_bounds


def expected_offset(view, delta):
    """Round the complete origin-relative transform, like Rust f64::round."""
    scale = view["scale"]
    if not math.isfinite(scale) or scale <= 0:
        raise ControlError("pan requires a positive finite canvas scale")
    result = {}
    for axis, distance in zip(("x", "y"), delta, strict=True):
        value = view[axis] - distance / scale
        if not math.isfinite(value):
            raise ControlError("pan coordinates must be finite")
        result[axis] = math.floor(value + 0.5) if value >= 0 else math.ceil(value - 0.5)
    return result


def same_view(before, after):
    return all(before[key] == after[key] for key in ("x", "y", "scale"))


def verify_pan(before, after, delta):
    expected = expected_offset(before["viewport"], delta)
    if any(after["viewport"][axis] != value for axis, value in expected.items()):
        raise ControlError(
            f"pan offset differs from origin - delta / scale: {expected}"
        )
    if after["viewport"]["scale"] != before["viewport"]["scale"]:
        raise ControlError("mouse pan changed the canvas scale")
    if window_geometry(before) != window_geometry(after):
        raise ControlError("mouse pan changed application canvas geometry")
    return expected


def verify_marker(before, after, old_window, new_window, old_view, new_view):
    old, new = marker_bounds(before, old_window), marker_bounds(after, new_window)
    for axis, index in (("x", 0), ("y", 1)):
        expected = (old_view[axis] - new_view[axis]) * old_view["scale"]
        old_position = old[index] / before[0]["scale"] + before[0][f"logical_{axis}"]
        new_position = new[index] / after[0]["scale"] + after[0][f"logical_{axis}"]
        if abs(new_position - old_position - expected) > 1:
            raise ControlError("presented terminal marker did not follow the pan")
        old_size = (old[index + 2] - old[index]) / before[0]["scale"]
        new_size = (new[index + 2] - new[index]) / after[0]["scale"]
        if abs(old_size - new_size) > 1:
            raise ControlError("pure mouse pan resized the presented terminal marker")
    return {"before_marker": old, "after_marker": new}


def overlaps(a, b):
    return a[0] < b[2] and b[0] < a[2] and a[1] < b[3] and b[1] < a[3]


def rect(item):
    return item["x"], item["y"], item["x"] + item["width"], item["y"] + item["height"]


def blockers(status, desktop):
    """Screen geometry, independent of canvas zoom and output pixel scale."""
    result = [
        rect(item)
        for key in ("windows", "shell_corners", "shell_panels", "canvas_maps")
        for item in status.get(key, [])
        if item.get("mapped", True) and item.get("visible", True)
    ]
    for layout in status.get("frame_layout", {}).get("outputs", []):
        result.extend(
            rect(item["rect"])
            for key in ("widgets", "groups")
            for item in layout.get(key, [])
            if not item.get("hidden", False)
        )
    view = status["viewport"]
    # desktop-status reports canonical tile origins; common::desktop supplies
    # the 88x96 content size and 6px transparent focus-ring margin.
    for item in desktop["items"]:
        result.append(
            (
                (item["x"] - 6 - view["x"]) * view["scale"],
                (item["y"] - 6 - view["y"]) * view["scale"],
                (item["x"] + 94 - view["x"]) * view["scale"],
                (item["y"] + 102 - view["y"]) * view["scale"],
            )
        )
    return result


def blank_start(status, desktop, delta):
    if len(status["outputs"]) != 1:
        raise ControlError("mouse pan acceptance requires one active output")
    output = status["outputs"][0]
    bounds = (
        output["logical_x"],
        output["logical_y"],
        output["logical_x"] + output["logical_width"],
        output["logical_y"] + output["logical_height"],
    )
    occupied = blockers(status, desktop)
    for fy in (0.7, 0.5, 0.3, 0.85, 0.15):
        for fx in (0.75, 0.5, 0.25, 0.9, 0.1):
            x = bounds[0] + output["logical_width"] * fx
            y = bounds[1] + output["logical_height"] * fy
            end_x, end_y = x + delta[0], y + delta[1]
            path = (
                min(x, end_x) - 32,
                min(y, end_y) - 32,
                max(x, end_x) + 32,
                max(y, end_y) + 32,
            )
            if (
                bounds[0] <= path[0] < path[2] <= bounds[2]
                and bounds[1] <= path[1] < path[3] <= bounds[3]
                and not any(overlaps(path, item) for item in occupied)
            ):
                return x, y
    raise ControlError("no observed blank canvas path clear of clients and shell")
