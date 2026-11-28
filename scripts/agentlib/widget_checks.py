"""Real canvas picker, duplicate instances, frame dragging, and launcher input."""

import json

from .control import ControlError
from .daily_checks import a11y
from .input import Input


def layout(smoke):
    return smoke.control.query("layout-status")


def copies(smoke, kind):
    return [
        widget
        for output in layout(smoke)["outputs"]
        for widget in output["widgets"]
        if widget["kind"] == kind
    ]


def mapped(smoke, identity):
    return next(
        (
            widget
            for output in layout(smoke)["outputs"]
            for widget in output["widgets"]
            if widget["id"] == identity and widget.get("mapped")
        ),
        None,
    )


def activate(smoke, name, role="button"):
    selector = f"//*[@role='{role}' and @name='{name}']"
    smoke.until(name, lambda: a11y(smoke, "find", selector)["elements"])
    a11y(smoke, "activate", selector)


def add(smoke, kind, query, title, *, point=None):
    before = {widget["id"] for widget in copies(smoke, kind)}
    if point:
        with Input(smoke.control) as inputs:
            inputs.click(*point, button="right")
        activate(smoke, "Add Widget…", "menu item")
        smoke.until(
            "widget-only search",
            lambda: (
                (state := smoke.control.query("canvas-search-status"))["active"]
                and state["filter"] == "widgets"
                and state["query"] == ""
                and all(result["kind"] == "widget" for result in state["results"])
            ),
        )
        editor = "Search widgets to add to the canvas"
    else:
        with Input(smoke.control) as inputs:
            inputs.chord("meta+k")
        editor = "Search apps, windows, files, folders, widgets, controls, and tools"
    smoke.until(
        "search editor focused",
        lambda: a11y(smoke, "find", f"//*[@name='{editor}' and @focused='true']")[
            "elements"
        ],
    )
    smoke.capture(f"search-{kind}-{len(before)}")
    with Input(smoke.control) as inputs:
        inputs.type(query)
    smoke.until(
        "widget query confirmed",
        lambda: smoke.control.query("canvas-search-status")["query"] == query,
    )
    activate(smoke, f"Add {title} Add widget to canvas")
    smoke.until(
        "picker closed",
        lambda: not smoke.control.query("canvas-search-status")["active"],
    )
    added = smoke.until(
        "new widget copy mapped",
        lambda: next(
            (
                widget
                for widget in copies(smoke, kind)
                if widget["id"] not in before and widget.get("mapped")
            ),
            None,
        ),
    )
    smoke.capture(f"added-{kind}-{added['id'].replace(':', '-')}")
    return added["id"]


def center(widget):
    rect = widget["rect"]
    return rect["x"] + rect["width"] / 2, rect["y"] + rect["height"] / 2


def run(smoke, *, quit_on_finish=True):
    smoke.until(
        "starter frame mapped",
        lambda: any(widget.get("mapped") for widget in copies(smoke, "apps")),
    )
    if smoke.control.query("desktop-status")["items"]:
        raise ControlError("installed apps created icons on the empty Desktop folder")
    before = copies(smoke, "clock")[0]
    launcher = add(smoke, "launcher", "launcher", "App launcher", point=(160, 220))
    with Input(smoke.control) as inputs:
        inputs.click(*center(mapped(smoke, launcher)))
    smoke.until(
        "canvas launcher opened Apps List",
        lambda: a11y(smoke, "find", "//*[@name='Search apps and windows...']")[
            "elements"
        ],
    )
    with Input(smoke.control) as inputs:
        inputs.chord("escape")
    smoke.until(
        "launcher dismissed",
        lambda: (
            not any(
                panel["kind"] == "search"
                for panel in smoke.control.status()["shell_panels"]
            )
        ),
    )

    with Input(smoke.control) as inputs:
        inputs.click(*center(mapped(smoke, launcher)), button="right")
    activate(smoke, "Change Layout…", "menu item")
    smoke.until(
        "widget selected for layout",
        lambda: (
            (selected := layout(smoke)["selected"])
            and selected["target"]["id"] == launcher
        ),
    )
    view = layout(smoke)["outputs"][0]["rect"]
    destination = view["x"] + 24, view["y"] + view["height"] / 2
    with Input(smoke.control) as inputs:
        inputs.drag(*center(mapped(smoke, launcher)), *destination)
    smoke.until(
        "same widget attached to frame",
        lambda: (
            any(
                output["layout"]["placements"]
                .get(f"widget:{launcher}", {})
                .get("surface")
                == "frame"
                for output in layout(smoke)["outputs"]
            )
            and not layout(smoke)["animating"]
        ),
    )
    with Input(smoke.control) as inputs:
        inputs.chord("escape")
    smoke.until("layout editor closed", lambda: not layout(smoke)["editing"])
    with Input(smoke.control) as inputs:
        inputs.click(*center(mapped(smoke, launcher)))
    smoke.until(
        "frame launcher opened Apps List",
        lambda: a11y(
            smoke, "find", "//*[@name='Search apps and windows...' and @focused='true']"
        )["elements"],
    )
    with Input(smoke.control) as inputs:
        inputs.type("Widget Terminal")
    activate(smoke, "Open: Widget Terminal")
    window = smoke.until(
        "launched terminal focused",
        lambda: next(
            (
                window
                for window in smoke.control.status()["windows"]
                if window["title"] == "WIDGET_TERMINAL" and window["focused"]
            ),
            None,
        ),
    )
    smoke.capture("widget-launcher-terminal")
    smoke.control.request(f"close-window {window['id']}")
    smoke.until("owned terminal closed", lambda: smoke.window(window["id"]) is None)

    with Input(smoke.control) as inputs:
        inputs.chord("meta+a")
        inputs.click(*center(mapped(smoke, launcher)))
    smoke.until(
        "launcher selected again",
        lambda: (
            (selected := layout(smoke)["selected"])
            and selected["target"]["id"] == launcher
        ),
    )
    activate(smoke, "Canvas", "toggle button")
    with Input(smoke.control) as inputs:
        inputs.chord("escape")
    smoke.until(
        "canvas placement settled",
        lambda: not layout(smoke)["editing"] and not layout(smoke)["animating"],
    )
    with Input(smoke.control) as inputs:
        inputs.click(*center(mapped(smoke, launcher)), button="right")
    activate(smoke, "Remove Widget", "menu item")
    smoke.until(
        "only selected widget retired",
        lambda: (
            not any(widget["id"] == launcher for widget in copies(smoke, "launcher"))
        ),
    )
    replacement = add(smoke, "launcher", "launcher", "App launcher")
    if replacement == launcher:
        raise ControlError("re-adding reused a retired widget identity")
    clock = add(smoke, "clock", "clock", "Clock", point=(450, 430))
    after = next(
        widget for widget in copies(smoke, "clock") if widget["id"] == before["id"]
    )
    if after["group"] != before["group"] or len(copies(smoke, "clock")) != 2:
        raise ControlError("adding another clock moved or replaced the starter clock")
    smoke.record(
        "widgets-verified",
        retired=launcher,
        replacement=replacement,
        clock=clock,
        canvas_launcher=True,
        frame_launcher=True,
        frame_drag=True,
    )
    keyboard_add(smoke)
    if quit_on_finish:
        smoke.control.request("quit")


