"""Native CSD state evidence, including committed client geometry and signals."""

from .client_accessibility import presentation_scale
from .control import ControlError
from .firefox_assertions import matches_output_bounds, matches_workarea
from .window_preview_assertions import marker_count


def state(window, client, output, viewport, mode, original, fullscreen_entries):
    focused = mode in ("focused", "fullscreen")
    maximized = mode != "floating"
    fullscreen = mode == "fullscreen"
    expected = {
        "mapped": True,
        "visible": True,
        "focused": True,
        "focused_view": focused,
        "maximized": maximized,
        "fullscreen": fullscreen,
        "output_coordinates": focused,
    }
    if not window or any(window.get(key) != value for key, value in expected.items()):
        raise ControlError(f"CSD {mode} has incorrect compositor flags")
    if client.get("fullscreen") != fullscreen or client.get("maximized") != maximized:
        raise ControlError(f"CSD {mode} has incorrect application protocol state")
    # This counter is updated synchronously by both GTK fixtures' state signals,
    # so a transient fullscreen configure between sampled frames also fails.
    if "fullscreen_entries" in client:
        if client["fullscreen_entries"] != fullscreen_entries:
            raise ControlError("unexpected application fullscreen signal")
        if not fullscreen and not client["header_mapped"]:
            raise ControlError("focused/canvas mode lost the application header")
    if focused:
        if window.get("titlebar_height") != 0:
            raise ControlError("focused CSD retains a compositor titlebar")
        if window["presentation_output"] != output["name"] or not matches_output_bounds(
            window, [output]
        ):
            raise ControlError("CSD focused view does not fill the tested output")
    elif window["presentation_output"] is not None:
        raise ControlError("canvas window remains pinned to an output")
    if mode == "canvas" and (
        not window["maximized_canvas_bounds"] or not matches_workarea(window, output)
    ):
        raise ControlError("CSD titlebar did not produce bounded canvas maximize")
    scale = presentation_scale(window, viewport)
    if (
        abs(window["content_width"] * scale - window["width"]) > 1.5
        or abs(
            window["content_height"] * scale
            + window["titlebar_height"]
            - window["height"]
        )
        > 1.5
    ):
        raise ControlError("CSD client has not committed the presented size")
    if mode == "floating" and original is not None:
        fields = ("canvas_x", "canvas_y", "content_width", "content_height")
        if any(abs(window[field] - original[field]) > 1 for field in fields):
            raise ControlError("CSD restore lost the original floating geometry")
        if window["maximized_canvas_bounds"] is not None:
            raise ControlError("CSD restore retained maximized canvas bounds")


def rendered(image, window, color):
    bounds = (
        window["x"],
        window["y"],
        window["x"] + window["width"],
        window["y"] + window["height"],
    )
    count = marker_count(image, bounds, color)
    if count < 100:
        raise ControlError(
            "CSD client marker is absent or stale after application input"
        )
    return count
