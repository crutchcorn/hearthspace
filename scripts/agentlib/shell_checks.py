"""Observed shell palette geometry and private accessibility assertions."""

import ast
import json
import subprocess

from .control import ControlError
from .daily_checks import a11y
from .input import Input
from .map_checks import SHELL, button_center, selector

BAR_ICONS = (
    "Select window group",
    "Search",
    "Notifications",
    "Canvas map",
    "Control center",
    "Power",
)
REMOVED = ("Overview", "Reset view", "Windows", "Settings", "Lock", "Log out")


def elements(smoke, name, *, role="button"):
    return a11y(smoke, "find", selector(name, role=role))["elements"]


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
    outputs = smoke.control.status()["outputs"]
    if len(outputs) != 1:
        raise ControlError("shell acceptance needs one active output")
    surface = opened(smoke, kind) if kind else None
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
    buttons = smoke.until(
        "compact shell buttons",
        lambda: a11y(smoke, "find", f"{SHELL}//*[@role='button' and @showing='true']")[
            "elements"
        ],
    )
    names = [item["name"] for item in buttons]
    if (
        len(names) != len(BAR_ICONS)
        or any(name not in names for name in BAR_ICONS)
        or any(name in names for name in REMOVED)
    ):
        raise ControlError(f"unexpected compact shell controls: {names}")
    ordered = [
        item["name"] for item in sorted(buttons, key=lambda item: item["bounds"]["x"])
    ]
    first = ordered.index(BAR_ICONS[0])
    if tuple(ordered[first : first + len(BAR_ICONS)]) != BAR_ICONS:
        raise ControlError(f"shell icon ordering/adjacency changed: {ordered}")
    editable = a11y(smoke, "find", f"{SHELL}//*[@showing='true' and @editable='true']")
    if editable["elements"]:
        raise ControlError("search entry is still permanently visible in the bar")
    smoke.record("compact-shell-bar", buttons=buttons)


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
