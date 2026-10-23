"""Real editable app-combobox assertions shared by gesture acceptance flows."""

from .control import ControlError
from .daily_checks import a11y
from .input import Input


def selected(smoke):
    from .favorite_ui import SETTINGS

    query = (
        f"({SETTINGS}//*[@role='list item' and @selected='true' "
        "and @showing='true'])[1]"
    )
    rows = a11y(smoke, "find", query)["elements"]
    return rows[0]["name"] if rows else None


def opened(smoke):
    from .favorite_ui import settings_elements

    return settings_elements(smoke, "Installed apps", "list box")


def dismiss(smoke):
    with Input(smoke.control) as inputs:
        inputs.chord("escape")
    smoke.until("app picker closed", lambda: not opened(smoke))


def choose(smoke, name):
    from .favorite_ui import settings_click, settings_elements

    pad = smoke.until(
        "gesture drawing pad", lambda: settings_elements(smoke, "Gesture drawing area")
    )[0]["bounds"]
    settings_click(smoke, "Open this app", "combo box")
    with Input(smoke.control) as inputs:
        inputs.chord("ctrl+a")
        inputs.type(name)
    smoke.until(
        "filtered installed app",
        lambda: settings_elements(smoke, name, "list item"),
    )
    current = settings_elements(smoke, "Gesture drawing area")[0]["bounds"]
    if current != pad:
        raise ControlError("opening the app picker moved the drawing area")
    smoke.capture("favorite-app-picker-filtered")
    with Input(smoke.control) as inputs:
        inputs.chord("enter")
    smoke.until("keyboard app choice closed picker", lambda: not opened(smoke))
    settings_click(smoke, "Choose app")
    smoke.until("selected gesture app", lambda: selected(smoke) == name)
    dismiss(smoke)
    smoke.record(
        "gesture-app-picker",
        app=name,
        editable=True,
        keyboard_choice=True,
        overlay=True,
    )
