"""Pinch acceptance through uinput, libinput, and presented native frames."""

import argparse
import math
import re
import shutil
import subprocess
import time
from pathlib import Path

from .control import ControlError, save_capture
from .gesture_trace import gesture
from .input import Input
from .pinch_assertions import same_viewport, verify_removal, verify_zoom
from .pinch_touchscreen import acceptance as touchscreen_acceptance
from .smoke import Smoke
from .touchpad import TouchDevice
from .visual import marker_bounds


def terminal(smoke):
    smoke.control.request("spawn foot")
    window = smoke.until("mapped terminal", smoke.mapped_terminal)
    with Input(smoke.control) as inputs:
        inputs.click(window["x"] + 100, window["y"] + window["height"] / 2)
        inputs.type(
            "printf '\\033[2J\\033[H\\033[48;2;0;255;0m"
            "HEARTHSPACE_PINCH_MARKER\\033[0m\\n'\n"
        )

    def visible():
        window = smoke.window()
        image = smoke.control.capture()
        try:
            marker_bounds(image, window)
        except ControlError:
            return None
        return window, image

    window, image = smoke.until("visible terminal marker", visible)
    smoke.record(
        "terminal-ready",
        window=window,
        **save_capture(image, smoke.artifacts / "terminal.png"),
    )
    return window


def zoom(smoke, device, name, anchor, start, end):
    with Input(smoke.control) as inputs:
        inputs.motion(*anchor)
    before = smoke.control.status()["viewport"]
    before_window = smoke.window()
    before_image = smoke.capture(f"{name}-before")
    trace_path = smoke.artifacts / "libinput.log"
    trace_start = trace_path.stat().st_size
    device.request("begin", fingers=3, distance=start)
    device.request("move", distance=end)

    def changed():
        viewport = smoke.control.status()["viewport"]
        ratio = viewport["scale"] / before["scale"]
        return (
            viewport
            if viewport["pinch_active"]
            and (ratio < 0.85 if end < start else ratio > 1.15)
            else None
        )

    after = smoke.until(f"active {name}", changed)
    while True:
        time.sleep(min(0.04, smoke.control.deadline.remaining()))
        current = smoke.control.status()["viewport"]
        if same_viewport(after, current):
            break
        after = current
    after_image = smoke.capture(f"{name}-active")
    after_window = smoke.window()
    observed = gesture(trace_path.read_text()[trace_start:], "PINCH")
    if abs(observed["scale"] - after["scale"] / before["scale"]) > 0.015:
        raise ControlError("libinput trace scale disagrees with the applied pinch")
    assertions = verify_zoom(
        before,
        after,
        anchor,
        before_window,
        after_window,
        before_image,
        after_image,
        translation=observed["translation"],
    )
    device.request("end")
    smoke.until(
        "pinch end", lambda: not smoke.control.status()["viewport"]["pinch_active"]
    )
    settled = smoke.control.status()["viewport"]
    if not same_viewport(after, settled):
        raise ControlError("normal pinch end changed the displayed transform")
    smoke.record(
        name,
        before=before,
        after=settled,
        anchor=anchor,
        libinput=observed,
        **assertions,
    )


