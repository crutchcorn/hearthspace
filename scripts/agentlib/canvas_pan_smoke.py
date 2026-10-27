"""Native mouse Pan acceptance through visible shell UI and real Foot pixels."""

import argparse
import json
import subprocess
from pathlib import Path

from . import canvas_pan_checks as checks
from . import canvas_pan_fixture as fixture
from . import canvas_pan_ui as ui
from .control import ControlError
from .daily import DailySmoke
from .input import BUTTONS, Input
from .png import crop
from .visual import pixel_rect, verify_move


def pan(smoke, name, delta=(43, -27)):
    start = fixture.blank(smoke, delta)
    before = smoke.control.status()
    window = smoke.window(smoke.target)
    with Input(smoke.control) as inputs:
        inputs.motion(*start)
        image = smoke.capture(f"{name}-grab")
        inputs.down("pointer-button", BUTTONS["left"])
        pressed = smoke.capture(f"{name}-grabbing")
        # On blank canvas a press has not moved any content. Retain the native
        # cursor frames and require a visible hover-to-held shape transition.
        bounds = (start[0] - 24, start[1] - 24, start[0] + 32, start[1] + 32)
        if crop(image[1], pixel_rect(image[0], bounds)) == crop(
            pressed[1], pixel_rect(pressed[0], bounds)
        ):
            raise ControlError("Pan press did not change the captured cursor region")
        # Several subpixel increments expose per-event rounding accumulation.
        for fraction in (0.13, 0.37, 0.61, 1):
            inputs.motion(
                start[0] + delta[0] * fraction, start[1] + delta[1] * fraction
            )
        active = smoke.control.status()
        checks.verify_pan(before, active, delta)
        active_image = smoke.capture(f"{name}-active")
        proof = checks.verify_marker(
            image,
            active_image,
            window,
            smoke.window(smoke.target),
            before["viewport"],
            active["viewport"],
        )
        inputs.up("pointer-button", BUTTONS["left"])
        inputs.motion(start[0] + delta[0] + 8, start[1] + delta[1] + 8)
    after = smoke.control.status()
    checks.verify_pan(before, after, delta)
    smoke.capture(f"{name}-released")
    smoke.record(
        "canvas-pan-drag-verified",
        name=name,
        delta=delta,
        before=before["viewport"],
        after=after["viewport"],
        cursor_shape_names_exposed=False,
        cursor_press_pixels_changed=True,
        **proof,
    )


def cancelled(smoke):
    start = fixture.blank(smoke, (32, 20))
    before = smoke.control.status()
    with Input(smoke.control) as inputs:
        inputs.motion(*start)
        inputs.down("pointer-button", BUTTONS["left"])
        inputs.motion(start[0] + 32, start[1] + 20)
        held = smoke.control.status()
        checks.verify_pan(before, held, (32, 20))
        inputs.down("pointer-button", BUTTONS["right"])
        inputs.chord("esc")
        ui.wait_tool(smoke, "pan")
        # Move over a real titlebar while the cancelled sequence is draining.
        # Premature handoff could start a window drag or deliver its release.
        window = smoke.window(smoke.target)
        target = (
            window["x"] + window["width"] / 2,
            window["y"] + window["titlebar_height"] / 2,
        )
        inputs.motion(*target)
        checks.verify_pan(before, smoke.control.status(), (32, 20))
        inputs.up("pointer-button", BUTTONS["left"])
        inputs.motion(target[0] + 20, target[1] + 10)
        # A new left press while the secondary button is still held must be
        # swallowed by the old owner too, even over a client decoration.
        inputs.down("pointer-button", BUTTONS["left"])
        inputs.motion(target[0] + 40, target[1] + 20)
        inputs.up("pointer-button", BUTTONS["left"])
        inputs.up("pointer-button", BUTTONS["right"])
    after = smoke.control.status()
    checks.verify_pan(before, after, (32, 20))
    if after.get("shell_panels") or smoke.control.query("desktop-status").get(
        "context"
    ):
        raise ControlError("cancelled pointer releases activated a menu")
    smoke.capture("canvas-pan-cancelled-drained")
    smoke.record(
        "canvas-pan-cancelled", viewport=after["viewport"], secondary_drained=True
    )
    pan(smoke, "canvas-pan-fresh-after-cancel", delta=(-31, 19))


