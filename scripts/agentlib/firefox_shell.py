"""Shell focus and zoom preserve a distant maximized browser on the canvas."""

from . import firefox_assertions as verify
from . import firefox_checks as checks
from . import map_checks
from .control import ControlError
from .pinch_touchscreen import settled
from .viewport_fixture import set_viewport

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


def prepare_canvas_view(smoke):
    # Move slightly away from the maximized entry view. Closing the shell must
    # restore keyboard focus without recentering either its max or restore bounds.
    before = settled(smoke)
    expected = {"x": before["x"] + 80, "y": before["y"] + 40, "scale": 1}
    set_viewport(smoke, **expected)
    smoke.until(
        "maximized Firefox focused before shell click",
        lambda: smoke.window(smoke.target).get("focused"),
    )
    view = settled(smoke)
    if any(view[key] != value for key, value in expected.items()):
        raise ControlError("shell zoom setup failed to pan the maximized canvas view")
    return view


def close_map_preserving_view(smoke, expected):
    before_view = settled(smoke)
    before = smoke.window(smoke.target)
    if not before or before.get("focused") or not verify.maximized(before):
        raise ControlError(
            "open canvas map lost focus or changed saved Firefox position"
        )
    verify.unchanged_canvas_geometry(expected, before)
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
        or not verify.maximized(after)
        or any(after[key] != before[key] for key in ("canvas_x", "canvas_y"))
    ):
        raise ControlError(
            "closing canvas map lost Firefox focus or its saved position"
        )
    smoke.record("shell-map-close-restores-focus", window=after, viewport=after_view)
    verify.unchanged_canvas_geometry(before, after)


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
    # The real CSD maximizes near the distant window, then canvas pan shifts it
    # on screen while retaining its maximized size and original restore bounds.
    checks.prepare_maximized(smoke)
    before_view = prepare_canvas_view(smoke)
    before = smoke.window(smoke.target)
    smoke.record("shell-distant-maximized", window=before, viewport=before_view)
    image = smoke.capture("shell-zoom-before")
    click_zoom_out(smoke)
    smoke.until(
        "shell reduced canvas scale",
        lambda: smoke.control.status()["viewport"]["scale"] < 0.95,
    )
    close_map_preserving_view(smoke, before)
    checks.zoom_result(smoke, "shell-zoom", before_view, before, image)
