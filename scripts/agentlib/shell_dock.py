"""Cycle the two owned terminal windows through their running-app dock entry."""

from . import shell_checks as checks
from .control import ControlError
from .daily_checks import a11y, focused
from .input import Input
from .map_checks import SHELL, button_center, corner_surface
from .shell_surfaces import observed_status


def next_button(smoke, title):
    def observed():
        buttons = a11y(
            smoke, "find", f"{SHELL}//*[@role='button' and @showing='true']"
        )["elements"]
        matches = [
            item
            for item in buttons
            if ": 2 windows, active. Next: " in item["name"]
            and item["name"].endswith(f". Next: {title}")
        ]
        return matches[0] if len(matches) == 1 else None

    return smoke.until(f"active running-app dock entry cycles to {title}", observed)


def run(smoke):
    checks.closed(smoke)
    status = smoke.control.status()
    windows = [
        item for item in status["windows"] if "foot" in item.get("app_id", "").lower()
    ]
    if len(windows) != 2 or not any(
        window["id"] == smoke.target and window.get("focused") for window in windows
    ):
        raise ControlError("dock cycling acceptance needs two owned Foot windows")
    original = next(window for window in windows if window["id"] == smoke.target)
    other = next(window for window in windows if window["id"] != smoke.target)
    ids = {item["id"] for item in status["windows"]}
    if original["title"] == other["title"]:
        raise ControlError("dock fixture windows have ambiguous titles")
    for window in (other, original):
        button = next_button(smoke, window["title"])
        current = observed_status(smoke)
        surface = corner_surface(current, "apps")
        point = button_center(button["bounds"], current["outputs"][0], surface)
        with Input(smoke.control) as inputs:
            inputs.click(*point)
        smoke.until(
            "dock activates the next existing app window",
            lambda window_id=window["id"]: focused(smoke, window_id),
        )
        if {item["id"] for item in smoke.control.status()["windows"]} != ids:
            raise ControlError("dock switching spawned or removed an app window")
        checks.closed(smoke)
        smoke.capture(f"dock-focused-{window['id']}")
        smoke.record("dock-window-activated", window_id=window["id"], button=button)
    smoke.record("running-app-dock-verified", window_ids=sorted(ids), cycled=True)
