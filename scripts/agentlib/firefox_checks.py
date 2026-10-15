"""Real Firefox controls, pointer resizing, and three-contact canvas gestures."""

import re
import shlex
from pathlib import Path

from . import firefox_assertions as verify
from .control import ControlError, save_capture
from .daily_checks import a11y, terminal
from .firefox_touch import DeadlineTouchDevice
from .input import Input
from .pinch_assertions import same_viewport
from .pinch_touchscreen import centroid, scaled, settled, to_device
from .visual import marker_bounds

FIREFOX = "//*[@role='application' and contains(@name,'Firefox')]"
SHELL = "//*[@role='application' and @name='hearthspace-shell']"


def selector(name, application=FIREFOX):
    return f"({application}//*[@showing='true' and @name='{name}'])[1]"


def activate(smoke, name, application=FIREFOX):
    target = selector(name, application)
    elements = smoke.until(
        f"visible {name} control", lambda: a11y(smoke, "find", target)["elements"]
    )
    smoke.record("ui-control", name=name, selector=target, elements=elements)
    a11y(smoke, "activate", target)


def launch(smoke):
    terminal(smoke)
    existing = {window["id"] for window in smoke.control.status()["windows"]}
    command = [
        str(smoke.args.firefox),
        "--no-remote",
        "--profile",
        str(smoke.profile),
        (smoke.directory / "browser-fixtures/zoom.html").as_uri(),
    ]
    with Input(smoke.control) as inputs:
        inputs.type("exec " + shlex.join(command) + "\n")
    smoke.record("firefox-launch", argv=command, source="owned Foot terminal")
    window = smoke.until(
        "new Firefox window",
        lambda: next(
            (
                window
                for window in smoke.control.status()["windows"]
                if window["id"] not in existing
                and window.get("mapped")
                and "firefox" in (window.get("app_id") or "").lower()
            ),
            None,
        ),
    )
    smoke.target = window["id"]
    dismissed = set()

    def fixture_ready():
        # Only the known initial Continue/Not now controls may be dismissed.
        # The test page itself has neither, and no existing profile is involved.
        for name in ("Continue", "Not now"):
            if (
                name not in dismissed
                and a11y(smoke, "find", selector(name))["elements"]
            ):
                smoke.capture(f"onboarding-{len(dismissed)}-before")
                activate(smoke, name)
                dismissed.add(name)
                return None
        window = smoke.window(smoke.target)
        image = smoke.control.capture()
        try:
            marker_bounds(image, window)
        except (ControlError, TypeError):
            return None
        return window, image

    window, image = smoke.until("local Firefox scale fixture", fixture_ready)
    smoke.record(
        "firefox-ready",
        window=window,
        onboarding=sorted(dismissed),
        **save_capture(image, smoke.artifacts / "firefox-ready.png"),
    )


def prepare_maximized(smoke):
    # Each independent case starts at 1.0, clear of the minimum zoom limit.
    smoke.control.request("reset-view")
    smoke.until(
        "unit canvas scale",
        lambda: abs(smoke.control.status()["viewport"]["scale"] - 1) < 1e-8,
    )
    smoke.control.request(f"focus-window {smoke.target}")
    if not verify.maximized(smoke.window(smoke.target)):
        smoke.floating_before_max = smoke.window(smoke.target)
        activate(smoke, "Maximize")
    return wait_maximized(smoke)


def wait_maximized(smoke):
    outputs = smoke.control.status()["outputs"]
    return smoke.until(
        "Firefox maximized configure applied to its usable output bounds",
        lambda: (
            window
            if verify.maximized_at_output(window := smoke.window(smoke.target), outputs)
            else None
        ),
    )


