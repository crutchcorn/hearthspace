"""Canvas Search touch acceptance through owned uinput/libinput contacts."""

import argparse
import json
import subprocess
import time
from pathlib import Path

from .control import ControlError
from .daily import DailySmoke
from .daily_checks import a11y, terminal
from .input import Input
from .pinch_touchscreen import to_device
from .search_touch import touchscreen


def search(smoke):
    return smoke.control.query("canvas-search-status")


def require(condition, message):
    if not condition:
        raise ControlError(message)


def early_typing(smoke):
    started = time.monotonic()
    with Input(smoke.control) as inputs:
        inputs.chord("ctrl+k")
        inputs.type(smoke.marker)
    current = smoke.until(
        "uppercase early typing",
        lambda: (state := search(smoke))["query"] == smoke.marker and state,
    )
    smoke.record(
        "cold-search-typing-ready",
        elapsed_seconds=time.monotonic() - started,
        query=smoke.marker,
        scene=current["scene"],
    )
    with Input(smoke.control) as inputs:
        inputs.chord("escape")
    smoke.until("early typing dismissed", lambda: not search(smoke)["active"])


def opened(smoke):
    with Input(smoke.control) as inputs:
        inputs.chord("ctrl+k")
    smoke.until(
        "native search editor",
        lambda: (
            "Search apps, windows, files, folders, controls, and tools"
            in a11y(smoke, "snapshot")["snapshot"]
        ),
    )
    with Input(smoke.control) as inputs:
        inputs.type(smoke.marker)
    smoke.until(
        "typed window search",
        lambda: (state := search(smoke))["active"] and state["query"] == smoke.marker,
    )
    time.sleep(0.4)
    state = search(smoke)
    preview = next(
        item["display"]
        for item in state["scene"]
        if item["id"] == f"window:{smoke.target}"
    )
    smoke.capture("touch-search-open")
    return state, (
        preview["x"] + preview["width"] / 2,
        preview["y"] + preview["height"] / 2,
    )


def tap(device, output, point):
    device.request("begin", fingers=1, points=to_device([point], output))
    device.request("end")


def still_searching(smoke, expected):
    time.sleep(0.1)  # Dispatch the final kernel contact frame before asserting.
    current = search(smoke)
    require(
        current["active"] and current["session"] == expected["session"],
        "cancelled or multi-contact touch activated/dismissed search",
    )
    return current


def regression(smoke):
    output = smoke.control.status()["outputs"][0]
    viewport = smoke.control.status()["viewport"]
    state, center = opened(smoke)
    neighbor = (center[0] + 32, center[1] + 32)
    with touchscreen(smoke, "canvas-results") as device:
        # Two contacts may arrive together or after a single-contact drag has
        # returned to its original target. Neither sequence is a result tap.
        device.request("begin", fingers=2, points=to_device([center, neighbor], output))
        device.request("end")
        still_searching(smoke, state)
        device.request("begin", fingers=1, points=to_device([center], output))
        device.request("move", points=to_device([(center[0] + 20, center[1])], output))
        device.request("move", points=to_device([center], output))
        device.request("add-finger", point=to_device([neighbor], output)[0])
        device.request("end")
        still_searching(smoke, state)
        device.request("begin", fingers=1, points=to_device([center], output))
        device.request("move", points=to_device([(center[0] + 20, center[1])], output))
        device.request("move", points=to_device([center], output))
        with touchscreen(smoke, "canvas-other-device") as other:
            tap(other, output, neighbor)
        device.request("end")
        still_searching(smoke, state)
        require(smoke.control.status()["viewport"] == viewport, "touch changed canvas")
        tap(device, output, center)
        smoke.until("touch activated preview", lambda: not search(smoke)["active"])
        smoke.until("window focused", lambda: smoke.window(smoke.target)["focused"])
    smoke.capture("touch-preview-activated")

    state, center = opened(smoke)
    # Device removal must cancel a press even when it ends on its original tile.
    with touchscreen(smoke, "canvas-removal") as device:
        device.request("begin", fingers=1, points=to_device([center], output))
        device.request("move", points=to_device([(center[0] + 20, center[1])], output))
        device.request("move", points=to_device([center], output))
    still_searching(smoke, state)

    with touchscreen(smoke, "canvas-field") as device:
        field = state["field"]
        close = (field["x"] + field["width"] - 24, field["y"] + 27)
        # Native wl_touch on the editor remains usable and is not duplicated as
        # a compositor pointer tap. A second contact cancels the close button.
        device.request("begin", fingers=1, points=to_device([close], output))
        device.request(
            "add-finger", point=to_device([(close[0] - 50, close[1])], output)[0]
        )
        device.request("end")
        still_searching(smoke, state)
        tap(device, output, (field["x"] + 100, field["y"] + 27))
        with Input(smoke.control) as inputs:
            inputs.chord("ctrl+a")
            inputs.type("no-match-native-touch")
        state = smoke.until(
            "native editor still accepts input",
            lambda: (
                (current := search(smoke))["query"] == "no-match-native-touch"
                and current
            ),
        )
        tap(device, output, close)
        smoke.until("native close button touch", lambda: not search(smoke)["active"])
    state, _ = opened(smoke)
    with touchscreen(smoke, "canvas-background") as device:
        tap(device, output, (output["logical_x"] + 8, output["logical_y"] + 8))
        smoke.until("empty touch dismissed search", lambda: not search(smoke)["active"])
    smoke.capture("touch-search-dismissed")
    smoke.record(
        "canvas-search-touch-passed",
        preview_activation=True,
        empty_dismissal=True,
        multi_contact_cancel=True,
        device_removal_cancel=True,
        native_field_touch=True,
    )


class CanvasSearchTouchSmoke(DailySmoke):
    def launcher(self, operation, *extra, expected=0):
        if operation == "start":
            extra = (*extra, "--isolated-profile")
        return super().launcher(operation, *extra, expected=expected)

    def workflow(self):
        self.manifest = self.connect()
        require(len(self.control.status()["outputs"]) == 1, "requires one output")
        self.step("ready", DailySmoke.ready)
        self.step("terminal", terminal)
        self.step("early-uppercase-typing", early_typing)
        self.step("canvas-search-touch", regression)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--session", type=Path, required=True)
    parser.add_argument("--timeout", type=float, default=90)
    parser.add_argument("--total-timeout", type=float, default=240)
    parser.add_argument("--log-level", default="info")
    parser.set_defaults(captures=1, notifications=False)
    args = parser.parse_args()
    try:
        CanvasSearchTouchSmoke(args).run()
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
