"""Observed shell palette geometry and private accessibility assertions."""

import ast
import json
import subprocess

from .control import ControlError
from .daily_checks import a11y
from .input import Input
from .map_checks import SHELL, button_center, corner_surface, selector
from .shell_surfaces import (
    CONTROL_WIDGETS,
    observed_status,
    widget_surface,
)

BAR_ICONS = (
    "Select desktop items",
    "Search canvas",
    "Select window group",
    "Change layout",
    "Apps List",
    "Notifications",
    "Canvas map",
    "Brightness settings",
    "Sound settings",
    "Power",
)
REMOVED = (
    "Overview",
    "Reset view",
    "Windows",
    "Settings",
    "Lock",
    "Log out",
    "Control center",
)
CONTROL_CORNERS = {
    "Notifications": "notifications",
    "Power": "system",
    "Wi-Fi settings": "system",
    "Brightness settings": "system",
    "Sound settings": "system",
    "Battery status": "system",
    "Date and time": "system",
    "Select desktop items": "canvas",
    "Search canvas": "canvas",
    "Select window group": "canvas",
    "Change layout": "canvas",
    "Canvas map": "canvas",
    "Canvas tools": "canvas",
    "Apps List": "apps",
}


def button_selector(name, role="button"):
    query = selector(name, role=role)
    if role == "button":
        # AccessKit exports native buttons with toggled state as ToggleButton.
        # Launcher layout/mode controls remain ordinary pointer targets.
        query = (
            f"({SHELL}//*[(@role='button' or @role='toggle button') "
            f"and @showing='true' and @name='{name}'])[1]"
        )
    return query


def elements(smoke, name, *, role="button"):
    return a11y(smoke, "find", button_selector(name, role))["elements"]


def panel(smoke, kind, *, exclusive=False):
    panels = smoke.control.status().get("shell_panels", [])
    matches = [item for item in panels if item["kind"] == kind]
    if len(matches) > 1:
        raise ControlError("shell acceptance needs one output and one matching panel")
    if exclusive and len(panels) != 1:
        return None
    return matches[0] if matches else None


def opened(smoke, kind, *, exclusive=False):
    state = "mapped without other shell panels" if exclusive else "mapped"
    return smoke.until(
        f"{kind} panel {state}", lambda: panel(smoke, kind, exclusive=exclusive)
    )


def closed(smoke):
    smoke.until(
        "all shell panels dismissed",
        lambda: not smoke.control.status().get("shell_panels"),
    )


def click(smoke, name, kind=None, *, corner=None):
    target = button_selector(name)
    # One snapshot supplies both the button and its owning window. A stale
    # button alone can point at another dock item after the corner resizes.
    query = f"{target} | {target}/ancestor::*[@role='frame'][1]"

    def geometry():
        status = observed_status(smoke)
        if len(status["outputs"]) != 1:
            raise ControlError("shell acceptance needs one active output")
        if kind:
            matches = [
                item for item in status.get("shell_panels", []) if item["kind"] == kind
            ]
            if len(matches) > 1:
                raise ControlError("shell acceptance needs one matching panel")
            surface = matches[0] if matches else None
        else:
            corner_name = corner or CONTROL_CORNERS[name]
            # shell_ready can precede the final corner's initial map. Only an
            # absent corner is pending; duplicates/wrong outputs remain errors.
            if "frame_layout" in status:
                widget = CONTROL_WIDGETS.get(
                    name, "battery" if name.startswith("Battery:") else None
                )
                if widget is None:
                    raise ControlError(f"no frame widget owns shell control {name}")
                try:
                    surface = widget_surface(status, widget)
                except ControlError as error:
                    if "not mapped" not in str(error):
                        raise
                    surface = None
            else:
                mapped = any(
                    item["corner"] == corner_name
                    for item in status.get("shell_corners", [])
                )
                surface = corner_surface(status, corner_name) if mapped else None
        output = status["outputs"][0]
        output_geometry = {
            key: output[key]
            for key in (
                "name",
                "logical_x",
                "logical_y",
                "logical_width",
                "logical_height",
                "scale",
            )
        }
        return output_geometry, surface

    def ready():
        before = geometry()
        if before[1] is None:
            return None
        observed = a11y(smoke, "find", query)["elements"]
        output, surface = geometry()
        if not surface or before != (output, surface):
            return None
        frames = [item for item in observed if item.get("role") == "Frame"]
        buttons = [
            item for item in observed if item.get("role") in ("Button", "ToggleButton")
        ]
        if len(frames) != 1 or len(buttons) != 1:
            return None
        bounds = frames[0].get("bounds") or {}
        if any(
            bounds.get(dimension) != surface[dimension] * output["scale"]
            for dimension in ("width", "height")
        ):
            return None
        return button_center(buttons[0].get("bounds"), output, surface), buttons

    point, observed = smoke.until(f"settled {name} button and shell surface", ready)
    smoke.record("shell-button", name=name, kind=kind, point=point, elements=observed)
    with Input(smoke.control) as inputs:
        inputs.click(*point)


