"""Real search navigation preserves editor focus and activates the selected row."""

from . import shell_checks as checks
from .control import ControlError
from .daily_checks import a11y, focused
from .input import Input
from .map_checks import SHELL
from .shell_presentation import (
    Frame,
    fitted,
    highlight_change,
    park_pointer,
    search_width,
)

RESULTS = (
    f"{SHELL}//*[@role='button' and @showing='true' and "
    "(starts-with(@name,'Window: ') or starts-with(@name,'Open: '))]"
)


def selected(smoke, name):
    def observed():
        matches = a11y(smoke, "find", RESULTS + "[@selected='true']")["elements"]
        return matches[0] if len(matches) == 1 and matches[0]["name"] == name else None

    return smoke.until(f"only selected search result is {name}", observed)


def window_label(window):
    return "Window: " + (
        window.get("title") or window.get("app_id") or "Untitled window"
    )


def run(smoke):
    status = smoke.control.status()
    windows = sorted(
        (
            item
            for item in status["windows"]
            if "foot" in item.get("app_id", "").lower()
        ),
        key=lambda item: (not item.get("focused"), item["id"]),
    )
    if len(windows) != 2 or windows[0]["id"] != smoke.target:
        raise ControlError("search arrow acceptance needs its two owned Foot windows")
    names = [window_label(window) for window in windows]
    if names[0] == names[1]:
        raise ControlError("search arrow fixture windows have ambiguous labels")
    ids = {item["id"] for item in status["windows"]}
    output = status["outputs"][0]
    with Input(smoke.control) as inputs:
        inputs.chord("super")
    checks.opened(smoke, "search")
    with Input(smoke.control) as inputs:
        inputs.type("foot")
    selected(smoke, names[0])
    rows = smoke.until(
        "two running-window result bounds",
        lambda: (
            matches[:2]
            if len(matches := a11y(smoke, "find", RESULTS)["elements"]) >= 2
            and [item["name"] for item in matches[:2]] == names
            else None
        ),
    )
    surface = checks.opened(smoke, "search")
    park_pointer(smoke, output)
    first = Frame(smoke.capture("search-arrow-first", output["name"]))
    captures = [first]
    changes = []
    for key, name in (("down", names[1]), ("up", names[0])):
        with Input(smoke.control) as inputs:
            inputs.chord(key)
        selected(smoke, name)
        if checks.query_text(smoke) != "foot":
            raise ControlError(
                "arrow selection lost editor focus or modified the query"
            )
        capture = Frame(smoke.capture(f"search-arrow-{key}", output["name"]))
        if checks.panel(smoke, "search") != surface:
            raise ControlError("search arrow navigation changed popup geometry")
        changes.append(highlight_change(captures[-1], capture, surface, rows))
        captures.append(capture)
    with Input(smoke.control) as inputs:
        inputs.chord("ctrl+a")
        inputs.type("HEARTHSPACE-NO-SUCH-INSTALLED-APP")
    smoke.until(
        "unmatched query shows the empty result state",
        lambda: checks.elements(smoke, "No matching apps or windows", role=None),
    )
    empty, gap = fitted(
        smoke,
        "search",
        "Enter to open",
        output,
        max_bottom_gap=32,
    )
    if a11y(smoke, "find", RESULTS)["elements"]:
        raise ControlError("an unmatched search still exposes application results")
    search_width(empty, output)
    smoke.capture("apps-unmatched")
    smoke.record(
        "search-empty-state-fits",
        populated=surface,
        empty=empty,
        content_bottom_gap=gap,
    )
    with Input(smoke.control) as inputs:
        inputs.chord("ctrl+a")
        inputs.chord("backspace")
    smoke.until(
        "clearing the query restores installed apps",
        lambda: a11y(smoke, "find", RESULTS + "[starts-with(@name,'Open: ')]")[
            "elements"
        ],
    )
    if checks.query_text(smoke) != "":
        raise ControlError("clearing Apps List did not retain its empty focused editor")
    smoke.capture("apps-cleared-browsing")
    with Input(smoke.control) as inputs:
        inputs.type("foot")
    selected(smoke, names[0])
    with Input(smoke.control) as inputs:
        inputs.chord("down")
    selected(smoke, names[1])
    with Input(smoke.control) as inputs:
        inputs.chord("enter")
    checks.closed(smoke)
    smoke.until(
        "Enter focuses the second selected window",
        lambda: focused(smoke, windows[1]["id"]),
    )
    if {item["id"] for item in smoke.control.status()["windows"]} != ids:
        raise ControlError(
            "search Enter spawned or removed a window instead of selecting it"
        )
    smoke.capture("search-arrow-activated")
    smoke.record(
        "search-arrow-selection",
        selected_id=windows[1]["id"],
        row_pixel_change_fractions=changes,
        query="foot",
        window_ids=sorted(ids),
    )
    smoke.control.request(f"focus-window {smoke.target}")
    smoke.until("original fixture focus restored", lambda: focused(smoke, smoke.target))
