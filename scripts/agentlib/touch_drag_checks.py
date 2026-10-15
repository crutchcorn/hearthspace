"""One-contact gestures through uinput/libinput with held-contact checkpoints."""

import time
from pathlib import Path

from . import firefox_assertions, firefox_checks
from . import touch_drag_assertions as verify
from .control import ControlError
from .input import Input
from .pinch_touchscreen import from_device, settled, to_device
from .touchpad import TouchDevice
from .visual import marker_bounds


def prepare(smoke, *, zoom, firefox):
    smoke.control.request("focus-launcher")
    smoke.control.request("reset-view")
    settled(smoke)
    smoke.control.request(f"focus-window {smoke.target}")
    if firefox and firefox_assertions.maximized(smoke.window(smoke.target)):
        firefox_checks.activate(smoke, "Restore Down")
    else:
        smoke.control.request(f"restore-window {smoke.target}")
    smoke.until(
        "restored window for direct-touch move",
        lambda: firefox_assertions.restored(smoke.window(smoke.target)),
    )
    width = min(900, smoke.output["logical_width"] - 260)
    height = min(520, smoke.output["logical_height"] - 250)
    if width < 700 or height < 400:
        raise ControlError(
            "touch drag fixture needs an output at least 960 by 650 logical"
        )
    smoke.control.request(f"resize-window {smoke.target} {width} {height}")
    # Foot commits complete character cells, so its client size can round down
    # by one cell. Firefox's pixel-sized fixture must match the configure exactly.
    tolerance = 0 if firefox else 32
    smoke.until(
        "committed fixture size",
        lambda: (
            (window := smoke.window(smoke.target))
            and 0 <= width - window["content_width"] <= tolerance
            and 0 <= height - window["content_height"] <= tolerance
        ),
    )
    smoke.control.request("focus-launcher")
    smoke.control.request("reset-view")
    settled(smoke)
    smoke.control.request(
        f"move-window {smoke.target} {smoke.output['logical_x'] + 100} "
        f"{smoke.output['logical_y'] + 120}"
    )
    smoke.control.request(f"focus-window {smoke.target}")
    if zoom:
        smoke.control.request("zoom-out")
        smoke.until(
            "nonunit canvas scale",
            lambda: smoke.control.status()["viewport"]["scale"] < 0.9,
        )
    view = settled(smoke)
    if not zoom and abs(view["scale"] - 1) > 1e-8:
        raise ControlError("unit-scale touch drag setup did not reset zoom")
    window = smoke.window(smoke.target)
    if firefox and window["titlebar_height"] != 0:
        raise ControlError("Firefox fixture must use its client-side titlebar")
    if not firefox and window["titlebar_height"] <= 0:
        raise ControlError("terminal fixture must have compositor decorations")
    return window, view


def point(smoke, logical):
    output = smoke.output
    if not (
        output["logical_x"]
        <= logical[0]
        < output["logical_x"] + output["logical_width"]
        and output["logical_y"]
        <= logical[1]
        < output["logical_y"] + output["logical_height"]
    ):
        raise ControlError("touchscreen gesture point lies outside its output")
    device = to_device([logical], output)
    return device, from_device(device, output)[0]


def released(smoke, before, name):
    # A real post-lift pointer motion must not continue the prior touch grab.
    with Input(smoke.control) as inputs:
        inputs.motion(smoke.output["logical_x"] + 25, smoke.output["logical_y"] + 90)
    time.sleep(min(0.06, smoke.control.deadline.remaining()))
    after = smoke.window(smoke.target)
    verify.unchanged(before, after)
    image = smoke.capture(f"{name}-released")
    marker_bounds(image, after)
    smoke.record("touch-release-verified", name=name, window=after)


