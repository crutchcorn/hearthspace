"""Three-contact touchscreen zoom through real kernel touch frames."""

import time
from pathlib import Path

from .control import ControlError
from .input import Input
from .pinch_assertions import same_viewport, verify_touch_removal, verify_zoom
from .touchpad import TouchDevice
from .visual import verify_move


def to_device(points, output):
    return [
        [
            round((x - output["logical_x"]) * 2000 / output["logical_width"]),
            round((y - output["logical_y"]) * 1400 / output["logical_height"]),
        ]
        for x, y in points
    ]


def from_device(points, output):
    return [
        (
            x * output["logical_width"] / 2000 + output["logical_x"],
            y * output["logical_height"] / 1400 + output["logical_y"],
        )
        for x, y in points
    ]


def centroid(points):
    return tuple(sum(point[axis] for point in points) / len(points) for axis in (0, 1))


def scaled(points, ratio):
    center = centroid(points)
    return [
        [center[axis] + (point[axis] - center[axis]) * ratio for axis in (0, 1)]
        for point in points
    ]


def contacts(window, output):
    x, y, width, height, titlebar = (
        window[key] for key in ("x", "y", "width", "height", "titlebar_height")
    )
    # First contact is on the close button: an eager pointer emulation would
    # close the client before the three-contact chord could be recognized.
    return to_device(
        [
            (x + width - titlebar / 2, y + titlebar / 2),
            (x + width * 0.12, y + height * 0.5),
            (x + width * 0.4, y + height * 0.8),
        ],
        output,
    )


def unchanged_window(smoke, before):
    after = smoke.window(before["id"])
    if after is None or any(
        abs(after[key] - before[key]) > 1 for key in ("x", "y", "width", "height")
    ):
        raise ControlError(
            "touch chord clicked/dragged the terminal before zoom recognition"
        )


def begin(smoke, device, points, window, *, stagger=False):
    if stagger:
        device.request("begin", fingers=1, points=points[:1])
        unchanged_window(smoke, window)
        for point in points[1:]:
            device.request("add-finger", point=point)
            unchanged_window(smoke, window)
    else:
        device.request("begin", fingers=len(points), points=points)
        unchanged_window(smoke, window)


def changed(smoke, before):
    viewport = smoke.control.status()["viewport"]
    return (
        viewport
        if viewport["pinch_active"] and not same_viewport(before, viewport)
        else None
    )


def settled(smoke):
    before = smoke.control.status()["viewport"]
    while True:
        time.sleep(min(0.04, smoke.control.deadline.remaining()))
        after = smoke.control.status()["viewport"]
        if same_viewport(before, after):
            return after
        before = after


def zoom(smoke, device, output, ratio, name):
    window = smoke.window()
    points = contacts(window, output)
    anchor = centroid(from_device(points, output))
    with Input(smoke.control) as inputs:
        inputs.motion(output["logical_x"] + 20, output["logical_y"] + 100)
    before = smoke.control.status()["viewport"]
    image = smoke.capture(f"{name}-before")
    begin(smoke, device, points, window, stagger=True)
    moved = scaled(points, ratio)
    device.request("move", points=moved)
    smoke.until(name, lambda: changed(smoke, before))
    after = settled(smoke)
    actual_ratio = after["scale"] / before["scale"]
    if not (actual_ratio < 0.85 if ratio < 1 else actual_ratio > 1.15):
        raise ControlError("touchscreen pinch did not zoom in the requested direction")
    after_image = smoke.capture(f"{name}-active")
    after_window = smoke.window()
    assertions = verify_zoom(
        before, after, anchor, window, after_window, image, after_image
    )
    device.request("lift")
    smoke.until(
        "first contact lift ends touchscreen pinch",
        lambda: not smoke.control.status()["viewport"]["pinch_active"],
    )
    device.request("move", points=scaled(moved[:2], 1.05))
    device.request("end")
    final = settled(smoke)
    if final["pinch_active"] or not same_viewport(after, final):
        raise ControlError("remaining contacts changed a completed touchscreen pinch")
    unchanged_window(smoke, after_window)
    smoke.record(name, before=before, after=final, anchor=anchor, **assertions)


def single_drag(smoke, device, output):
    window = smoke.window()
    start = (window["x"] + 60, window["y"] + window["titlebar_height"] / 2)
    points = to_device([start], output)
    image = smoke.capture("touchscreen-single-drag-before")
    begin(smoke, device, points, window)
    device.request("move", points=to_device([(start[0] + 80, start[1] + 30)], output))
    device.request("end")
    after = smoke.until(
        "single-contact window drag", lambda: smoke.changed(window, ("x", "y"))
    )
    if after["x"] - window["x"] < 40:
        raise ControlError("single-contact drag did not move the window far enough")
    captured = smoke.capture("touchscreen-single-drag-after")
    bounds = verify_move(image, captured, window, after)
    smoke.record(
        "touchscreen-single-drag", before=window, after=after, marker_bounds=bounds
    )


