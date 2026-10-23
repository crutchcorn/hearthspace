"""Drag the viewport frame observed in the actual rendered minimap."""

import math
import time

from . import map_checks as checks
from .control import ControlError
from .input import Input
from .pinch_touchscreen import settled
from .png import pixels


def accent_viewport(capture, surface, bounds):
    metadata, png = capture
    width, height, channels, rows = pixels(png)
    scale = metadata["scale"]
    left = (surface["x"] - metadata["logical_x"]) * scale + bounds["x"]
    top = (surface["y"] - metadata["logical_y"]) * scale + bounds["y"]
    right, bottom = left + bounds["width"], top + bounds["height"]
    if not (0 <= left < right <= width and 0 <= top < bottom <= height):
        raise ControlError("minimap lies outside its captured output")
    points = []
    for y in range(math.ceil(top), math.floor(bottom)):
        for x in range(math.ceil(left), math.floor(right)):
            color = rows[y][x * channels : x * channels + 3]
            if all(
                abs(actual - target) <= 6
                for actual, target in zip(color, (233, 100, 42))
            ):
                points.append((x, y))
    if len(points) < 16:
        raise ControlError("minimap has no clearly rendered orange viewport frame")
    xs, ys = zip(*points)
    rect = (min(xs), min(ys), max(xs) + 1, max(ys) + 1)
    if rect[2] - rect[0] < 8 or rect[3] - rect[1] < 8:
        raise ControlError("rendered viewport is too small for a precise drag")
    return rect


def verify_drag(before, after, delta, projected_size, output):
    if not math.isclose(before["scale"], after["scale"], abs_tol=1e-9):
        raise ControlError("minimap drag changed canvas zoom")
    expected = {}
    for key, movement, projected, screen_extent in zip(
        ("x", "y"),
        delta,
        projected_size,
        (output["logical_width"], output["logical_height"]),
    ):
        # The measured frame includes its two-logical-pixel stroke. Infer the
        # canvas ratio from displayed geometry rather than duplicate fit logic.
        estimate = movement * screen_extent / before["scale"] / (projected - 2)
        actual = after[key] - before[key]
        if not 0.65 * estimate <= actual <= 1.35 * estimate:
            raise ControlError(
                f"minimap {key} drag moved canvas by {actual}, expected about {estimate}"
            )
        expected[key] = estimate
    return expected


def run(smoke, *, touchscreen=False):
    surface, output = checks.map_surface(smoke)
    element = checks.map_image(smoke)
    kind = "touchscreen" if touchscreen else "pointer"
    image = smoke.capture(f"map-{kind}-drag-before", output["name"])
    rect = accent_viewport(image, surface, element["bounds"])
    scale = output["scale"]
    start = (
        output["logical_x"] + (rect[0] + rect[2]) / 2 / scale,
        output["logical_y"] + (rect[1] + rect[3]) / 2 / scale,
    )
    delta = (12, 9)
    before = smoke.control.status()
    if touchscreen:
        from .map_touch import drag

        delta = drag(smoke, output, start, delta)
    else:
        with Input(smoke.control) as inputs:
            inputs.drag(*start, start[0] + delta[0], start[1] + delta[1], duration=0.35)
    after = smoke.until(
        "minimap drag pans the canvas right and down",
        lambda: (
            status
            if not (status := smoke.control.status())["viewport"]["animating"]
            and status["viewport"]["x"] > before["viewport"]["x"] + 1
            and status["viewport"]["y"] > before["viewport"]["y"] + 1
            else None
        ),
    )
    settled(smoke)
    after = smoke.control.status()
    expected = verify_drag(
        before["viewport"],
        after["viewport"],
        delta,
        ((rect[2] - rect[0]) / scale, (rect[3] - rect[1]) / scale),
        output,
    )
    if checks.window_geometry(before) != checks.window_geometry(after):
        raise ControlError("minimap viewport drag moved an application window")
    if not after.get("canvas_maps"):
        raise ControlError("minimap viewport drag dismissed its popover")
    # A subsequent pointer motion must not continue the released pointer/touch
    # grab. Keep it inside the map so dismissal cannot mask a stuck drag.
    with Input(smoke.control) as inputs:
        inputs.motion(start[0] + delta[0] + 20, start[1] + delta[1] + 12)
    time.sleep(min(0.1, smoke.control.deadline.remaining()))
    released = smoke.control.status()["viewport"]
    if any(released[key] != after["viewport"][key] for key in ("x", "y", "scale")):
        raise ControlError("released minimap drag continued on pointer motion")
    smoke.capture(f"map-{kind}-drag-after", output["name"])
    smoke.record(
        "map-viewport-drag",
        device=kind,
        before=before["viewport"],
        after=after["viewport"],
        accent_frame=rect,
        pointer_delta=delta,
        expected_canvas_delta=expected,
    )
    return after["viewport"]
