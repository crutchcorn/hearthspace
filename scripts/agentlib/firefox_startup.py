"""Give the owned Firefox fixture usable geometry before first-run controls."""

from . import firefox_assertions as verify
from .control import ControlError


def ordinary_firefox(window, target):
    return bool(
        verify.restored(window)
        and window.get("id") == target
        and "firefox" in (window.get("app_id") or "").lower()
        and window.get("titlebar_height") == 0
        and not window.get("output_coordinates")
    )


def unit_viewport(status):
    view = status["viewport"]
    return (
        abs(view["scale"] - 1) <= 1e-8
        and not view.get("animating")
        and not view.get("pinch_active")
    )


def target_window(status, target):
    return next(
        (window for window in status["windows"] if window["id"] == target), None
    )


def prepare(smoke):
    status = smoke.control.status()
    if len(status["outputs"]) != 1:
        raise ControlError("Firefox startup sizing requires one active output")
    if not unit_viewport(status):
        raise ControlError(
            "Firefox startup sizing requires a settled unit canvas scale"
        )
    output = status["outputs"][0]
    available = verify.workarea_bounds(output)
    width, height = min(1000, available["width"]), min(760, available["height"])
    # The first-run Terms of Use screen is 560px wide and about 572px tall.
    # Leave room for its content and for the later browser control checks.
    if width < 800 or height < 600:
        raise ControlError(
            "Firefox onboarding needs at least 800 by 600 usable logical pixels"
        )
    before = target_window(status, smoke.target)
    if not ordinary_firefox(before, smoke.target):
        raise ControlError(
            "Firefox startup sizing requires the owned floating CSD window"
        )

    # The traced Firefox startup maps at 500x200 despite its CLI dimensions.
    # Use the existing fixture resize command; CSD acceptance comes later.
    smoke.control.request(f"resize-window {smoke.target} {width} {height}")

    def committed():
        current = smoke.control.status()
        window = target_window(current, smoke.target)
        return (
            window
            if unit_viewport(current)
            and ordinary_firefox(window, smoke.target)
            and window["content_width"] == window["width"] == width
            and window["content_height"] == window["height"] == height
            else None
        )

    smoke.until("Firefox startup resize commits client dimensions", committed)
    # Enlarging at the original position can extend beyond the output. Focus
    # uses the existing reveal path, which brings the committed window into view.
    smoke.control.request(f"focus-window {smoke.target}")

    def visible():
        window = committed()
        return (
            window
            if window
            and window.get("focused")
            and output["logical_x"] <= window["x"]
            and output["logical_y"] <= window["y"]
            and window["x"] + width <= output["logical_x"] + output["logical_width"]
            and window["y"] + height <= output["logical_y"] + output["logical_height"]
            else None
        )

    smoke.until("Firefox startup window is fully visible", visible)
    smoke.capture("firefox-startup-sized")
    after = smoke.until("Firefox startup geometry after capture", visible)
    smoke.record(
        "firefox-startup-size-verified",
        source="owned fixture resize-window setup",
        before=before,
        after=after,
        requested_content_size={"width": width, "height": height},
    )
    return after