def acceptance(smoke):
    window = terminal(smoke)
    anchor = (window["x"] + 80, window["y"] + window["titlebar_height"] + 70)
    with (smoke.artifacts / "libinput.log").open("w") as log:
        with TouchDevice(smoke.directory, log) as device:
            smoke.record("virtual-touchpad", **device.device)
            zoom(smoke, device, "pinch-out", anchor, 900, 550)
            zoom(smoke, device, "pinch-in", anchor, 550, 850)
            before = smoke.control.status()["viewport"]
            device.request("begin", fingers=2, distance=900)
            device.request("move", distance=550)
            device.request("end")
            # Input has finished; allow its final kernel frame to dispatch.
            time.sleep(0.1)
            ignored = smoke.control.status()["viewport"]
            if ignored["pinch_active"] or not same_viewport(before, ignored):
                raise ControlError("two-finger gesture changed the canvas")
            smoke.record("two-finger-ignored", before=before, after=ignored)
            window = smoke.window()
            if window["titlebar_height"] <= 0:
                raise ControlError(
                    "pinch drag regression requires a decorated terminal"
                )
            before = smoke.control.status()["viewport"]
            with Input(smoke.control) as inputs:
                inputs.motion(
                    window["x"] + 100, window["y"] + window["titlebar_height"] / 2
                )
                inputs.down("pointer-button", 272)
                device.request("begin", fingers=3, distance=900)
                device.request("move", distance=550)
                device.request("end")
                time.sleep(0.1)
                ignored = smoke.control.status()["viewport"]
                if ignored["pinch_active"] or not same_viewport(before, ignored):
                    raise ControlError("pinch during a window drag changed the canvas")
                inputs.up("pointer-button", 272)
            smoke.record("drag-pinch-ignored", before=before, after=ignored)
            with Input(smoke.control) as inputs:
                inputs.motion(*anchor)
            before = smoke.control.status()["viewport"]
            device.request("begin", fingers=3, distance=900)
            device.request("move", distance=650)
            smoke.until(
                "pinch before finger-count cancellation",
                lambda: (
                    (current := smoke.control.status()["viewport"])["pinch_active"]
                    and not same_viewport(before, current)
                ),
            )
            smoke.capture("cancel-active")
            device.request("add-finger")
            smoke.until(
                "finger-count cancellation",
                lambda: not smoke.control.status()["viewport"]["pinch_active"],
            )
            cancelled = smoke.control.status()["viewport"]
            if not same_viewport(before, cancelled):
                raise ControlError(
                    "finger-count cancellation did not restore the initial viewport"
                )
            device.request("end")
            smoke.capture("cancel-restored")
            smoke.record("finger-count-cancelled", before=before, after=cancelled)
            before = smoke.control.status()["viewport"]
            smoke.capture("removal-before")
            device.request("begin", fingers=3, distance=900)
            device.request("move", distance=650)
            smoke.until(
                "pinch before unplug",
                lambda: (
                    (current := smoke.control.status()["viewport"])["pinch_active"]
                    and not same_viewport(before, current)
                ),
            )
            smoke.capture("removal-active")
            active_at_removal = smoke.control.status()["viewport"]
            smoke.record(
                "device-removal-requested", before=before, active=active_at_removal
            )
            # Let kernel/libinput decide whether unplug cancels or releases the
            # active contacts. The retained trace determines the exact assertion.
        smoke.until(
            "inactive pinch after unplug",
            lambda: not smoke.control.status()["viewport"]["pinch_active"],
        )
        after = smoke.control.status()["viewport"]
        smoke.capture("removal-after")
        smoke.record(
            "device-removal-observed",
            before=before,
            active=active_at_removal,
            after=after,
            device_removed=not Path(device.device["device"]).exists(),
        )
        trace = (smoke.artifacts / "libinput.log").read_text()
        removal = verify_removal(trace, before, active_at_removal, after)
        if Path(device.device["device"]).exists():
            raise ControlError("virtual touchpad remained after its worker exited")
        smoke.record(
            "device-removal-verified",
            before=before,
            after=after,
            device_removed=True,
            **removal,
        )
    trace = (smoke.artifacts / "libinput.log").read_text()
    finger_counts = set(re.findall(r"GESTURE_PINCH_BEGIN\s+\S+\s+([23])\b", trace))
    if finger_counts != {"2", "3"} or not re.search(
        r"GESTURE_PINCH_END[^\n]*cancelled", trace
    ):
        raise ControlError(
            "libinput trace did not prove two/three-finger pinches and cancellation"
        )
    smoke.record(
        "libinput-evidence", finger_counts=sorted(finger_counts), cancellation=True
    )


def run(args):
    if shutil.which("foot") is None or shutil.which("libinput") is None:
        raise ControlError(
            "pinch smoke requires foot, libinput-tools, and python3-evdev"
        )
    subprocess.run(["/usr/bin/python3", "-c", "import evdev"], check=True, timeout=5)
    if (args.session / "session.json").exists():
        raise ControlError("pinch smoke requires a new session directory")
    smoke = Smoke(args)
    error = None
    try:
        smoke.launcher(
            "start", "--no-build", "--watchdog", str(math.ceil(args.timeout + 90))
        )
        manifest = smoke.connect()
        smoke.record("ready", manifest=manifest, status=smoke.control.status())
        if args.device != "touchscreen":
            acceptance(smoke)
        if args.device != "touchpad":
            touchscreen_acceptance(smoke, smoke.window() or terminal(smoke))
    except BaseException as failure:
        error = failure
        smoke.record("operation-failed", error=str(failure))
        raise
    finally:
        if (smoke.directory / "session.json").exists():
            try:
                smoke.launcher("stop")
                smoke.verify_restored()
            except BaseException as failure:
                smoke.record("recovery-failed", error=str(failure))
                if error is None:
                    raise
    smoke.record("complete", ok=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--session", type=Path, required=True)
    parser.add_argument("--timeout", type=float, default=240)
    parser.add_argument(
        "--device", choices=("touchpad", "touchscreen", "both"), default="both"
    )
    parser.add_argument(
        "--log-level", default="info,hearthspace_compositor::compositor::input=debug"
    )
    args = parser.parse_args()
    try:
        run(args)
        return 0
    except (
        ControlError,
        OSError,
        ValueError,
        KeyError,
        subprocess.SubprocessError,
        KeyboardInterrupt,
    ) as error:
        print(f"pinch smoke failed: {error}")
        return 1