def drag(smoke, device, name, delta, *, firefox, ending="release"):
    before = smoke.window(smoke.target)
    view = settled(smoke)
    # The owned one-tab Firefox profile has empty draggable tab-strip space at
    # 60% width. Use client-logical height scaled by canvas zoom, not output DPI.
    title_y = 22 * view["scale"] if firefox else before["titlebar_height"] / 2
    start = (before["x"] + before["width"] * 0.6, before["y"] + title_y)
    start_device, actual_start = point(smoke, start)
    image = smoke.capture(f"{name}-before")
    marker_bounds(image, before)
    smoke.record(
        "touch-drag-target", name=name, point=start, before=before, viewport=view
    )
    device.request("begin", fingers=1, points=start_device)
    completed = False
    try:
        for phase, fraction in (("active", 0.5), ("continued", 1)):
            destination = tuple(start[axis] + delta[axis] * fraction for axis in (0, 1))
            device_point, actual = point(smoke, destination)
            expected = tuple(actual[axis] - actual_start[axis] for axis in (0, 1))
            device.request("move", points=device_point)

            def follows_contact(expected=expected):
                after = smoke.window(smoke.target)
                try:
                    verify.moved(before, after, expected, view["scale"])
                except ControlError:
                    return None
                return after

            after = smoke.until(f"{name} {phase} follows contact", follows_contact)
            captured = smoke.capture(f"{name}-{phase}")
            proof = verify.move_pixels(before, after, image, captured)
            verify.viewport_unchanged(view, smoke.control.status()["viewport"])
            smoke.record(
                "touch-drag-verified",
                name=name,
                phase=phase,
                before=before,
                after=after,
                expected_delta=expected,
                **proof,
            )
        if ending == "second-contact":
            extra = (destination[0] + 30, destination[1] + 100)
            extra_device, _ = point(smoke, extra)
            device.request("add-finger", point=extra_device[0])
            moved = [(x + 25, y + 15) for x, y in (destination, extra)]
            device.request("move", points=[point(smoke, item)[0][0] for item in moved])
            verify.unchanged(after, smoke.window(smoke.target))
            verify.viewport_unchanged(view, smoke.control.status()["viewport"])
            smoke.capture(f"{name}-second-contact-released")
            smoke.record("second-contact-released-grab", name=name, window=after)
        completed = True
    finally:
        if ending != "unplug" or not completed:
            device.request("end")
    if ending != "unplug":
        released(smoke, after, name)
    return after


def resize(smoke, device, name):
    before = smoke.window(smoke.target)
    view = settled(smoke)
    start = (
        before["x"] + before["width"] - view["scale"],
        before["y"] + before["height"] / 2,
    )
    points, actual_start = point(smoke, start)
    image = smoke.capture(f"{name}-before")
    device.request("begin", fingers=1, points=points)
    try:
        for amount in (35, 80):
            destination, actual = point(smoke, (start[0] - amount, start[1]))
            device.request("move", points=destination)
            requested = actual_start[0] - actual[0]

            def follows_contact(requested=requested):
                after = smoke.window(smoke.target)
                try:
                    verify.resized(before, after, requested, view["scale"])
                except ControlError:
                    return None
                return after

            after = smoke.until(f"{name} resize by {amount}", follows_contact)
        captured = smoke.capture(f"{name}-active")
        proof = verify.resize_pixels(before, after, image, captured)
        verify.viewport_unchanged(view, smoke.control.status()["viewport"])
        smoke.record(
            "touch-resize-verified", name=name, before=before, after=after, **proof
        )
    finally:
        device.request("end")
    released(smoke, after, name)


def run(smoke, name, *, zoom, firefox):
    prepare(smoke, zoom=zoom, firefox=firefox)
    path = smoke.artifacts / f"libinput-{name}.log"
    with (
        path.open("w") as log,
        TouchDevice(
            smoke.directory, log, "touchscreen", deadline=smoke.control.deadline
        ) as device,
    ):
        smoke.record("virtual-touchscreen", name=name, **device.device)
        drag(smoke, device, f"{name}-first", (80, 40), firefox=firefox)
        drag(smoke, device, f"{name}-repeat", (-60, -25), firefox=firefox)
        if firefox:
            resize(smoke, device, f"{name}-resize")
            drag(
                smoke,
                device,
                f"{name}-second-contact",
                (35, 20),
                firefox=True,
                ending="second-contact",
            )
            dropped = drag(
                smoke,
                device,
                f"{name}-unplug",
                (-25, -15),
                firefox=True,
                ending="unplug",
            )
    if firefox:
        released(smoke, dropped, f"{name}-unplug")
    if Path(device.device["device"]).exists():
        raise ControlError("owned touchscreen device survived cleanup")
    trace = path.read_text()
    if any(
        event not in trace
        for event in ("TOUCH_DOWN", "TOUCH_MOTION", "TOUCH_UP", "TOUCH_FRAME")
    ):
        raise ControlError("touch drag trace is missing real touchscreen events")
    smoke.record("virtual-touchscreen-removed", name=name, trace=str(path))