def controls_and_resize(smoke):
    prepare_maximized(smoke)
    smoke.capture("csd-restore-before")
    activate(smoke, "Restore Down")
    before = smoke.until(
        "Firefox restored by CSD",
        lambda: (
            window if verify.restored(window := smoke.window(smoke.target)) else None
        ),
    )
    smoke.capture("csd-restore-after")
    output = smoke.control.status()["outputs"][0]
    x, y = before["x"] + before["width"] - 1, before["y"] + before["height"] / 2
    if not (
        output["logical_x"] <= x < output["logical_x"] + output["logical_width"]
        and output["logical_y"] <= y < output["logical_y"] + output["logical_height"]
    ):
        raise ControlError("Firefox right resize edge is outside the captured output")
    amount = 160
    with Input(smoke.control) as inputs:
        inputs.drag(x, y, x - amount, y, duration=0.5)
    after = smoke.until(
        "pointer-resized Firefox",
        lambda: (
            window
            if (window := smoke.window(smoke.target))
            and before["width"] - window["width"] > 80
            else None
        ),
    )
    verify.verify_resize(before, after, amount)
    image = smoke.capture("edge-resize-after")
    marker_bounds(image, after)
    smoke.record(
        "edge-resize-verified", before=before, after=after, pointer_delta=-amount
    )


def zoom_result(smoke, name, before_view, before, image):
    after_view = settled(smoke)
    last_error = None

    def visible_result():
        nonlocal last_error
        after = smoke.window(smoke.target)
        captured = smoke.control.capture()
        try:
            proof = verify.verify_zoom(
                before_view, after_view, before, after, image, captured
            )
        except ControlError as error:
            last_error = str(error)
            return None
        return after, captured, proof

    try:
        after, captured, proof = smoke.until(
            f"{name} visible browser shrink", visible_result
        )
    except ControlError as error:
        raise ControlError(f"{name}: {last_error or error}") from error
    smoke.record(
        name,
        before_viewport=before_view,
        after_viewport=after_view,
        before=before,
        after=after,
        **proof,
        **save_capture(captured, smoke.artifacts / f"{name}-after.png"),
    )


def begin_pinch(smoke, device, output):
    window = smoke.window(smoke.target)
    with Input(smoke.control) as inputs:
        inputs.motion(
            window["x"] + window["width"] / 2, window["y"] + window["height"] / 2
        )
    if device.kind == "touchpad":
        device.request("begin", fingers=3, distance=900)
        device.request("move", distance=550)
        return None
    # Interior contacts avoid CSD, browser chrome, and page controls.
    points = to_device(
        [
            (window["x"] + window["width"] * x, window["y"] + window["height"] * y)
            for x, y in ((0.3, 0.45), (0.7, 0.45), (0.5, 0.75))
        ],
        output,
    )
    device.request("begin", fingers=3, points=points)
    moved = scaled(points, 0.65)
    device.request("move", points=moved)
    return moved


def touch_zoom(smoke, kind):
    before = prepare_maximized(smoke)
    output = smoke.control.status()["outputs"][0]
    before_view = smoke.control.status()["viewport"]
    image = smoke.capture(f"{kind}-zoom-before")
    path = smoke.artifacts / f"libinput-firefox-{kind}.log"
    with (
        path.open("w") as log,
        DeadlineTouchDevice(
            smoke.directory, log, smoke.control.deadline, kind=kind
        ) as device,
    ):
        smoke.record("virtual-device", kind=kind, **device.device)
        begin_pinch(smoke, device, output)
        smoke.until(
            "active three-contact pinch",
            lambda: (
                view
                if (view := smoke.control.status()["viewport"])["pinch_active"]
                and view["scale"] < 0.85
                else None
            ),
        )
        smoke.capture(f"{kind}-zoom-active")
        device.request("end")
        smoke.until(
            "pinch ended",
            lambda: not smoke.control.status()["viewport"]["pinch_active"],
        )
        zoom_result(smoke, f"{kind}-zoom", before_view, before, image)
        cancel_pinch(smoke, device, output)
    if Path(device.device["device"]).exists():
        raise ControlError(f"owned virtual {kind} survived cleanup")
    trace = path.read_text()
    required = (
        ("GESTURE_PINCH_BEGIN", "GESTURE_PINCH_UPDATE", "GESTURE_PINCH_END")
        if kind == "touchpad"
        else ("TOUCH_DOWN", "TOUCH_MOTION", "TOUCH_UP", "TOUCH_FRAME")
    )
    if any(event not in trace for event in required):
        raise ControlError(f"{kind} trace lacks its required input events")
    if kind == "touchpad" and not re.search(
        r"GESTURE_PINCH_BEGIN\s+\+\S+s\s+3\b", trace
    ):
        raise ControlError("touchpad trace does not prove three-finger recognition")
    smoke.record("virtual-device-removed", kind=kind, trace=str(path))


