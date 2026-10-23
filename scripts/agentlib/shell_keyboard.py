"""Reach each corner using the compositor shortcut and real widget navigation."""

from . import shell_checks as checks
from .control import ControlError
from .daily_checks import a11y, focused
from .input import Input
from .map_checks import SHELL
from .shell_surfaces import observed_status, output_layout


def focus_control(smoke, name, *, reverse=False):
    """Reach the owning native surface before checking its real Tab order."""
    status = observed_status(smoke)
    count = len(output_layout(status)["widgets"]) if "frame_layout" in status else 4
    target = f"{checks.button_selector(name)}/ancestor::*[@role='frame'][1]"
    focused_frame = f"{SHELL}//*[@role='frame' and .//*[@focused='true']]"

    def current():
        return a11y(smoke, "find", focused_frame)["elements"]

    def identities(elements):
        return {(item["bus_name"], item["object_path"]) for item in elements}

    for _ in range(count + 1):
        before = identities(current())
        owning = identities(a11y(smoke, "find", target)["elements"])
        if before & owning:
            checks.tab_to(smoke, name)
            return
        with Input(smoke.control) as inputs:
            inputs.chord("super+shift+f6" if reverse else "super+f6")
        smoke.until(
            "native frame focus advances",
            lambda previous=before: identities(current()) - previous,
        )
    raise ControlError(f"frame shortcut did not reach {name} in one traversal")


def run(smoke):
    checks.closed(smoke)
    smoke.until(
        "app focused before corner traversal", lambda: focused(smoke, smoke.target)
    )
    viewport = smoke.control.status()["viewport"]
    for name, kind in (
        ("Notifications", "notifications"),
        ("Sound settings", "controls"),
        ("Canvas map", "map"),
        ("Apps List", "search"),
    ):
        focus_control(smoke, name)
        smoke.capture(f"corner-keyboard-focus-{kind}")
        with Input(smoke.control) as inputs:
            inputs.chord("enter")
        checks.opened(smoke, kind, exclusive=True)
        with Input(smoke.control) as inputs:
            inputs.chord("esc")
        checks.closed(smoke)
        smoke.until("Escape restores the app", lambda: focused(smoke, smoke.target))
    focus_control(smoke, "Apps List", reverse=True)
    with Input(smoke.control) as inputs:
        inputs.chord("esc")
    smoke.until("Escape leaves a bare corner", lambda: focused(smoke, smoke.target))
    checks.closed(smoke)
    if smoke.control.status()["viewport"] != viewport:
        raise ControlError("corner keyboard navigation moved the canvas")
    smoke.record("corner-keyboard-navigation-verified", reverse=True, restored_app=True)
