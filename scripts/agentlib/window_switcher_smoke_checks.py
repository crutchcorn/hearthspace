"""Owned native switcher actions with continuous animation observations."""

import time
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

from . import overview_checks
from .control import ControlError
from .gesture_trace import gesture
from .input import Input
from .overview_assertions import marker_count, preserved
from .pinch_assertions import same_viewport
from .pinch_touchscreen import to_device
from .touchpad import TouchDevice, checked_points, contact_positions
from .window_switcher_smoke_assertions import (
    held_motion,
    input_proof,
    previews,
    released_motion,
    selection,
)


def settled(smoke):
    def ready():
        status = smoke.control.status()
        return (
            status
            if not (
                status["viewport"]["animating"]
                or status["window_switcher"]["animating"]
            )
            else None
        )

    return smoke.until("window switcher preview settled", ready)


def fixtures(smoke):
    overview_checks.fixtures(smoke)
    for target, position in zip(smoke.targets, ((-5000, -3000), (0, 0), (5000, 3000))):
        smoke.control.request(f"restore-window {target}")
        smoke.control.request(f"move-window {target} {position[0]} {position[1]}")
    smoke.until(
        "all switcher fixture clients restored",
        lambda: all(
            not item["fullscreen"] and not item["maximized"]
            for item in smoke.control.status()["windows"]
        ),
    )
    prepare(smoke)
    smoke.capture("switcher-fixtures-arranged")


def prepare(smoke):
    settled(smoke)
    first, target, last = smoke.targets
    # Re-establish a known MRU stack after earlier commits and fixture actions.
    for identity in (first, last, target):
        smoke.control.request(f"focus-window {identity}")
        settled(smoke)
    before = settled(smoke)
    selection(before, target, active=False)
    return before


def observe(smoke, operation, *, active=True):
    """Observe live preview motion even while a device move blocks for 400 ms."""
    samples = []
    with ThreadPoolExecutor(max_workers=1) as pool:
        pending = pool.submit(operation)
        while True:
            input_finished = pending.done()
            if input_finished:
                pending.result()
            samples.append(smoke.control.status())
            if (
                input_finished
                and samples[-1]["window_switcher"]["active"] == active
                and not (
                    samples[-1]["viewport"]["animating"]
                    or samples[-1]["window_switcher"]["animating"]
                )
            ):
                return samples
            time.sleep(min(0.005, smoke.control.deadline.remaining()))


def held(smoke, name, target, before, samples, *, source="keyboard"):
    status = settled(smoke)
    original = next(item["id"] for item in before["windows"] if item["focused"])
    selection(status, target, active=True, focused=original)
    if status["window_switcher"]["source"] != source:
        raise ControlError("switcher is owned by the wrong input source")
    image = smoke.capture(f"{name}-held")
    proof = previews(status, image, smoke.targets)
    motion = held_motion(before, samples, status)
    smoke.record(
        "switcher-held-verified", name=name, status=status, previews=proof, **motion
    )
    return status


def released(smoke, name, target, before, samples):
    status = settled(smoke)
    motion = released_motion(before, samples, status, target)
    window = next(item for item in status["windows"] if item["id"] == target)
    if not window["visible"]:
        raise ControlError("switcher did not reveal the selected client")
    image = smoke.capture(f"{name}-released")
    bounds = (
        window["x"],
        window["y"],
        window["x"] + window["width"],
        window["y"] + window["height"],
    )
    if marker_count(image, bounds, smoke.targets[target]) < 100:
        raise ControlError(
            "released switcher did not present the selected client's marker"
        )
    smoke.record(
        "switcher-release-verified", name=name, target=target, status=status, **motion
    )


def keyboard(smoke):
    before = prepare(smoke)
    first, middle, last = smoke.targets
    smoke.capture("keyboard-before")
    with Input(smoke.control) as inputs:
        inputs.down("key", 56)
        samples = observe(smoke, lambda: inputs.chord("tab"))
        held(smoke, "keyboard-forward", last, before, samples)
        before_reverse = smoke.control.status()
        samples = observe(smoke, lambda: inputs.chord("shift+tab"))
        held(smoke, "keyboard-reverse", middle, before_reverse, samples)
        inputs.chord("shift+tab")
        final_held = settled(smoke)
        selection(final_held, first, active=True, focused=middle)
        release_samples = observe(smoke, lambda: inputs.up("key", 56), active=False)
    released(smoke, "keyboard", first, final_held, release_samples)
    before = prepare(smoke)
    with Input(smoke.control) as inputs:
        inputs.down("key", 56)
        inputs.chord("tab")
        settled(smoke)
        inputs.chord("escape")
        after = settled(smoke)
        selection(after, middle, active=False)
        preserved(before, after)
    after = settled(smoke)
    selection(after, middle, active=False)
    preserved(before, after)
    smoke.capture("keyboard-cancelled")
    smoke.record("switcher-keyboard-cancel-verified", status=after)


