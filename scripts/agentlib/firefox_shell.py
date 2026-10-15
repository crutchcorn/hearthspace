"""Click the real shell while a maximized browser has distant saved geometry."""

import math

from . import firefox_assertions as verify
from . import firefox_checks as checks
from .control import ControlError
from .daily_checks import a11y
from .input import BUTTONS, Input
from .pinch_touchscreen import settled


def button_center(bounds, output):
    # AccessKit's Wayland shell extents use physical surface pixels. Agent
    # pointer input uses compositor-global logical coordinates instead.
    if not bounds or any(
        not math.isfinite(bounds.get(key, float("nan")))
        for key in ("x", "y", "width", "height")
    ):
        raise ControlError("shell button has no finite observed bounds")
    if bounds["width"] <= 0 or bounds["height"] <= 0:
        raise ControlError("shell button has empty observed bounds")
    x = (bounds["x"] + bounds["width"] / 2) / output["scale"]
    y = (bounds["y"] + bounds["height"] / 2) / output["scale"]
    if not (0 <= x < output["logical_width"] and 0 <= y < 48):
        raise ControlError("observed button center lies outside the shell control bar")
    return x + output["logical_x"], y + output["logical_y"]


def click_zoom_out(smoke):
    target = checks.selector("Zoom out", checks.SHELL)
    elements = smoke.until(
        "observed shell Zoom out button",
        lambda: a11y(smoke, "find", target)["elements"],
    )
    output = smoke.control.status()["outputs"][0]
    point = button_center(elements[0].get("bounds"), output)
    smoke.record(
        "shell-pointer-target", selector=target, elements=elements, point=point
    )
    with Input(smoke.control) as inputs:
        inputs.motion(*point)
        inputs.down("pointer-button", BUTTONS["left"])
        smoke.until(
            "shell pointer press clears Firefox focus",
            lambda: not smoke.window(smoke.target).get("focused"),
        )
        smoke.record("shell-pointer-focused", window=smoke.window(smoke.target))
        inputs.up("pointer-button", BUTTONS["left"])


def reset_pinned_view(smoke):
    # Reset centers a focused floating app and leaves a pinned app's offset
    # unchanged. Focus the shell for an actual origin reset, then refocus the
    # pinned browser without revealing its distant saved floating position.
    smoke.control.request("focus-launcher")
    smoke.control.request("reset-view")
    smoke.control.request(f"focus-window {smoke.target}")
    smoke.until(
        "maximized Firefox focused before shell click",
        lambda: smoke.window(smoke.target).get("focused"),
    )
    view = settled(smoke)
    if any(view[key] != value for key, value in (("x", 0), ("y", 0), ("scale", 1))):
        raise ControlError("shell zoom setup failed to reset the distant viewport")
    return view


def run(smoke):
    if not verify.restored(smoke.window(smoke.target)):
        raise ControlError("shell zoom setup requires the previously restored Firefox")
    smoke.control.request(f"move-window {smoke.target} 100000 100000")
    smoke.until(
        "distant saved floating Firefox position",
        lambda: (
            (window := smoke.window(smoke.target))
            and not window["visible"]
            and window["canvas_x"] == window["canvas_y"] == 100000
        ),
    )
    # Focus reveals the real CSD Maximize control. Resetting the view afterwards
    # puts the saved floating location offscreen again while max stays pinned.
    before = checks.prepare_maximized(smoke)
    before_view = reset_pinned_view(smoke)
    smoke.record("shell-distant-maximized", window=before, viewport=before_view)
    image = smoke.capture("shell-zoom-before")
    click_zoom_out(smoke)
    smoke.until(
        "shell reduced canvas scale",
        lambda: smoke.control.status()["viewport"]["scale"] < 0.95,
    )
    checks.zoom_result(smoke, "shell-zoom", before_view, before, image)
    after = smoke.window(smoke.target)
    if (
        after.get("focused")
        or after["canvas_x"] != 100000
        or after["canvas_y"] != 100000
    ):
        raise ControlError("shell zoom stole focus or changed saved floating position")
    smoke.record("shell-focus-and-saved-position-preserved", window=after)