def tab_to(smoke, name):
    """Prove real Tab navigation can focus an option without activating it."""
    target = f"{button_selector(name)}[@focused='true']"
    with Input(smoke.control) as inputs:
        for count in range(9):
            if count:
                inputs.chord("tab")
            matches = a11y(smoke, "find", target)["elements"]
            if matches:
                smoke.record(
                    "shell-keyboard-focus", name=name, tabs=count, elements=matches
                )
                return
    raise ControlError(f"Tab did not focus shell {name} within eight presses")


def bar(smoke):
    closed(smoke)

    def complete_buttons():
        # Desktop tiles also expose ToggleButton; only corner controls belong
        # in this check. Active canvas tools must remain visible to validation.
        corner_panels = (
            "@name='Notifications' or @name='System tray' or "
            "@name='Canvas tools' or @name='Running apps and Apps List'"
        )
        names = " or ".join(f"@name='{name}'" for name in BAR_ICONS)
        # Native Frame nodes need not expose their Winit titles. Identify the
        # owner through its controls, then include every button in that owner
        # so duplicate and obsolete actions remain visible to the assertions.
        owners = (
            f"{SHELL}//*[(@role='button' or @role='toggle button') "
            f"and @showing='true' and ({names})]/ancestor::*[@role='frame'][1]"
        )
        legacy = f"{SHELL}//*[@role='panel' and ({corner_panels})]"
        buttons = a11y(
            smoke,
            "find",
            f"({owners} | {legacy})"
            "//*[(@role='button' or @role='toggle button') and @showing='true']",
        )["elements"]
        # Each corner registers a separate accessibility window. The first
        # visible controls can arrive before the final corner has registered.
        names = {item["name"] for item in buttons}
        return buttons if all(name in names for name in BAR_ICONS) else None

    buttons = smoke.until("four-corner shell buttons", complete_buttons)
    names = [item["name"] for item in buttons]
    if any(names.count(name) != 1 for name in BAR_ICONS) or any(
        name in names for name in REMOVED
    ):
        raise ControlError(f"unexpected corner shell controls: {names}")
    status = observed_status(smoke)
    output = status["outputs"][0]
    corners = [
        corner_surface(status, name)
        for name in ("notifications", "system", "canvas", "apps")
    ]
    for surface in corners:
        x = surface["x"] - output["logical_x"]
        y = surface["y"] - output["logical_y"]
        right, bottom = x + surface["width"], y + surface["height"]
        if not (
            0 <= x < right <= output["logical_width"]
            and 0 <= y < bottom <= output["logical_height"]
        ):
            raise ControlError(f"shell corner lies outside output: {surface}")
        is_right = x + surface["width"] / 2 > output["logical_width"] / 2
        is_bottom = y + surface["height"] / 2 > output["logical_height"] / 2
        if is_right != (surface["corner"] in ("system", "apps")) or is_bottom != (
            surface["corner"] in ("canvas", "apps")
        ):
            raise ControlError(
                f"shell corner has the wrong purpose/position: {surface}"
            )
        expected_height = min(54, max(1, output["logical_height"] - 48))
        max_x = max(0, output["logical_width"] - surface["width"])
        max_y = max(0, output["logical_height"] - surface["height"])
        expected_x = max(0, max_x - 24) if is_right else min(24, max_x)
        expected_y = max(0, max_y - 24) if is_bottom else min(24, max_y)
        if (x, y, surface["height"]) != (expected_x, expected_y, expected_height):
            raise ControlError(
                f"shell corner does not use 54px height/24px inset: {surface}"
            )
    for name in BAR_ICONS:
        button = next(item for item in buttons if item["name"] == name)
        button_center(
            button.get("bounds"), output, widget_surface(status, CONTROL_WIDGETS[name])
        )
    editable = a11y(smoke, "find", f"{SHELL}//*[@showing='true' and @editable='true']")
    if editable["elements"]:
        raise ControlError("search entry is still permanently visible in the bar")
    smoke.record("four-corner-shell", buttons=buttons, corners=corners)


def dbus_value(smoke, address, destination, path, method, *arguments):
    result = subprocess.run(
        [
            "gdbus",
            "call",
            "--address",
            address,
            "--dest",
            destination,
            "--object-path",
            path,
            "--method",
            method,
            "--",
            *arguments,
        ],
        capture_output=True,
        text=True,
        check=True,
        timeout=smoke.control.deadline.remaining(),
    )
    value = ast.literal_eval(result.stdout)
    if not isinstance(value, tuple) or len(value) != 1 or not isinstance(value[0], str):
        raise ControlError("unexpected private accessibility text response")
    return value[0]


def query_text(smoke):
    target = f"{SHELL}//*[@focused='true' and @editable='true']"
    matches = a11y(smoke, "find", target)["elements"]
    if len(matches) != 1:
        return None
    manifest = json.loads((smoke.directory / "session.json").read_text())
    address = dbus_value(
        smoke,
        manifest["dbus_session_bus_address"],
        "org.a11y.Bus",
        "/org/a11y/bus",
        "org.a11y.Bus.GetAddress",
    )
    node = matches[0]
    return dbus_value(
        smoke,
        address,
        node["bus_name"],
        node["object_path"],
        "org.a11y.atspi.Text.GetText",
        "0",
        "-1",
    )
