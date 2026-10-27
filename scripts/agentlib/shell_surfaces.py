"""Map native accessibility bounds through the actual independent frame surface."""

from .control import ControlError

WIDGET_CORNERS = {
    "notifications": "notifications",
    "clock": "system",
    "wifi": "system",
    "volume": "system",
    "brightness": "system",
    "battery": "system",
    "tray": "system",
    "power": "system",
    "tools": "canvas",
    "apps": "apps",
}
CONTROL_WIDGETS = {
    "Notifications": "notifications",
    "Power": "power",
    "Wi-Fi settings": "wifi",
    "Brightness settings": "brightness",
    "Sound settings": "volume",
    "Battery status": "battery",
    "Date and time": "clock",
    "Select desktop items, choose canvas tool": "tools",
    "Pan canvas, choose canvas tool": "tools",
    "Search canvas, choose canvas tool": "tools",
    "Select window group": "tools",
    "Change layout": "tools",
    "Canvas map": "tools",
    "Canvas tools": "tools",
    "Apps List": "apps",
}


def observed_status(smoke):
    status = smoke.control.status()
    # Legacy sessions and fixtures report corner surfaces. Their local coordinates
    # differ from independent frame members and must never be mixed.
    if not status.get("shell_corners"):
        status = status | {"frame_layout": smoke.control.query("layout-status")}
    return status


def output_layout(status):
    outputs = status["outputs"]
    if len(outputs) != 1:
        raise ControlError("shell corner acceptance requires one active output")
    layouts = [
        item
        for item in status.get("frame_layout", {}).get("outputs", [])
        if item.get("output") == outputs[0]["name"]
    ]
    if len(layouts) != 1:
        raise ControlError("expected one frame layout for the active output")
    return layouts[0]


def widget_surface(status, widget):
    if "frame_layout" not in status:
        return corner_surface(status, WIDGET_CORNERS[widget])
    layout = output_layout(status)
    matches = [item for item in layout["widgets"] if item["id"] == widget]
    if not matches:
        raise ControlError(f"{widget} frame widget is not mapped")
    if len(matches) != 1 or matches[0].get("hidden"):
        raise ControlError(f"expected one visible {widget} frame widget")
    rect = matches[0]["rect"]
    if rect["width"] <= 0 or rect["height"] <= 0:
        raise ControlError(f"{widget} frame widget is not mapped")
    return rect | {"output": layout["output"], "corner": WIDGET_CORNERS[widget]}


def corner_surface(status, corner):
    if "frame_layout" in status:
        if corner != "system":
            return widget_surface(
                status,
                {
                    "notifications": "notifications",
                    "canvas": "tools",
                    "apps": "apps",
                }[corner],
            )
        layout = output_layout(status)
        groups = [group for group in layout["groups"] if group["id"] == "system"]
        if len(groups) != 1:
            raise ControlError("expected one default system frame group")
        return groups[0]["rect"] | {"output": layout["output"], "corner": corner}
    outputs = status["outputs"]
    if len(outputs) != 1:
        raise ControlError("shell corner acceptance requires one active output")
    matches = [
        surface
        for surface in status.get("shell_corners", [])
        if surface["corner"] == corner and surface.get("output") == outputs[0]["name"]
    ]
    if len(matches) != 1:
        raise ControlError(f"expected one mapped {corner} shell corner")
    return matches[0]