def keyboard_add(smoke):
    before = {widget["id"] for widget in copies(smoke, "brightness")}
    with Input(smoke.control) as inputs:
        inputs.chord("meta+k")
    editor = "Search apps, windows, files, folders, widgets, controls, and tools"
    smoke.until(
        "keyboard search focused",
        lambda: a11y(smoke, "find", f"//*[@name='{editor}' and @focused='true']")[
            "elements"
        ],
    )
    with Input(smoke.control) as inputs:
        inputs.type("add brightness")
    smoke.until(
        "keyboard widget result selected",
        lambda: (
            (state := smoke.control.query("canvas-search-status"))["query"]
            == "add brightness"
            and state["selected_id"] == "widget:brightness"
        ),
    )
    smoke.capture("keyboard-widget-result")
    with Input(smoke.control) as inputs:
        inputs.chord("enter")
    added = smoke.until(
        "keyboard-added widget mapped",
        lambda: next(
            (
                widget
                for widget in copies(smoke, "brightness")
                if widget["id"] not in before and widget.get("mapped")
            ),
            None,
        ),
    )
    smoke.record("keyboard-widget-added", widget=added)


def empty_frame_recovery(smoke):
    # Establish the empty arrangement through the real placement owner, then
    # recover using only the shell's keyboard and accessible canvas menu.
    smoke.control.request('frame-settings {"allow_hide_tools":true}')
    smoke.control.request("layout-tool")
    state = layout(smoke)
    for output in state["outputs"]:
        for widget in output["widgets"]:
            command = {
                "session": state["session"],
                "output": output["output"],
                "action": "remove",
                "target": {"kind": "widget", "id": widget["id"]},
            }
            smoke.control.request(f"layout-command {json.dumps(command)}")
    smoke.control.request(
        "layout-command "
        + json.dumps(
            {"session": state["session"], "output": state["output"], "action": "done"}
        )
    )
    smoke.until(
        "empty frame settled",
        lambda: (
            not (state := layout(smoke))["editing"]
            and not state["animating"]
            and all(not output["widgets"] for output in state["outputs"])
        ),
    )
    with Input(smoke.control) as inputs:
        inputs.chord("shift+f10")
    activate(smoke, "Add Widget…", "menu item")
    editor = "Search widgets to add to the canvas"
    smoke.until(
        "recovery picker focused",
        lambda: a11y(smoke, "find", f"//*[@name='{editor}' and @focused='true']")[
            "elements"
        ],
    )
    with Input(smoke.control) as inputs:
        inputs.type("launcher")
    smoke.until(
        "recovery launcher selected",
        lambda: (
            smoke.control.query("canvas-search-status")["selected_id"]
            == "widget:launcher"
        ),
    )
    with Input(smoke.control) as inputs:
        inputs.chord("enter")
    launcher = smoke.until(
        "launcher restored without a dock",
        lambda: next(
            (widget for widget in copies(smoke, "launcher") if widget.get("mapped")),
            None,
        ),
    )
    with Input(smoke.control) as inputs:
        inputs.click(*center(launcher))
    smoke.until(
        "standalone launcher focused Apps List",
        lambda: a11y(
            smoke, "find", "//*[@name='Search apps and windows...' and @focused='true']"
        )["elements"],
    )
    smoke.capture("empty-frame-recovered")
    with Input(smoke.control) as inputs:
        inputs.chord("escape")
    smoke.record("empty-frame-recovery-verified", launcher=launcher["id"])
