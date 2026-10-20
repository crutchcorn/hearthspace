"""Observed shell palette geometry and private accessibility assertions."""

import ast
import json
import subprocess

from .control import ControlError
from .daily_checks import a11y
from .input import Input
from .map_checks import SHELL, button_center, corner_surface, selector

BAR_ICONS = (
    "Select window group",
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
    "Select window group": "canvas",
    "Canvas map": "canvas",
    "Apps List": "apps",
}


def elements(smoke, name, *, role="button"):
    query = selector(name, role=role)
    if role == "button":
        # AccessKit exports native buttons with toggled state as ToggleButton.
        # Launcher layout/mode controls remain ordinary pointer targets.
        query = (
            f"({SHELL}//*[(@role='button' or @role='toggle button') "
            f"and @showing='true' and @name='{name}'])[1]"
        )
    return a11y(smoke, "find", query)["elements"]


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


def click(smoke, name, kind=None):
    observed = smoke.until(f"visible {name} button", lambda: elements(smoke, name))
    status = smoke.control.status()
    outputs = status["outputs"]
    if len(outputs) != 1:
        raise ControlError("shell acceptance needs one active output")
    surface = (
        opened(smoke, kind) if kind else corner_surface(status, CONTROL_CORNERS[name])
    )
    point = button_center(observed[0].get("bounds"), outputs[0], surface)
    smoke.record("shell-button", name=name, kind=kind, point=point, elements=observed)
    with Input(smoke.control) as inputs:
        inputs.click(*point)


def tab_to(smoke, name):
    """Prove real Tab navigation can focus an option without activating it."""
    target = f"{selector(name, role='button')}[@focused='true']"
    with Input(smoke.control) as inputs:
        for count in range(1, 9):
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
        buttons = a11y(
            smoke, "find", f"{SHELL}//*[@role='button' and @showing='true']"
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
    status = smoke.control.status()
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
            button.get("bounds"), output, corner_surface(status, CONTROL_CORNERS[name])
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
