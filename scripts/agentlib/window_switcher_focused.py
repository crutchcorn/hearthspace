"""Real focused-view gesture cancellation, commit and blocked canvas movement."""

import copy
from pathlib import Path

from . import pan_input
from . import window_switcher_smoke_checks as checks
from .control import ControlError
from .input import Input
from .touchpad import TouchDevice
from .window_preview_assertions import marker_count, preserved
from .window_switcher_smoke_assertions import (
    committed,
    input_proof,
    intermediate_motion,
    scene,
    selection,
    window_rects,
)


def window(status, identity):
    return next(item for item in status["windows"] if item["id"] == identity)


def retained(before, after, identity):
    preserved(before, after)
    item = window(after, identity)
    if not (item["focused"] and item["focused_view"] and item["maximized"]):
        raise ControlError("held/cancelled switcher left the focused view")
    if item["fullscreen"]:
        raise ControlError("focused switcher changed protocol fullscreen")


def switched(before, after, original, target):
    item = window(after, original)
    if item["focused_view"] or not item["maximized"] or item["fullscreen"]:
        raise ControlError("committed switcher did not leave focused view bounded")
    # Only the previous focused window changes its buffer/presentation on exit.
    # Canonical homes, other clients, zoom and final centering remain protected
    # by the same assertions used for ordinary switching.
    normalized = copy.deepcopy(before)
    old = window(normalized, original)
    for key in ("content_width", "content_height", "presentation_output"):
        old[key] = item[key]
    committed(normalized, after, target)


def enter(smoke):
    checks.prepare(smoke)
    identity = list(smoke.targets)[1]
    with Input(smoke.control) as inputs:
        inputs.chord("super+shift+f")

    def ready():
        status = checks.settled(smoke)
        item = window(status, identity)
        output = smoke.output
        return (
            status
            if (
                item["focused_view"]
                and item["maximized"]
                and not item["fullscreen"]
                and item["titlebar_height"] > 0
                and all(
                    abs(item[key] - output[f"logical_{key}"]) <= 1
                    for key in ("x", "y", "width", "height")
                )
                and abs(
                    item["content_height"]
                    + item["titlebar_height"]
                    - output["logical_height"]
                )
                <= 1
            )
            else None
        )

    before = smoke.until("focused view client fills output with its header", ready)
    selection(before, identity, active=False)
    return before


def visible(smoke, name, status, identity):
    item = window(status, identity)
    image = smoke.capture(name)
    bounds = (
        item["x"],
        item["y"],
        item["x"] + item["width"],
        item["y"] + item["height"],
    )
    count = marker_count(image, bounds, smoke.targets[identity])
    if not item["visible"] or count < 100:
        raise ControlError("focused switcher outcome lacks the visible client marker")
    return count


def blocked_pan(smoke, kind, before, identity):
    path = smoke.artifacts / f"libinput-{kind}-focused-blocked-pan.log"
    case = {"delta": (-64, 0)}
    with (
        path.open("w") as log,
        TouchDevice(
            smoke.directory, log, kind, deadline=smoke.control.deadline
        ) as device,
    ):
        state = pan_input.begin(smoke, device, case, window(before, identity))
        samples = checks.observe(
            smoke, lambda: pan_input.move(smoke, device, case, state), active=False
        )
        device.request("end")
        after = checks.settled(smoke)
        for sample in [*samples, after]:
            retained(before, sample, identity)
            selection(sample, identity, active=False)
    if Path(device.device["device"]).exists():
        raise ControlError("focused pan input device survived cleanup")
    proof = pan_input.verify_trace(path, kind, case, state)
    smoke.record(
        "focused-canvas-pan-blocked", device=kind, trace=str(path), input=proof
    )


def focused(smoke, kind, fingers):
    original, target = list(smoke.targets)[1:]
    before = enter(smoke)
    if fingers == 4:
        blocked_pan(smoke, kind, before, original)
    visible(smoke, f"{kind}-{fingers}-focused-before", before, original)
    for cancel in (True, False):
        name = f"{kind}-{fingers}-focused-{'cancel' if cancel else 'commit'}"
        path = smoke.artifacts / f"libinput-{name}.log"
        with (
            path.open("w") as log,
            TouchDevice(
                smoke.directory, log, kind, deadline=smoke.control.deadline
            ) as device,
        ):
            move = checks.begin_swipe(smoke, device, kind, fingers, -1)
            samples = checks.observe(smoke, move)
            held = checks.held(smoke, name, target, before, samples, source="gesture")
            for sample in [*samples, held]:
                retained(before, sample, original)
            if cancel:
                with Input(smoke.control) as inputs:
                    checks.observe(smoke, lambda: inputs.chord("escape"), active=False)
                cancelled = checks.settled(smoke)
                retained(before, cancelled, original)
                selection(cancelled, original, active=False)
                device.request("end")
                after = checks.settled(smoke)
                retained(before, after, original)
                selection(after, original, active=False)
                pixels = visible(smoke, f"{name}-released", after, original)
                smoke.record(
                    "focused-switcher-cancelled",
                    name=name,
                    status=after,
                    marker_pixels=pixels,
                )
                before = after
            else:
                samples = checks.observe(
                    smoke, lambda: device.request("end"), active=False
                )
                after = checks.settled(smoke)
                switched(held, after, original, target)
                motion = intermediate_motion(scene(held), samples, window_rects(after))
                pixels = visible(smoke, f"{name}-released", after, target)
                smoke.record(
                    "focused-switcher-committed",
                    name=name,
                    status=after,
                    marker_pixels=pixels,
                    **motion,
                )
        if Path(device.device["device"]).exists():
            raise ControlError("focused switcher input device survived cleanup")
        smoke.record(
            "focused-switcher-input-proof",
            name=name,
            trace=str(path),
            device_removed=True,
            **input_proof(path, kind, fingers, -1),
        )