def tap_close(smoke, output):
    window = smoke.window()
    point = contacts(window, output)[0]
    with (smoke.artifacts / "libinput-touchscreen-tap.log").open("w") as log:
        with TouchDevice(smoke.directory, log, kind="touchscreen") as device:
            begin(smoke, device, [point], window)
            smoke.capture("touchscreen-tap-held")
            unchanged_window(smoke, window)
            device.request("end")
            smoke.until(
                "single-contact tap closes on release",
                lambda: smoke.window(window["id"]) is None,
            )
            smoke.capture("touchscreen-tap-closed")
        if Path(device.device["device"]).exists():
            raise ControlError("tap fixture touchscreen survived its worker")
    smoke.record(
        "touchscreen-single-tap", window_id=window["id"], closed_on_release=True
    )


def acceptance(smoke, window):
    output = smoke.control.status()["outputs"][0]
    with (smoke.artifacts / "libinput-touchscreen.log").open("w") as log:
        with TouchDevice(smoke.directory, log, kind="touchscreen") as device:
            smoke.record("virtual-touchscreen", **device.device)
            zoom(smoke, device, output, 0.7, "touchscreen-pinch-out")
            zoom(smoke, device, output, 1.3, "touchscreen-pinch-in")
            before = smoke.control.status()["viewport"]
            window = smoke.window()
            points = contacts(window, output)[:2]
            begin(smoke, device, points, window, stagger=True)
            device.request("move", points=scaled(points, 0.8))
            device.request("end")
            ignored = settled(smoke)
            if ignored["pinch_active"] or not same_viewport(before, ignored):
                raise ControlError("two touchscreen contacts changed the canvas")
            unchanged_window(smoke, window)
            smoke.record("touchscreen-two-finger-ignored", before=before, after=ignored)
            points = contacts(window, output)
            begin(smoke, device, points, window)
            moved = scaled(points, 0.8)
            device.request("move", points=moved)
            smoke.until(
                "touchscreen pinch before fourth contact",
                lambda: changed(smoke, before),
            )
            smoke.capture("touchscreen-cancel-active")
            fourth = centroid(moved)
            device.request("add-finger", point=fourth)
            smoke.until(
                "fourth contact restores the three-finger canvas transform",
                lambda: same_viewport(before, smoke.control.status()["viewport"]),
            )
            device.request("move", points=scaled([*moved, fourth], 1.05))
            device.request("end")
            cancelled = settled(smoke)
            if cancelled["pinch_active"] or not same_viewport(before, cancelled):
                raise ControlError(
                    "fourth touchscreen contact did not restore the starting view"
                )
            if smoke.control.status()["window_switcher"]["active"]:
                raise ControlError("subthreshold fourth-contact motion opened switcher")
            unchanged_window(smoke, window)
            smoke.capture("touchscreen-cancel-restored")
            smoke.record(
                "touchscreen-fourth-contact-cancelled", before=before, after=cancelled
            )
            single_drag(smoke, device, output)
            window = smoke.window()
            points = contacts(window, output)
            moved = scaled(points, 0.8)
            begin(smoke, device, points, window)
            device.request("move", points=moved)
            smoke.until(
                "touchscreen pinch before unplug", lambda: changed(smoke, before)
            )
            active = settled(smoke)
            smoke.capture("touchscreen-removal-active")
            smoke.record("touchscreen-removal-requested", before=before, active=active)
        smoke.until(
            "touchscreen inactive after unplug",
            lambda: not smoke.control.status()["viewport"]["pinch_active"],
        )
        after = smoke.control.status()["viewport"]
        smoke.capture("touchscreen-removal-after")
        smoke.record(
            "touchscreen-removal-observed", before=before, active=active, after=after
        )
        trace = (smoke.artifacts / "libinput-touchscreen.log").read_text()
        evidence = verify_touch_removal(trace, before, active, after)
        if Path(device.device["device"]).exists():
            raise ControlError("virtual touchscreen survived its worker")
        if any(
            kind not in trace
            for kind in ("TOUCH_DOWN", "TOUCH_MOTION", "TOUCH_UP", "TOUCH_FRAME")
        ):
            raise ControlError(
                "touchscreen trace did not exercise the raw touch frame path"
            )
        smoke.record(
            "touchscreen-removal-verified", after=after, device_removed=True, **evidence
        )
    tap_close(smoke, output)
