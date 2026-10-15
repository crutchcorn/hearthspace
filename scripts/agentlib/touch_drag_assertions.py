"""State and pixel assertions for touch-owned move/resize pointer grabs."""

from .control import ControlError
from .firefox_assertions import restored
from .pinch_assertions import same_viewport
from .png import crop
from .visual import marker_bounds, pixel_rect, verify_move


def unchanged(before, after):
    if after is None or any(
        abs(before[key] - after[key]) > 1
        for key in ("x", "y", "width", "height", "canvas_x", "canvas_y")
    ):
        raise ControlError("window grab remained active after the touchscreen released")


def viewport_unchanged(before, after):
    if after["pinch_active"] or not same_viewport(before, after):
        raise ControlError("single-contact window drag changed the canvas viewport")


def moved(before, after, delta, scale):
    if not restored(after):
        raise ControlError("touch drag requires an ordinary restored window")
    for axis, requested in zip(("x", "y"), delta):
        if abs(after[axis] - before[axis] - requested) > 3:
            raise ControlError(
                "window did not follow the touchscreen's screen distance"
            )
        key = f"canvas_{axis}"
        if abs((after[key] - before[key]) * scale - requested) > 3:
            raise ControlError("touchscreen drag used the wrong canvas scale")
    if any(abs(after[key] - before[key]) > 1 for key in ("width", "height")):
        raise ControlError("touchscreen move unexpectedly resized the window")


def move_pixels(before, after, image, captured):
    marker = verify_move(image, captured, before, after)
    original = marker_bounds(image, before)
    if any(
        abs((marker[axis + 2] - marker[axis]) - (original[axis + 2] - original[axis]))
        > 2
        for axis in (0, 1)
    ):
        raise ControlError("touchscreen move changed the rendered marker size")
    return {"marker_bounds": marker}


def resized(before, after, amount):
    if not restored(after) or abs(before["width"] - after["width"] - amount) > 3:
        raise ControlError("touchscreen did not resize Firefox by its screen distance")
    if any(abs(before[key] - after[key]) > 1 for key in ("x", "y", "height")):
        raise ControlError("touchscreen right-edge resize changed another edge")


def resize_pixels(before, after, image, captured):
    old, new = marker_bounds(image, before), marker_bounds(captured, after)
    if any(abs(a - b) > 1 for a, b in zip(old, new)):
        raise ControlError("right-edge resize moved or resized the fixed page marker")
    right = before["x"] + before["width"] - 8
    left = after["x"] + after["width"] + 8
    y = after["y"] + after["height"] * 0.7
    region = (left, y, right, y + 16)
    if left >= right:
        raise ControlError("touchscreen resize did not vacate a visible strip")
    previous = crop(image[1], pixel_rect(image[0], region))
    current = crop(captured[1], pixel_rect(captured[0], region))
    if not previous or not current or previous == current:
        raise ControlError("touchscreen resize left stale pixels in the vacated strip")
    return {"marker_bounds": new, "vacated_logical_region": region}
