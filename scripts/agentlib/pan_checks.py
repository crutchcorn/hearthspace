"""Presented-marker acceptance for translated three-contact native gestures."""

from pathlib import Path

from . import pan_input
from .control import ControlError
from .firefox_assertions import (
    fullscreen_at_output,
    marker_size,
    maximized,
    maximized_at_output,
    unchanged_canvas_geometry,
)
from .pinch_assertions import same_viewport, verify_zoom
from .pinch_touchscreen import settled
from .touchpad import TouchDevice
from .viewport_fixture import set_viewport
from .visual import marker_bounds

CASES = [
    {"name": "right", "delta": (80, 0)},
    {"name": "left", "delta": (-80, 0)},
    {"name": "down", "delta": (0, 60)},
    {"name": "up", "delta": (0, -60)},
    {"name": "diagonal", "delta": (60, 40)},
    {"name": "combined", "delta": (60, 40), "ratio": 0.75},
    {"name": "combined-cancel", "delta": (60, 40), "ratio": 0.75, "cancel": True},
    {"name": "two-finger-ignored", "delta": (40, 20), "ratio": 0.75, "fingers": 2},
    {
        "name": "maximized-pan-cancel",
        "delta": (60, 40),
        "mode": "maximized",
        "cancel": True,
    },
    {
        "name": "maximized-combined",
        "delta": (60, 40),
        "ratio": 0.75,
        "mode": "maximized",
    },
    {"name": "fullscreen-pan", "delta": (60, 40), "mode": "fullscreen"},
]


def prepare(smoke, case):
    smoke.control.request(f"restore-window {smoke.target}")
    set_viewport(smoke, x=0, y=0, scale=1)
    smoke.until(
        "restored terminal client geometry",
        lambda: (
            (window := smoke.window(smoke.target))
            and not window["maximized"]
            and not window["fullscreen"]
            and all(
                window[key] == smoke.floating[key]
                for key in ("content_width", "content_height")
            )
        ),
    )
    smoke.control.request(f"focus-window {smoke.target}")
    settled(smoke)
    if mode := case.get("mode"):
        if mode == "maximized":
            smoke.control.request(f"move-window {smoke.target} 100000 100000")
            command, matches = "maximize-window", maximized_at_output
        else:
            command, matches = "fullscreen-window", fullscreen_at_output
        smoke.control.request(f"{command} {smoke.target} true")
        smoke.until(
            f"terminal configured {mode}",
            lambda: matches(smoke.window(smoke.target), [smoke.output]),
        )
    return smoke.window(smoke.target)


def unchanged(before, after):
    keys = (
        "x",
        "y",
        "width",
        "height",
        "canvas_x",
        "canvas_y",
        "maximized",
        "fullscreen",
    )
    if any(before[key] != after[key] for key in keys):
        raise ControlError(
            "cancelled/ignored gesture changed terminal geometry or state"
        )


def pan_cancelled(before, status):
    # A fourth contact can start a many-finger candidate after rolling back
    # the three-finger transform. That candidate must not open the switcher.
    return (
        same_viewport(before, status["viewport"])
        and not status["window_switcher"]["active"]
    )


def expected_direction(case, observed):
    for requested, actual in zip(case["delta"], observed["translation"]):
        if requested and (actual * requested <= 0 or abs(actual) < 10):
            raise ControlError(
                "observed gesture did not translate in the requested direction"
            )
    if case.get("ratio", 1) == 1 and abs(observed["scale"] - 1) > 1e-9:
        raise ControlError("pure swipe unexpectedly changed scale")


def pinned_marker(mode, image, captured, window, after_window):
    old, new = marker_size(image, window), marker_size(captured, after_window)
    if any(abs(a - b) > 2 for a, b in zip(old, new)):
        raise ControlError("pure pan changed the rendered marker's native size")
    if mode == "fullscreen" and marker_bounds(image, window) != marker_bounds(
        captured, after_window
    ):
        raise ControlError("pan moved the fullscreen marker within its fixed window")
    return {"before_marker_size": old, "after_marker_size": new}