def cancel_pinch(smoke, device, output):
    before = prepare_maximized(smoke)
    floating = smoke.floating_before_max
    viewport = smoke.control.status()["viewport"]
    smoke.record(
        "pinch-cancellation-before",
        kind=device.kind,
        maximized=before,
        floating=floating,
        viewport=viewport,
    )
    moved = begin_pinch(smoke, device, output)
    smoke.until(
        "restored during active pinch",
        lambda: verify.restored(smoke.window(smoke.target)),
    )
    smoke.capture(f"{device.kind}-cancel-active")
    device.request("add-finger", **({"point": centroid(moved)} if moved else {}))
    smoke.until(
        "fourth contact cancels pinch",
        lambda: not smoke.control.status()["viewport"]["pinch_active"],
    )
    device.request("end")
    after = wait_maximized(smoke)
    restored_view = settled(smoke)
    if not same_viewport(viewport, restored_view):
        raise ControlError("cancelled Firefox pinch did not restore the exact viewport")
    if any(abs(before[key] - after[key]) > 1 for key in ("x", "y", "width", "height")):
        raise ControlError(
            "cancelled Firefox pinch did not restore its maximized bounds"
        )
    smoke.capture(f"{device.kind}-cancel-restored")
    smoke.record(
        "pinch-cancellation-verified",
        kind=device.kind,
        before=before,
        after=after,
        viewport=restored_view,
    )
    activate(smoke, "Restore Down")
    restored = smoke.until(
        "CSD restore after cancellation preserves floating geometry",
        lambda: (
            window
            if verify.restored(window := smoke.window(smoke.target))
            and all(
                abs(window[key] - floating[key]) <= 2
                for key in ("canvas_x", "canvas_y", "width", "height")
            )
            else None
        ),
    )
    smoke.capture(f"{device.kind}-cancel-floating-restored")
    smoke.record(
        "post-cancel-floating-geometry",
        kind=device.kind,
        expected=floating,
        actual=restored,
    )


def fullscreen(smoke):
    prepare_maximized(smoke)
    with Input(smoke.control) as inputs:
        inputs.chord("f11")
    outputs = smoke.control.status()["outputs"]
    before = smoke.until(
        "Firefox F11 configure applied to full output bounds",
        lambda: (
            window
            if verify.fullscreen_at_output(
                window := smoke.window(smoke.target), outputs
            )
            else None
        ),
    )
    image = smoke.capture("fullscreen-before")
    viewport = smoke.control.status()["viewport"]
    with Input(smoke.control) as inputs:
        inputs.motion(
            before["x"] + before["width"] / 2, before["y"] + before["height"] / 2
        )
        inputs.scroll(0, 20, modifiers=("super",))
    smoke.until(
        "canvas changed behind fullscreen",
        lambda: smoke.control.status()["viewport"]["scale"] != viewport["scale"],
    )
    after_view = settled(smoke)
    after = smoke.window(smoke.target)
    captured = smoke.capture("fullscreen-after")
    proof = verify.verify_fullscreen(before, after, image, captured)
    smoke.record(
        "fullscreen-preserved",
        before=before,
        after=after,
        before_viewport=viewport,
        after_viewport=after_view,
        **proof,
    )
    with Input(smoke.control) as inputs:
        inputs.chord("f11")
    smoke.until(
        "Firefox leaves F11 fullscreen",
        lambda: not smoke.window(smoke.target).get("fullscreen"),
    )
