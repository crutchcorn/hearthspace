"""Click the real shell while a maximized browser has distant saved geometry."""

from . import firefox_assertions as verify
from . import firefox_checks as checks
from . import map_checks
from .control import ControlError
from .pinch_touchscreen import settled

button_center = map_checks.button_center


def click_zoom_out(smoke):
    def focus_cleared():
        smoke.until(
            "shell pointer press clears Firefox focus",
            lambda: not smoke.window(smoke.target).get("focused"),
        )
        smoke.record("shell-pointer-focused", window=smoke.window(smoke.target))

    map_checks.click(smoke, "Canvas map", on_press=focus_cleared)
    map_checks.opened(smoke)
    map_checks.click(smoke, "Zoom out", on_press=focus_cleared)


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


def close_map_preserving_view(smoke):
    before_view = settled(smoke)
    before = smoke.window(smoke.target)
    if (
        not before
        or before.get("focused")
        or before["canvas_x"] != 100000
        or before["canvas_y"] != 100000
    ):
        raise ControlError(
            "open canvas map lost focus or changed saved Firefox position"
        )
    smoke.record("shell-map-holds-focus", window=before, viewport=before_view)
    map_checks.close(smoke)
    smoke.until(
        "closing canvas map restores Firefox keyboard focus",
        lambda: (window := smoke.window(smoke.target)) and window.get("focused"),
    )
    after_view = settled(smoke)
    after = smoke.window(smoke.target)
    if any(after_view[key] != before_view[key] for key in ("x", "y", "scale")):
        raise ControlError("closing canvas map panned or zoomed to Firefox")
    if (
        not after
        or not after.get("focused")
        or any(after[key] != before[key] for key in ("canvas_x", "canvas_y"))
    ):
        raise ControlError(
            "closing canvas map lost Firefox focus or its saved position"
        )
    smoke.record("shell-map-close-restores-focus", window=after, viewport=after_view)


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
    close_map_preserving_view(smoke)
    checks.zoom_result(smoke, "shell-zoom", before_view, before, image)