def desktop_keys(smoke):
    smoke.control.request("canvas-tool select")
    ui.wait_tool(smoke, "select")
    with Input(smoke.control) as inputs:
        inputs.click(*fixture.blank(smoke))
    smoke.until(
        "blank desktop owns keyboard focus",
        lambda: not any(item["focused"] for item in smoke.control.status()["windows"]),
    )
    for key, tool, meta_first in (
        ("h", "pan", False),
        ("v", "select", True),
        ("h", "pan", True),
    ):
        ui.shortcut(smoke, key, tool, meta_first=meta_first)
    smoke.capture("canvas-pan-desktop-shortcuts")
    smoke.record("canvas-pan-desktop-shortcuts", keys=["Meta+H", "Meta+V", "Meta+H"])


def client_input(smoke):
    destination = fixture.receive(smoke, "canvas-pan-client-hvh")
    # Move keyboard focus away with an actual desktop click before proving the
    # Pan tool permits an ordinary client click to restore it.
    desktop_keys(smoke)
    before = smoke.control.status()
    with Input(smoke.control) as inputs:
        inputs.click(*fixture.client_point(smoke))
        inputs.type("hvh\n")
    fixture.received(smoke, destination, "hvh")
    ui.wait_tool(smoke, "pan")
    checks.verify_pan(before, smoke.control.status(), (0, 0))
    window = smoke.window(smoke.target)
    if window["titlebar_height"] <= 0:
        raise ControlError(
            "Foot requires compositor decorations for titlebar acceptance"
        )
    image = smoke.capture("canvas-pan-client-input")
    x = window["x"] + window["width"] / 2
    y = window["y"] + window["titlebar_height"] / 2
    with Input(smoke.control) as inputs:
        inputs.drag(x, y, x + 24, y + 18)
    moved = smoke.until(
        "titlebar moves the client while Pan is selected",
        lambda: (
            current
            if (current := smoke.window(smoke.target))["canvas_x"] != window["canvas_x"]
            else None
        ),
    )
    if not checks.same_view(before["viewport"], smoke.control.status()["viewport"]):
        raise ControlError("client titlebar drag panned the canvas")
    for axis, distance in (("x", 24), ("y", 18)):
        if abs(moved[axis] - window[axis] - distance) > 1:
            raise ControlError("client titlebar did not follow the pointer")
    captured = smoke.capture("canvas-pan-client-titlebar")
    verify_move(image, captured, window, moved)
    smoke.record("canvas-pan-client-routing", typed="hvh", titlebar_moves_window=True)


def run(smoke):
    fixture.setup(smoke)
    ui.select_pan(smoke)
    for scale in (0.8, 1.0, 2.0):
        fixture.prepare(smoke, scale)
        ui.wait_tool(smoke, "pan")
        pan(smoke, f"canvas-pan-scale-{scale}")
    fixture.prepare(smoke, 1)
    cancelled(smoke)
    client_input(smoke)
    smoke.record(
        "canvas-pan-verified",
        scales=[0.8, 1.0, 2.0],
        desktop_icon="not exercised; blank canvas and actual client regions covered",
        input_source="existing control Input pointer bridge",
    )


class CanvasPanSmoke(DailySmoke):
    def __init__(self, args):
        args.captures, args.notifications = 1, False
        super().__init__(args)

    def launcher(self, operation, *extra, expected=0):
        if operation == "start":
            extra = (*extra, "--isolated-profile")
        return super().launcher(operation, *extra, expected=expected)

    def workflow(self):
        self.manifest = self.connect()
        try:
            self.step("ready", DailySmoke.ready)
            self.step("canvas-pan", run)
        except BaseException:
            self.failure_evidence("canvas-pan-failed")
            raise


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--session", type=Path, required=True, help="fresh evidence directory"
    )
    parser.add_argument("--timeout", type=float, default=180)
    parser.add_argument("--total-timeout", type=float, default=300)
    parser.add_argument("--log-level", default="info")
    args = parser.parse_args()
    try:
        CanvasPanSmoke(args).run()
        return 0
    except (
        ControlError,
        OSError,
        ValueError,
        KeyError,
        subprocess.SubprocessError,
        KeyboardInterrupt,
    ) as error:
        print(json.dumps({"ok": False, "error": str(error)}), flush=True)
        return 1
