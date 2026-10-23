"""Reach each corner using the compositor shortcut and real widget navigation."""

from . import shell_checks as checks
from .control import ControlError
from .daily_checks import focused
from .input import Input


def run(smoke):
    checks.closed(smoke)
    smoke.until(
        "app focused before corner traversal", lambda: focused(smoke, smoke.target)
    )
    viewport = smoke.control.status()["viewport"]
    for count, name, kind in (
        (1, "Notifications", "notifications"),
        (2, "Sound settings", "controls"),
        (3, "Canvas map", "map"),
        (4, "Apps List", "search"),
    ):
        with Input(smoke.control) as inputs:
            for _ in range(count):
                inputs.chord("super+f6")
        checks.tab_to(smoke, name)
        smoke.capture(f"corner-keyboard-focus-{kind}")
        with Input(smoke.control) as inputs:
            inputs.chord("enter")
        checks.opened(smoke, kind, exclusive=True)
        with Input(smoke.control) as inputs:
            inputs.chord("esc")
        checks.closed(smoke)
        smoke.until("Escape restores the app", lambda: focused(smoke, smoke.target))
    with Input(smoke.control) as inputs:
        inputs.chord("super+shift+f6")
    checks.tab_to(smoke, "Apps List")
    with Input(smoke.control) as inputs:
        inputs.chord("esc")
    smoke.until("Escape leaves a bare corner", lambda: focused(smoke, smoke.target))
    checks.closed(smoke)
    if smoke.control.status()["viewport"] != viewport:
        raise ControlError("corner keyboard navigation moved the canvas")
    smoke.record("corner-keyboard-navigation-verified", reverse=True, restored_app=True)
