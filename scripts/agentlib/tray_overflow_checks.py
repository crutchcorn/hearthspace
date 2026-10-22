"""Observed overflow geometry, painted icons, and absence of leaked input."""

import math

from . import shell_checks, tray_checks
from .control import ControlError
from .daily_checks import a11y
from .map_checks import SHELL
from .pinch_assertions import same_viewport
from .shell_presentation import Frame, park_pointer
from .shell_surfaces import observed_status, output_layout, widget_surface
from .tray_overflow_fixture import PREFIX, title


def item_count(output):
    # Current tray pitch is 38 logical pixels. Respect the watcher's 64-item
    # bound and fail explicitly when this output cannot overflow at that limit.
    extent = max(output["logical_width"], output["logical_height"])
    count = min(64, max(24, math.ceil(extent / 38) + 8))
    if 38 * count - 4 <= extent:
        raise ControlError(
            "output is too large to overflow within the 64-item watcher limit"
        )
    return count


def bounded(surface, output):
    values = [surface.get(key, float("nan")) for key in ("x", "y", "width", "height")]
    if not all(math.isfinite(value) for value in values):
        raise ControlError("standalone tray has nonfinite bounds")
    x, y, width, height = values
    if not (
        width > 0
        and height > 0
        and x >= output["logical_x"]
        and y >= output["logical_y"]
        and x + width <= output["logical_x"] + output["logical_width"] + 1
        and y + height <= output["logical_y"] + output["logical_height"] + 1
    ):
        raise ControlError("standalone tray extends beyond the actual output")


def fully_visible(element, surface, scale):
    bounds = element.get("bounds") or {}
    values = [bounds.get(key, float("nan")) for key in ("x", "y", "width", "height")]
    if not all(math.isfinite(value) for value in values) or scale <= 0:
        return False
    x, y, width, height = [value / scale for value in values]
    # The detached surface has 5 logical pixels of chrome around its portal.
    # Outer-surface containment alone would accept partially clipped icons.
    return (
        width > 0
        and height > 0
        and x >= 4.5
        and y >= 4.5
        and x + width <= surface["width"] - 4.5
        and y + height <= surface["height"] - 4.5
    )


def elements(smoke, *, focused=False):
    suffix = " and @focused='true'" if focused else ""
    return a11y(
        smoke,
        "find",
        f"{SHELL}//*[@role='button' and starts-with(@name,'{PREFIX}'){suffix}]",
    )["elements"]


def observe(smoke, edge):
    status = observed_status(smoke)
    layout = output_layout(status)
    widget = next(item for item in layout["widgets"] if item["id"] == "tray")
    placement = layout["layout"]["placements"].get("widget:tray", {})
    if (
        widget.get("group")
        or widget.get("merged")
        or placement.get("surface") != "frame"
        or placement.get("edge") != edge
    ):
        raise ControlError(
            "overflow acceptance requires a standalone tray on its requested edge"
        )
    output = status["outputs"][0]
    surface = widget_surface(status, "tray")
    bounded(surface, output)
    expected = "vertical" if edge == "left" else "horizontal"
    if widget["orientation"] != expected:
        raise ControlError("standalone tray has the wrong orientation")
    icons = elements(smoke)
    return {
        "surface": surface,
        "output": output,
        "icons": icons,
        "visible": [
            item for item in icons if fully_visible(item, surface, output["scale"])
        ],
        "viewport": status["viewport"],
    }


def revealed(smoke, edge, name, *, focused=False):
    observed = observe(smoke, edge)
    matches = [item for item in observed["visible"] if item["name"] == name]
    if len(matches) != 1:
        return None
    if focused and not any(
        item["name"] == name for item in elements(smoke, focused=True)
    ):
        return None
    return observed


def unchanged(smoke, peer, before, baseline):
    if peer.process.poll() is not None:
        raise ControlError("overflow app peer died during interaction")
    leaked = peer.events()[baseline:]
    if leaked:
        raise ControlError(f"tray navigation leaked app actions: {leaked}")
    after = smoke.control.status()["viewport"]
    if not same_viewport(before, after) or after.get("pinch_active"):
        raise ControlError("tray overflow input moved or zoomed the canvas")
    shell_checks.closed(smoke)


def evidence(smoke, edge, label, observed, names):
    park_pointer(smoke, observed["output"])
    capture_name = f"tray-overflow-{edge}-{label}"
    # Keep the actual frame whose pixels are asserted, with native metadata.
    frame = Frame(smoke.capture(capture_name, observed["output"]["name"]))
    counts = {}
    for name in names:
        icon = next(item for item in observed["visible"] if item["name"] == name)
        count = tray_checks.color_pixels(
            frame, icon["bounds"], "red", observed["surface"]
        )
        if count <= 20:
            raise ControlError(f"revealed tray icon {name} lacks its fixture pixels")
        counts[name] = count
    snapshot = a11y(smoke, "snapshot")["snapshot"]
    (smoke.artifacts / f"{capture_name}.xml").write_text(snapshot)
    smoke.record(
        "tray-overflow-evidence",
        edge=edge,
        action=label,
        capture=capture_name,
        matching_pixels=counts,
        **observed,
    )


def require_overflow(observed, count):
    names = [item["name"] for item in observed["icons"]]
    if sorted(names) != [title(index) for index in range(count)]:
        return False
    visible = {item["name"] for item in observed["visible"]}
    return (
        title(0) in visible and title(count - 1) not in visible and len(visible) < count
    )
