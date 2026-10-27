"""Pointer activation from private AT-SPI bounds and current native placement."""

from .control import ControlError
from .daily_checks import a11y
from .input import Input
from .map_checks import SHELL, button_center
from .shell_surfaces import observed_status, widget_surface

TRIGGERS = (
    "Select desktop items, choose canvas tool",
    "Pan canvas, choose canvas tool",
    "Search canvas, choose canvas tool",
    "Canvas tools",
)


def selector(names):
    choices = " or ".join(f"@name='{name}'" for name in names)
    return (
        f"{SHELL}//*[(@role='button' or @role='toggle button') "
        f"and @showing='true' and ({choices})]"
    )


def surface(smoke, menu):
    status = observed_status(smoke)
    if len(status["outputs"]) != 1:
        raise ControlError("mouse pan acceptance requires one active output")
    output = status["outputs"][0]
    if not menu:
        return output, widget_surface(status, "tools")
    matches = [
        item for item in status.get("shell_panels", []) if item["kind"] == "controls"
    ]
    if len(matches) > 1:
        raise ControlError("canvas tools menu has ambiguous native surfaces")
    return output, matches[0] if matches else None


def observed_point(elements, output, owning):
    frames = [item for item in elements if item.get("role") == "Frame"]
    buttons = [
        item for item in elements if item.get("role") in ("Button", "ToggleButton")
    ]
    if len(frames) != 1 or len(buttons) != 1:
        return None
    bounds = frames[0].get("bounds") or {}
    if any(
        bounds.get(axis) != owning[axis] * output["scale"]
        for axis in ("width", "height")
    ):
        return None
    return button_center(buttons[0].get("bounds"), output, owning)


def click(smoke, names, *, menu=False):
    target = selector(names)
    query = f"{target} | {target}/ancestor::*[@role='frame'][1]"

    def ready():
        before = surface(smoke, menu)
        if before[1] is None:
            return None
        observed = a11y(smoke, "find", query)["elements"]
        after = surface(smoke, menu)
        if before != after:
            return None
        point = observed_point(observed, *after)
        return (point, observed) if point else None

    point, observed = smoke.until("settled canvas tool pointer target", ready)
    smoke.record("canvas-tool-pointer-target", point=point, elements=observed)
    with Input(smoke.control) as inputs:
        inputs.click(*point)


def wait_tool(smoke, name):
    return smoke.until(
        f"authoritative canvas tool becomes {name}",
        lambda: (
            state
            if (state := smoke.control.query("layout-status")).get("canvas_tool")
            == name
            else None
        ),
    )


def wait_trigger(smoke, title):
    target = selector((f"{title}, choose canvas tool", "Canvas tools"))
    # SemanticButton's toggled Button becomes an AT-SPI ToggleButton. The
    # vendored accesskit_atspi_common bridge maps its true state to Pressed,
    # not Checked (node.rs); native snapshots expose pressed="true".
    return smoke.until(
        f"shell reflects selected {title} after its worker completes",
        lambda: a11y(smoke, "find", target + "[@pressed='true']")["elements"],
    )


def select_pan(smoke):
    # The wide trigger first selects its remembered primary if no tool is
    # active. Establish Select before clicking it to open the actual picker.
    smoke.control.request("canvas-tool select")
    wait_tool(smoke, "select")
    wait_trigger(smoke, "Select desktop items")
    click(smoke, TRIGGERS)
    smoke.capture("canvas-tools-menu")
    click(smoke, ("Pan canvas",), menu=True)
    # Pointer release only queues the shell worker. Query its authoritative
    # compositor result before taking the acceptance snapshot.
    layout = wait_tool(smoke, "pan")
    smoke.until(
        "canvas tools menu dismissed",
        lambda: not smoke.control.status().get("shell_panels"),
    )
    wait_trigger(smoke, "Pan canvas")
    tree = a11y(smoke, "snapshot")["snapshot"]
    (smoke.artifacts / "canvas-pan-selected.xml").write_text(tree)
    smoke.capture("canvas-pan-selected")
    smoke.record("canvas-pan-selected-state", layout=layout)