def swipe(smoke, kind, fingers, direction):
    name = f"{kind}-{fingers}-{'left' if direction < 0 else 'right'}"
    before = prepare(smoke)
    target = list(smoke.targets)[2 if direction < 0 else 0]
    smoke.capture(f"{name}-before")
    path = smoke.artifacts / f"libinput-{name}.log"
    with (
        path.open("w") as log,
        TouchDevice(
            smoke.directory, log, kind, deadline=smoke.control.deadline
        ) as device,
    ):
        smoke.record("virtual-switcher-device", name=name, **device.device)
        if kind == "touchpad":
            device.request("begin", fingers=fingers, distance=700)

            def move():
                device.request("move", distance=700, translation=[160 * direction, 0])
        else:
            points = checked_points(contact_positions(700, fingers))
            device.request("begin", fingers=fingers, points=points)
            moved = checked_points(
                [
                    (x + 64 * direction * 2000 / smoke.output["logical_width"], y)
                    for x, y in points
                ]
            )

            def move():
                device.request("move", points=moved)

        samples = observe(smoke, move)
        final_held = held(smoke, name, target, before, samples, source="gesture")
        release_samples = observe(smoke, lambda: device.request("end"), active=False)
        smoke.until(
            "fingers released switcher",
            lambda: not smoke.control.status()["window_switcher"]["active"],
        )
        released(smoke, name, target, final_held, release_samples)
    if Path(device.device["device"]).exists():
        raise ControlError("window switcher input device survived cleanup")
    smoke.record(
        "switcher-input-proof",
        name=name,
        trace=str(path),
        device_removed=True,
        **input_proof(path, kind, fingers, direction),
    )


def ownership(smoke, kind):
    before = prepare(smoke)
    middle, last = list(smoke.targets)[1:]
    path = smoke.artifacts / f"libinput-{kind}-ownership.log"
    with (
        path.open("w") as log,
        TouchDevice(
            smoke.directory, log, kind, deadline=smoke.control.deadline
        ) as device,
        Input(smoke.control) as inputs,
    ):
        points = checked_points(contact_positions(700, 4))

        def begin():
            if kind == "touchpad":
                device.request("begin", fingers=4, distance=700)
            else:
                device.request("begin", fingers=4, points=points)

        def move(multiplier=1):
            if kind == "touchpad":
                device.request("move", distance=700, translation=[-160 * multiplier, 0])
            else:
                device.request(
                    "move",
                    points=checked_points(
                        [
                            (
                                x
                                - 64
                                * multiplier
                                * 2000
                                / smoke.output["logical_width"],
                                y,
                            )
                            for x, y in points
                        ]
                    ),
                )

        inputs.down("key", 56)
        inputs.chord("tab")
        keyboard_view = settled(smoke)
        selection(keyboard_view, last, active=True, focused=middle)
        if kind == "touchscreen":
            # Hold over the *restored* close button, then lift after the reveal.
            # Preview geometry is smaller and would not prove a late client tap.
            window = next(item for item in before["windows"] if item["id"] == last)
            close = (
                smoke.output["logical_x"]
                + (
                    smoke.output["logical_width"]
                    + window["width"]
                    - window["titlebar_height"]
                )
                / 2,
                smoke.output["logical_y"]
                + (
                    smoke.output["logical_height"]
                    - window["height"]
                    + window["titlebar_height"]
                )
                / 2,
            )
            device.request("begin", fingers=1, points=to_device([close], smoke.output))
            inputs.up("key", 56)
            settled(smoke)
            device.request("end")
            smoke.capture("touchscreen-close-tap-suppressed-after-alt-release")
            after_tap = smoke.control.status()
            selection(after_tap, last, active=False)
            if set(smoke.targets) != {window["id"] for window in after_tap["windows"]}:
                raise ControlError(
                    "touch held through Alt release clicked a client's close button"
                )
            smoke.record(
                "switcher-late-touch-release-suppressed",
                target=last,
                point=close,
                status=after_tap,
            )
            prepare(smoke)
            inputs.down("key", 56)
            inputs.chord("tab")
            keyboard_view = settled(smoke)
        begin()
        move()
        device.request("end")
        keyboard_after = settled(smoke)
        selection(keyboard_after, last, active=True, focused=middle)
        if keyboard_after["window_switcher"]["source"] != "keyboard":
            raise ControlError("swipe stole the Alt-held overlay")
        if not same_viewport(keyboard_view["viewport"], keyboard_after["viewport"]):
            raise ControlError("ignored swipe moved the keyboard-owned viewport")
        held_motion(keyboard_view, [keyboard_after], keyboard_after)
        image = smoke.capture(f"{kind}-keyboard-owner-held")
        previews(keyboard_after, image, smoke.targets)
        inputs.up("key", 56)
        prepare(smoke)
        begin()
        move()
        gesture_view = settled(smoke)
        selection(gesture_view, last, active=True, focused=middle)
        inputs.chord("alt+tab")
        gesture_after = settled(smoke)
        selection(gesture_after, last, active=True, focused=middle)
        held_motion(gesture_view, [gesture_after], gesture_after)
        if smoke.control.status()["window_switcher"]["source"] != "gesture":
            raise ControlError("Alt+Tab stole the finger-held overlay")
        inputs.chord("escape")
        cancelled = settled(smoke)
        selection(cancelled, middle, active=False)
        if not same_viewport(before["viewport"], cancelled["viewport"]):
            raise ControlError("Escape did not restore the gesture's starting view")
        move(2)
        selection(smoke.control.status(), middle, active=False)
        device.request("end")
        after_lift = settled(smoke)
        selection(after_lift, middle, active=False)
        preserved(before, after_lift)
        smoke.capture(f"{kind}-cancelled-until-lift")
    if Path(device.device["device"]).exists():
        raise ControlError("ownership fixture device survived cleanup")
    proof = (
        gesture(path.read_text(), "SWIPE", 4, require_end=True)
        if kind == "touchpad"
        else {"raw_touch_frames": True, "contact_sequences": 2}
    )
    smoke.record(
        "switcher-ownership-verified",
        device=kind,
        trace=str(path),
        device_removed=True,
        input=proof,
    )