def check_active(smoke, case, state, observed, before, window, image, active):
    after_window = smoke.window(smoke.target)
    captured = smoke.capture(f"{state['name']}-active")
    expected_direction(case, observed)
    ratio = active["scale"] / before["scale"]
    if abs(ratio - observed["scale"]) > 0.015:
        raise ControlError("applied scale differs from independent input evidence")
    if case.get("mode") == "fullscreen":
        unchanged(window, after_window)
        proof = pinned_marker("fullscreen", image, captured, window, after_window)
    else:
        if case.get("mode") == "maximized":
            if not maximized(window) or not maximized(after_window):
                raise ControlError("pan did not preserve visible canvas maximization")
            unchanged_canvas_geometry(window, after_window)
        proof = verify_zoom(
            before,
            active,
            state["anchor"],
            window,
            after_window,
            image,
            captured,
            translation=observed["translation"],
        )
    smoke.record(
        "pan-transform-verified",
        name=state["name"],
        before=before,
        after=active,
        before_window=window,
        after_window=after_window,
        input=observed,
        **proof,
    )


def run(smoke, kind, case):
    name = f"{kind}-{case['name']}"
    window = prepare(smoke, case)
    before = smoke.control.status()["viewport"]
    image = smoke.capture(f"{name}-before")
    path = smoke.artifacts / f"libinput-{name}.log"
    with (
        path.open("w") as log,
        TouchDevice(
            smoke.directory, log, kind, deadline=smoke.control.deadline
        ) as device,
    ):
        smoke.record("virtual-pan-device", name=name, **device.device)
        state = {**pan_input.begin(smoke, device, case, window), "name": name}
        pan_input.move(smoke, device, case, state)
        if case.get("fingers", 3) == 2:
            device.request("end")
            after = settled(smoke)
            if after["pinch_active"] or not same_viewport(before, after):
                raise ControlError("two contacts changed the canvas")
            unchanged(window, smoke.window(smoke.target))
            captured = smoke.capture(f"{name}-after")
            if marker_bounds(image, window) != marker_bounds(
                captured, smoke.window(smoke.target)
            ):
                raise ControlError("ignored two-contact gesture moved presented marker")
            smoke.record("two-contact-ignored", name=name, before=before, after=after)
        else:
            smoke.until(
                "active translated gesture",
                lambda: (
                    (view := smoke.control.status()["viewport"])["pinch_active"]
                    and not same_viewport(before, view)
                ),
            )
            active = settled(smoke)
            # Capture before reading the independent reader, giving its final
            # already-injected update time to flush without guessing a sleep.
            smoke.capture(f"{name}-input-complete")
            observed = pan_input.observed(smoke, device, case, state, path)
            check_active(smoke, case, state, observed, before, window, image, active)
            if case.get("cancel"):
                pan_input.cancel(device, state)

                smoke.until(
                    "fourth contact restores the three-finger canvas transform",
                    lambda: pan_cancelled(before, smoke.control.status()),
                )
            device.request("end")
            after = settled(smoke)
            expected = before if case.get("cancel") else active
            if after["pinch_active"] or not same_viewport(expected, after):
                raise ControlError(
                    "gesture ending did not preserve/restore the exact viewport"
                )
            if case.get("cancel"):

                def restored():
                    current = smoke.window(smoke.target)
                    return current and all(
                        current[key] == window[key]
                        for key in (
                            "x",
                            "y",
                            "width",
                            "height",
                            "maximized",
                            "fullscreen",
                        )
                    )

                smoke.until(
                    "cancelled terminal geometry restored",
                    restored,
                )
                unchanged(window, smoke.window(smoke.target))
            captured = smoke.capture(f"{name}-after")
            if case.get("cancel") and marker_bounds(image, window) != marker_bounds(
                captured, smoke.window(smoke.target)
            ):
                raise ControlError(
                    "cancelled gesture did not restore the exact rendered marker"
                )
            smoke.record(
                "pan-ending-verified",
                name=name,
                cancelled=bool(case.get("cancel")),
                viewport=after,
            )
    if Path(device.device["device"]).exists():
        raise ControlError("owned pan input device survived cleanup")
    smoke.record(
        "pan-input-proof",
        name=name,
        trace=str(path),
        device_removed=True,
        **pan_input.verify_trace(path, kind, case, state),
    )
