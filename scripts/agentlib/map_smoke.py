"""Shared canvas-map acceptance using real pointer input and presented pixels."""

import argparse
import json
import math
import shutil
import subprocess
from pathlib import Path

from . import map_checks as checks
from . import map_drag
from .control import ControlError
from .daily import DailySmoke
from .daily_checks import a11y, terminal
from .input import Input
from .png import crop


def map_pixels(capture, surface, bounds):
    """Crop the observed image, excluding buttons, cursor, and unrelated clients."""
    metadata, png = capture
    scale = metadata["scale"]
    x = (surface["x"] - metadata["logical_x"]) * scale + bounds["x"]
    y = (surface["y"] - metadata["logical_y"]) * scale + bounds["y"]
    rect = (
        math.floor(x),
        math.floor(y),
        math.ceil(x + bounds["width"]),
        math.ceil(y + bounds["height"]),
    )
    if not (
        0 <= rect[0] < rect[2] <= metadata["width"]
        and 0 <= rect[1] < rect[3] <= metadata["height"]
    ):
        raise ControlError("observed minimap image lies outside its captured output")
    return crop(png, rect)


def percentage(smoke):
    viewport = smoke.until(
        "settled canvas zoom",
        lambda: (
            view
            if not (view := smoke.control.status()["viewport"])["animating"]
            else None
        ),
    )
    scale = viewport["scale"]
    label = f"{scale * 100:.0f}%"
    elements = smoke.until(
        "current canvas zoom percentage",
        lambda: a11y(smoke, "find", checks.selector(label))["elements"],
    )
    smoke.record("map-zoom-percentage", scale=scale, label=label, elements=elements)


def outside_client(smoke):
    surface, output = checks.map_surface(smoke)
    window = smoke.window(smoke.target)
    for fraction in (0.25, 0.5, 0.75):
        x = window["x"] + window["width"] * fraction
        y = window["y"] + window["height"] * 0.7
        if (
            output["logical_x"] <= x < output["logical_x"] + output["logical_width"]
            and output["logical_y"] + 48
            < y
            < output["logical_y"] + output["logical_height"]
            and not (
                surface["x"] <= x < surface["x"] + surface["width"]
                and surface["y"] <= y < surface["y"] + surface["height"]
            )
        ):
            with Input(smoke.control) as inputs:
                inputs.click(x, y)
            checks.closed(smoke)
            smoke.until(
                "outside client receives focus",
                lambda: smoke.window(smoke.target)["focused"],
            )
            return
    raise ControlError("fixture has no exposed client point outside the map")


def run(smoke):
    terminal(smoke)
    before = smoke.control.status()
    original = smoke.window(smoke.target)
    checks.click(smoke, "Canvas map")
    checks.opened(smoke)
    element = checks.map_image(smoke)
    surface, output = checks.map_surface(smoke)
    opened = smoke.control.status()
    if checks.window_geometry(opened) != checks.window_geometry(before):
        raise ControlError("opening canvas map changed application window geometry")
    if opened["viewport"] != before["viewport"]:
        raise ControlError("opening canvas map changed the viewport")
    expected_count = len(before["windows"])
    if not element["name"].startswith(f"Canvas minimap: {expected_count} windows."):
        raise ControlError("minimap window count includes missing or shell windows")
    smoke.record("map-opened", surface=surface, accessible_image=element)
    first = smoke.capture("map-open", output["name"])
    baseline = map_pixels(first, surface, element["bounds"])
    smoke.control.request(f"move-window {smoke.target} 12000 -10000")
    smoke.until(
        "distant fixture geometry",
        lambda: smoke.window(smoke.target)["canvas_x"] == 12000,
    )

    def updated_image():
        captured = smoke.control.capture(output["name"])
        return (
            captured
            if map_pixels(captured, surface, element["bounds"]) != baseline
            else None
        )

    smoke.until("minimap pixels update after moving a distant window", updated_image)
    smoke.capture("map-distant-window", output["name"])
    smoke.control.request(
        f"move-window {smoke.target} {original['canvas_x']} {original['canvas_y']}"
    )
    smoke.until(
        "fixture returned to its original canvas position",
        lambda: (
            checks.window_geometry(smoke.control.status())
            == checks.window_geometry(before)
        ),
    )
    scale = before["viewport"]["scale"]
    checks.click(smoke, "Zoom out")
    smoke.until(
        "map Zoom out reduces canvas scale",
        lambda: smoke.control.status()["viewport"]["scale"] < scale,
    )
    percentage(smoke)
    smoke.capture("map-zoom-out")
    smaller = smoke.control.status()["viewport"]["scale"]
    checks.click(smoke, "Zoom in")
    smoke.until(
        "map Zoom in increases canvas scale",
        lambda: smoke.control.status()["viewport"]["scale"] > smaller,
    )
    percentage(smoke)
    smoke.capture("map-zoom-in")
    checks.close(smoke)
    checks.click(smoke, "Canvas map")
    checks.opened(smoke)
    with Input(smoke.control) as inputs:
        inputs.chord("esc")
    checks.closed(smoke)
    checks.click(smoke, "Canvas map")
    checks.opened(smoke)
    checks.click(smoke, "Canvas map")
    checks.closed(smoke)
    checks.click(smoke, "Canvas map")
    checks.opened(smoke)
    outside_client(smoke)
    for touchscreen in (
        [False, True] if getattr(smoke.args, "touchscreen", False) else [False]
    ):
        checks.click(smoke, "Canvas map")
        checks.opened(smoke)
        dragged = map_drag.run(smoke, touchscreen=touchscreen)
        checks.close(smoke)
        final_viewport = smoke.control.status()["viewport"]
        if any(final_viewport[key] != dragged[key] for key in ("x", "y", "scale")):
            raise ControlError("dismissing the map lost its dragged canvas position")
    if set(checks.window_geometry(smoke.control.status())) != set(
        checks.window_geometry(before)
    ):
        raise ControlError("canvas map appeared as an application or removed a window")
    smoke.capture("map-dismissed")
    smoke.record(
        "canvas-map-verified",
        live_pixels=True,
        zoom_directions=2,
        dismissals=[
            "Close canvas map",
            "Escape",
            "Canvas map toggle",
            "outside client",
        ],
    )


class MapSmoke(DailySmoke):
    def __init__(self, args):
        args.captures, args.notifications = 1, False
        super().__init__(args)

    def workflow(self):
        self.manifest = self.connect()
        self.step("ready", DailySmoke.ready)
        try:
            self.step("canvas-map", run)
        except BaseException:
            self.failure_evidence("canvas-map-failed")
            raise


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--session", type=Path, required=True)
    parser.add_argument("--timeout", type=float, default=120)
    parser.add_argument("--total-timeout", type=float, default=240)
    parser.add_argument("--log-level", default="info")
    parser.add_argument(
        "--touchscreen",
        action="store_true",
        help="also drag through an owned native uinput touchscreen",
    )
    args = parser.parse_args()
    try:
        if args.touchscreen:
            if shutil.which("libinput") is None:
                raise ControlError(
                    "--touchscreen requires libinput-tools and python3-evdev"
                )
            subprocess.run(
                ["/usr/bin/python3", "-c", "import evdev"], check=True, timeout=5
            )
        MapSmoke(args).run()
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
