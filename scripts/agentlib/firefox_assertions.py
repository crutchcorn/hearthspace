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
        and not window.get("presentation_output")
    )


def maximized(window):
    return (
        window
        and window.get("mapped")
        and window.get("visible")
        and window.get("maximized")
        and not window.get("fullscreen")
        and window.get("presentation_output")
    )


def maximized_at_output(window, outputs):
    """A state flag alone may precede the client's maximized configure commit."""
    if not maximized(window):
        return False
    # Keep aligned with compositor config::CONTROL_BAR_HEIGHT. Firefox draws CSD.
    return matches_output_bounds(window, outputs, top_inset=48)


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


def matches_output_bounds(window, outputs, top_inset=0):
    output = next(
        (item for item in outputs if item["name"] == window["presentation_output"]),
        None,
    )
    if output is None:
        return False
    expected = {
        "x": output["logical_x"],
        "y": output["logical_y"] + top_inset,
        "width": output["logical_width"],
        "height": max(1, output["logical_height"] - top_inset),
    }
    return all(abs(window[key] - value) <= 1 for key, value in expected.items())


def marker_size(capture, window):
    bounds = marker_bounds(capture, window)
    return [(bounds[axis + 2] - bounds[axis]) / capture[0]["scale"] for axis in (0, 1)]


def verify_zoom(before_view, after_view, before, after, before_image, after_image):
    ratio = after_view["scale"] / before_view["scale"]
    if not maximized(before) or not restored(after):
        raise ControlError("canvas zoom did not restore the maximized Firefox window")
    if not 0.45 < ratio < 0.95:
        raise ControlError("canvas zoom did not shrink by a measurable amount")
    if any(after[key] >= before[key] - 8 for key in ("width", "height")):
        raise ControlError("restored Firefox did not visibly shrink on both axes")
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
