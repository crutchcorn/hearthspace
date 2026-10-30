"""Small real Foot fixture and target selection for mouse-pan acceptance."""

import shlex

from .canvas_pan_checks import blank_start
from .control import ControlError
from .daily_checks import terminal
from .input import Input
from .shell_surfaces import observed_status
from .viewport_fixture import set_viewport
from .visual import marker_bounds


def blank(smoke, delta=(0, 0)):
    status = observed_status(smoke)
    desktop = smoke.control.query("desktop-status")
    point = blank_start(status, desktop, delta)
    smoke.record("canvas-pan-blank-target", point=point, delta=delta)
    return point


def client_point(smoke):
    window = smoke.window(smoke.target)
    return window["x"] + window["width"] / 2, window["y"] + window["height"] * 0.7


def configured(window, x, y, width, height):
    # Foot rounds floating resize requests down to complete character cells.
    # Match the 32px setup bound in touch_drag_checks and shell_backdrop;
    # pan assertions still compare the exact observed geometry before/after.
    return bool(
        window
        and (window["canvas_x"], window["canvas_y"]) == (x, y)
        and 0 <= width - window["content_width"] <= 32
        and 0 <= height - window["content_height"] <= 32
        and not window["maximized"]
        and not window["fullscreen"]
    )


def prepare(smoke, scale):
    smoke.control.request(f"restore-window {smoke.target}")
    set_viewport(smoke, x=37, y=-29, scale=scale)
    output = smoke.control.status()["outputs"][0]
    width = round(min(420, output["logical_width"] * 0.35) / scale)
    height = round(min(260, output["logical_height"] * 0.35) / scale)
    if width < 160 or height < 90:
        raise ControlError("output too small for a visible Foot pan fixture")
    smoke.control.request(f"resize-window {smoke.target} {width} {height}")
    x = round(37 + (output["logical_x"] + 80) / scale)
    y = round(-29 + (output["logical_y"] + 100) / scale)
    smoke.control.request(f"move-window {smoke.target} {x} {y}")
    smoke.until(
        "pan fixture configured",
        lambda: configured(smoke.window(smoke.target), x, y, width, height),
    )
    smoke.control.request(f"focus-window {smoke.target}")
    with Input(smoke.control) as inputs:
        inputs.type(
            "printf '\\033[2J\\033[H\\033[48;2;0;255;0mPAN_MARKER\\033[0m\\n'\n"
        )

    def visible():
        window = smoke.window(smoke.target)
        image = smoke.control.capture(output["name"])
        try:
            marker_bounds(image, window)
        except ControlError:
            return None
        return window

    smoke.until("presented green Foot pan marker", visible)
    smoke.capture(f"canvas-pan-fixture-{scale}")


def setup(smoke):
    terminal(smoke)
    prepare(smoke, 1)


def receive(smoke, name):
    """Arm a real terminal read; caller subsequently types the exact expected text."""
    destination = smoke.artifacts / f"{name}.txt"
    smoke.control.request(f"focus-window {smoke.target}")
    with Input(smoke.control) as inputs:
        inputs.type(
            "IFS= read -r pan_received; "
            f"printf '%s' \"$pan_received\" > {shlex.quote(str(destination))}\n"
        )
    return destination


def received(smoke, destination, expected):
    smoke.until(
        "exact app text received through pointer focus",
        lambda: destination.is_file() and destination.read_text() == expected,
    )
    smoke.record("canvas-pan-app-text", path=str(destination), expected=expected)
