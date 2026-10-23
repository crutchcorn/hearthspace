"""Firefox state and rendered-marker assertions, independent of input method."""

from .control import ControlError
from .visual import marker_bounds


def restored(window):
    return (
        window
        and window.get("mapped")
        and window.get("visible")
        and not window.get("maximized")
        and not window.get("fullscreen")
        and not window.get("focused_view")
        and not window.get("presentation_output")
    )


def maximized(window):
    return (
        window
        and window.get("mapped")
        and window.get("visible")
        and window.get("maximized")
        and not window.get("fullscreen")
        and not window.get("focused_view")
        and not window.get("presentation_output")
        and not window.get("output_coordinates")
    )


def restored_client_size(window, saved):
    """Cleared state flags can precede the restored client's buffer commit."""
    return bool(
        restored(window)
        and all(
            window[key] == saved[key] for key in ("content_width", "content_height")
        )
    )


def verify_fullscreen_exit(before_view, after_view, before, after):
    if not maximized(before) or not maximized(after):
        raise ControlError("F11 exit did not restore saved canvas maximization")
    unchanged_canvas_geometry(before, after)
    # F11 permits canvas navigation behind the pinned client. Restore its world
    # bounds through the current viewport instead of reusing old screen bounds.
    ratio = after_view["scale"] / before_view["scale"]
    for axis in ("x", "y"):
        world = before[axis] / before_view["scale"] + before_view[axis]
        expected = (world - after_view[axis]) * after_view["scale"]
        if abs(after[axis] - expected) > 2:
            raise ControlError("F11 exit did not restore projected canvas position")
    if any(abs(after[key] - before[key] * ratio) > 2 for key in ("width", "height")):
        raise ControlError("F11 exit did not restore projected canvas size")
    return {"restore_scale_ratio": ratio}


def maximized_at_output(window, outputs):
    """Prove entry fits an output's workarea, before subsequent canvas travel."""
    if not maximized(window):
        return False
    return any(matches_workarea(window, output) for output in outputs)


def focused_at_output(window, outputs):
    """Client CSD maximize fills the output without advertising fullscreen."""
    return bool(
        window
        and all(
            window.get(key)
            for key in (
                "mapped",
                "visible",
                "focused",
                "focused_view",
                "maximized",
                "output_coordinates",
            )
        )
        and window.get("fullscreen") is False
        and window.get("titlebar_height") == 0
        and matches_output_bounds(window, outputs)
        and all(
            abs(window[f"content_{key}"] - window[key]) <= 1
            for key in ("width", "height")
        )
    )


def fullscreen_at_output(window, outputs):
    if not (
        window
        and window.get("mapped")
        and window.get("visible")
        and window.get("fullscreen")
        and window.get("presentation_output")
    ):
        return False
    return matches_output_bounds(window, outputs)


def workarea_bounds(output):
    # Website clearances apply to outer logical bounds, including native SSD.
    compact = output["logical_width"] <= 760
    side = min(12 if compact else 24, max(0, output["logical_width"] - 1) // 2)
    vertical = min(78 if compact else 96, max(0, output["logical_height"] - 1) // 2)
    return {
        "x": output["logical_x"] + side,
        "y": output["logical_y"] + vertical,
        "width": max(1, output["logical_width"] - 2 * side),
        "height": max(1, output["logical_height"] - 2 * vertical),
    }


def matches_workarea(window, output):
    return all(
        abs(window[key] - value) <= 1 for key, value in workarea_bounds(output).items()
    )


def matches_output_bounds(window, outputs):
    output = next(
        (item for item in outputs if item["name"] == window["presentation_output"]),
        None,
    )
    if output is None:
        return False
    expected = {
        "x": output["logical_x"],
        "y": output["logical_y"],
        "width": output["logical_width"],
        "height": output["logical_height"],
    }
    return all(abs(window[key] - value) <= 1 for key, value in expected.items())


def marker_size(capture, window):
    bounds = marker_bounds(capture, window)
    return [(bounds[axis + 2] - bounds[axis]) / capture[0]["scale"] for axis in (0, 1)]


def unchanged_canvas_geometry(before, after):
    bounds = before.get("maximized_canvas_bounds")
    if bounds is None or bounds != after.get("maximized_canvas_bounds"):
        raise ControlError("canvas travel changed or lost the maximized world bounds")
    if any(
        before[key] != after[key]
        for key in ("canvas_x", "canvas_y", "content_width", "content_height")
    ):
        raise ControlError(
            "canvas travel changed the maximized window's world geometry"
        )


def verify_zoom(before_view, after_view, before, after, before_image, after_image):
    ratio = after_view["scale"] / before_view["scale"]
    if not maximized(before) or not maximized(after):
        raise ControlError("canvas zoom did not preserve Firefox's canvas maximization")
    unchanged_canvas_geometry(before, after)
    if not 0.45 < ratio < 0.95:
        raise ControlError("canvas zoom did not shrink by a measurable amount")
    if any(abs(after[key] - before[key] * ratio) > 2 for key in ("width", "height")):
        raise ControlError("maximized Firefox bounds did not scale on both axes")
    for axis in ("x", "y"):
        world = before[axis] / before_view["scale"] + before_view[axis]
        expected = (world - after_view[axis]) * after_view["scale"]
        if abs(after[axis] - expected) > 2:
            raise ControlError("maximized Firefox position did not follow canvas zoom")
    old, new = marker_size(before_image, before), marker_size(after_image, after)
    if any(
        abs(new_size - old_size * ratio) > 3 for old_size, new_size in zip(old, new)
    ):
        raise ControlError("presented Firefox marker did not scale with the canvas")
    return {"scale_ratio": ratio, "before_marker_size": old, "after_marker_size": new}


def verify_resize(before, after, amount):
    if not restored(before) or not restored(after):
        raise ControlError("edge resize must preserve restored canvas state")
    if abs((before["width"] - after["width"]) - amount) > 20:
        raise ControlError("Firefox edge resize did not track the pointer distance")
    if any(abs(before[key] - after[key]) > 4 for key in ("x", "y", "height")):
        raise ControlError(
            "right-edge resize unexpectedly moved or resized another edge"
        )


def verify_fullscreen(before, after, before_image, after_image):
    if not before.get("fullscreen") or not after.get("fullscreen"):
        raise ControlError("canvas zoom changed Firefox's explicit fullscreen state")
    if not after.get("presentation_output"):
        raise ControlError("fullscreen Firefox lost its output assignment")
    if any(abs(before[key] - after[key]) > 1 for key in ("x", "y", "width", "height")):
        raise ControlError("fullscreen Firefox moved or scaled with the canvas")
    old, new = marker_size(before_image, before), marker_size(after_image, after)
    if any(abs(a - b) > 2 for a, b in zip(old, new)):
        raise ControlError("fullscreen Firefox marker changed size during canvas zoom")
    return {"before_marker_size": old, "after_marker_size": new}
