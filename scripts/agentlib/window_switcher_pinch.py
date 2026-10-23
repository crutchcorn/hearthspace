"""Persistent native switcher pinches with independent input and pixel evidence."""

from pathlib import Path

from . import window_switcher_focused as focused_checks
from . import window_switcher_pinch_input as pinch_input
from .control import ControlError
from .input import Input
from .touchpad import TouchDevice
from .window_preview_assertions import preserved
from .window_switcher_smoke_assertions import (
    committed,
    intermediate_motion,
    previews,
    scene,
    selection,
    window_rects,
)
from .window_switcher_smoke_checks import observe, prepare, settled


def unchanged_focus(before, after, *, active, target, source):
    """A pinch may change visibility, but never focus, layout or held selection."""
    preserved(before, after)
    focused = next(window["id"] for window in before["windows"] if window["focused"])
    selection(after, target, active=active, focused=focused)
    switcher = after["window_switcher"]
    if active and switcher["source"] != source:
        raise ControlError("pinch changed the switcher's input owner")
    if active and before["window_switcher"]["active"]:
        for key in ("window_ids", "output"):
            if switcher[key] != before["window_switcher"][key]:
                raise ControlError("pinch changed the open switcher's order or output")


def gesture(
    smoke, kind, fingers, *, outward=False, small=False, cancel=False, label=""
):
    before = settled(smoke)
    initial = before["window_switcher"]
    keyboard_owned = initial["active"] and initial["source"] == "keyboard"
    expected = initial["active"] if small or cancel or keyboard_owned else not outward
    target = (
        initial["selected_window_id"] if initial["active"] else list(smoke.targets)[-1]
    )
    source = initial["source"] if initial["active"] else "gesture"
    name = f"{kind}-{fingers}-pinch-" + ("out" if outward else "in")
    if keyboard_owned:
        name += "-keyboard-owner"
    elif initial["active"] and not outward:
        name += "-already-open"
    if small:
        name += "-subthreshold"
    if cancel:
        name += "-cancelled"
    if label:
        name += f"-{label}"
    smoke.capture(f"{name}-before")
    path = smoke.artifacts / f"libinput-{name}.log"
    # Clear libinput's motion threshold without crossing the compositor's
    # 0.8/1.25 thresholds in the subthreshold cases.
    ratio = (1.24 if outward else 0.85) if small else (1.5 if outward else 0.6)
    with (
        path.open("w") as log,
        TouchDevice(
            smoke.directory, log, kind, deadline=smoke.control.deadline
        ) as device,
    ):
        smoke.record("virtual-switcher-pinch-device", name=name, **device.device)
        state = pinch_input.begin(
            device, fingers, outward=outward, stagger=kind == "touchscreen"
        )
        samples = observe(
            smoke,
            lambda: pinch_input.move(device, state, ratio),
            active=initial["active"],
        )
        for sample in samples:
            unchanged_focus(
                before, sample, active=initial["active"], target=target, source=source
            )
        smoke.capture(f"{name}-held")
        if cancel:
            pinch_input.cancel(
                device, state, trace_path=path, deadline=smoke.control.deadline
            )
        device.request("end")
        smoke.until(
            "switcher pinch completed and previews settled",
            lambda: (
                not (status := smoke.control.status())["viewport"]["pinch_active"]
                and status["window_switcher"]["active"] == expected
                and not status["window_switcher"]["animating"]
            ),
        )
        after = settled(smoke)
        unchanged_focus(before, after, active=expected, target=target, source=source)
        image = smoke.capture(f"{name}-after-lift")
        proof = previews(after, image, smoke.targets) if expected else {}
        smoke.record(
            "switcher-pinch-verified",
            name=name,
            before=initial,
            after=after["window_switcher"],
            previews=proof,
        )
    if Path(device.device["device"]).exists():
        raise ControlError("switcher pinch input device survived cleanup")
    # Persistence must survive the completed device sequence and device cleanup.
    unchanged_focus(
        before, settled(smoke), active=expected, target=target, source=source
    )
    smoke.record(
        "switcher-pinch-input-evidence",
        name=name,
        trace=str(path),
        device_removed=True,
        **pinch_input.trace_proof(path, kind, fingers, cancelled=cancel, ratio=ratio),
    )


def stages(smoke, kind, fingers):
    smoke.step(f"{kind}-{fingers}-pinch-prepare", prepare)
    for label, options in (
        ("in-subthreshold", {"small": True}),
        *([("in-cancelled", {"cancel": True})] if fingers == 4 else []),
        ("in-open", {}),
        ("in-repeat", {}),
        ("out-subthreshold", {"outward": True, "small": True}),
        ("out-dismiss", {"outward": True}),
    ):
        smoke.step(
            f"{kind}-{fingers}-pinch-{label}",
            lambda smoke, options=options: gesture(smoke, kind, fingers, **options),
        )
    smoke.step(
        f"{kind}-{fingers}-pinch-keyboard-owner",
        lambda smoke: keyboard_owner(smoke, kind, fingers),
    )
    smoke.step(
        f"{kind}-{fingers}-pinch-focused-view",
        lambda smoke: focused(smoke, kind, fingers),
    )


def keyboard_owner(smoke, kind, fingers):
    prepare(smoke)
    middle, last = list(smoke.targets)[1:]
    with Input(smoke.control) as inputs:
        inputs.down("key", 56)
        inputs.chord("tab")
        held = settled(smoke)
        selection(held, last, active=True, focused=middle)
        if held["window_switcher"]["source"] != "keyboard":
            raise ControlError("Alt+Tab did not own the switcher before pinch")
        gesture(smoke, kind, fingers)
        gesture(smoke, kind, fingers, outward=True)
        inputs.up("key", 56)
    after = settled(smoke)
    committed(held, after, last)
    smoke.record("switcher-pinch-keyboard-owner-verified", device=kind, fingers=fingers)


def focused(smoke, kind, fingers):
    original, target = list(smoke.targets)[1:]
    before = focused_checks.enter(smoke)
    name = f"{kind}-{fingers}-pinch-focused"
    focused_checks.visible(smoke, f"{name}-before", before, original)
    gesture(smoke, kind, fingers, label="focused-open")
    held = settled(smoke)
    focused_checks.retained(before, held, original)
    gesture(smoke, kind, fingers, outward=True, label="focused-dismiss")
    cancelled = settled(smoke)
    focused_checks.retained(before, cancelled, original)
    selection(cancelled, original, active=False)
    pixels = focused_checks.visible(smoke, f"{name}-cancelled", cancelled, original)
    smoke.record(
        "focused-switcher-pinch-cancelled",
        name=name,
        status=cancelled,
        marker_pixels=pixels,
    )
    gesture(smoke, kind, fingers, label="focused-reopen")
    held = settled(smoke)
    focused_checks.retained(before, held, original)
    with Input(smoke.control) as inputs:
        samples = observe(smoke, lambda: inputs.chord("enter"), active=False)
    after = settled(smoke)
    focused_checks.switched(held, after, original, target)
    motion = intermediate_motion(scene(held), samples, window_rects(after))
    pixels = focused_checks.visible(smoke, f"{name}-committed", after, target)
    smoke.record(
        "focused-switcher-pinch-committed",
        name=name,
        status=after,
        marker_pixels=pixels,
        **motion,
    )
