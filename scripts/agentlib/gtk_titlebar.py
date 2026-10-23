"""Opt-in CSD acceptance; borrows root's native session, owns only its clients."""

import time
from contextlib import contextmanager
from pathlib import Path

from . import gtk_titlebar_assertions as verify
from . import gtk_touch_checks as checks
from .content_pixels import content_changed
from .control import ControlError
from .csd_shell import contact_before_entry, focused_hidden, park_before_entry
from .input import Input
from .pinch_touchscreen import settled
from .touch_drag_checks import point
from .touchpad import TouchDevice
from .viewport_fixture import set_viewport


def client_state(smoke):
    return checks.client_state(smoke)


def wait_state(smoke, label, mode, original):
    last_error = None

    def ready():
        nonlocal last_error
        window = smoke.window(smoke.target)
        client = client_state(smoke)
        if client is None:
            return None
        try:
            verify.state(
                window,
                client,
                smoke.output,
                smoke.control.status()["viewport"],
                mode,
                original,
                smoke.fullscreen_entries,
            )
        except ControlError as error:
            last_error = error
            return None
        return window, client

    try:
        window, client = smoke.until(f"{label}: {mode}", ready)
    except ControlError as error:
        raise ControlError(f"{error}; last state mismatch: {last_error}") from error
    smoke.record("csd-state", name=label, mode=mode, window=window, client=client)
    return window


def target(smoke, label, header):
    # A capture always precedes discovery. No bounds survive a state-changing
    # action, including restore/fullscreen where controls move or disappear.
    smoke.capture(f"{label}-before")
    window = smoke.window(smoke.target)
    if header:
        query = (
            f"{checks.selector(smoke)}//*[@name='GTK{smoke.kind[-1]} touch fixture']"
        )
        find = lambda: checks.accessible(smoke, query)
    else:
        names = (
            ("Restore", "Unmaximize", "Restore Down")
            if window["maximized"]
            else ("Maximize",)
        )
        if smoke.kind == "gtk4" and window["maximized"]:
            # GtkWindowControls keeps the accessible label "Maximize" while
            # window.toggle-maximized changes the icon and restores the window.
            names += ("Maximize",)
        find = lambda: checks.button(smoke, names)
    node, bounds, origin = smoke.until("current GTK titlebar target", find)
    window = smoke.window(smoke.target)
    viewport = smoke.control.status()["viewport"]
    location = checks.local_center(
        bounds, window, checks.presentation_scale(window, viewport), origin
    )
    smoke.record(
        "csd-target",
        name=label,
        header=header,
        node=node,
        bounds=bounds,
        geometry_origin=origin,
        point=location,
    )
    return location


def gesture(smoke, device, label, header, touch):
    # Keep separate gestures outside GTK's default multi-click grouping window.
    time.sleep(min(0.45, smoke.control.deadline.remaining()))
    park_before_entry(smoke, smoke.output)
    location = target(smoke, label, header)
    points, actual = point(smoke, location)
    start = time.monotonic()
    with Input(smoke.control) as inputs:
        for index in range(2 if header else 1):
            if index:
                time.sleep(min(0.06, smoke.control.deadline.remaining()))
            if touch:
                device.request("begin", fingers=1, points=points)
                try:
                    smoke.control.deadline.remaining()
                finally:
                    device.request("end")
            else:
                inputs.click(*location)
    contact_before_entry(smoke, actual if touch else location, touch)
    smoke.record(
        "csd-gesture",
        name=label,
        physical_touch=touch,
        clicks=2 if header else 1,
        actual_point=actual if touch else location,
        elapsed=time.monotonic() - start,
    )


def key(smoke, label, chord):
    park_before_entry(smoke, smoke.output)
    smoke.capture(f"{label}-before")
    with Input(smoke.control) as inputs:
        inputs.chord(chord)
    smoke.record("csd-key", name=label, chord=chord)


def evidence(smoke, device, label, mode, original):
    window = wait_state(smoke, label, mode, original)
    if mode == "focused":
        focused_hidden(smoke, label)
    before, editor_bounds = None, None
    if smoke.kind == "generic":
        before = smoke.capture(f"{label}-before-editor-input")
        _, bounds, origin = smoke.until(
            "generic fixture editor bounds",
            lambda: checks.accessible(
                smoke,
                "//*[@role='application' and contains(@name,'WindowTest')]//*[@editable='true']",
            ),
        )
        scale = checks.presentation_scale(window, smoke.control.status()["viewport"])
        center = checks.local_center(bounds, window, scale, origin)
        editor_bounds = (
            center[0] - bounds[2] * scale / 2,
            center[1] - bounds[3] * scale / 2,
            center[0] + bounds[2] * scale / 2,
            center[1] + bounds[3] * scale / 2,
        )
        smoke.gtk_count += 1
        # Alternate the leading glyphs so even similarly named checkpoints
        # produce a visible change far larger than a blinking caret.
        text = f"{'M' * 16 if smoke.gtk_count % 2 else 'i' * 16} CSD {label}"
        with Input(smoke.control) as inputs:
            inputs.chord("ctrl+a")
            inputs.type(text)
        smoke.until(
            "generic client receives exact input",
            lambda: (client_state(smoke) or {}).get("text") == text,
        )
        color = (0, 204, 102)
    else:
        checks.content(smoke, device, label)
        color = (49, 93, 221) if smoke.gtk_count % 2 else (39, 179, 64)
    window = wait_state(smoke, f"{label}-after-input", mode, original)

    def visible():
        image = smoke.capture(f"{label}-rendered")
        try:
            if before is not None:
                content_changed(before, image, editor_bounds)
            return verify.rendered(image, window, color)
        except ControlError:
            return None

    count = smoke.until("current application marker is presented", visible)
    smoke.record(
        "csd-rendered-input",
        name=label,
        marker_pixels=count,
        client=client_state(smoke),
    )


def keyboard_cases(smoke, device, prefix, original):
    # Begin floating. These intentionally exercise both distinct compositor
    # shortcuts and the application's own protocol-fullscreen request.
    cases = (
        ("canvas-shortcut", "super+m", "canvas"),
        ("focused-shortcut", "super+shift+f", "focused"),
        ("focused-shortcut-exit", "super+shift+f", "canvas"),
        ("focused-again", "super+shift+f", "focused"),
        ("app-fullscreen", "f11", "fullscreen"),
        ("app-fullscreen-exit", "f11", "focused"),
        ("canvas-from-focused", "super+m", "canvas"),
        ("canvas-restore", "super+m", "floating"),
    )
    for name, chord, mode in cases:
        label = f"{prefix}-{name}"
        if mode == "fullscreen":
            smoke.fullscreen_entries += 1
        key(smoke, label, chord)
        evidence(smoke, device, label, mode, original)


@contextmanager
def touchscreen_batch(smoke, name):
    """Own one device per coherent batch, within the worker's fixed lifetime."""
    path = smoke.artifacts / f"libinput-{name}.log"
    device = None
    try:
        with (
            path.open("w") as log,
            TouchDevice(
                smoke.directory, log, "touchscreen", deadline=smoke.control.deadline
            ) as device,
        ):
            smoke.record("csd-touchscreen-created", name=name, **device.device)
            yield device
    finally:
        # TouchDevice and its log have exited before checking removal, including
        # when a gesture/assertion fails. Keep the failed batch's physical trace.
        if device is not None:
            if Path(device.device["device"]).exists():
                raise ControlError("owned CSD touchscreen survived cleanup")
            smoke.record("csd-touchscreen-removed", name=name, trace=str(path))
    trace = path.read_text()
    if any(event not in trace for event in ("TOUCH_DOWN", "TOUCH_UP", "TOUCH_FRAME")):
        raise ControlError("CSD titlebar trace lacks real touchscreen evidence")


def gtk_cases(smoke, prefix, original):
    for touch in (False, True):
        method = "touch" if touch else "pointer"
        with touchscreen_batch(smoke, f"{prefix}-{method}") as device:
            for name, header, mode in (
                ("button", False, "focused"),
                ("button-restore", False, "floating"),
                ("double-header", True, "canvas"),
                ("double-header-restore", True, "floating"),
                ("button-again", False, "focused"),
                ("double-header-from-focused", True, "canvas"),
                ("double-header-final-restore", True, "floating"),
            ):
                label = f"{prefix}-{method}-{name}"
                gesture(smoke, device, label, header, touch)
                evidence(smoke, device, label, mode, original)
    with touchscreen_batch(smoke, f"{prefix}-keyboard") as device:
        keyboard_cases(smoke, device, prefix, original)


def generic_cases(smoke, prefix, original):
    # F9 calls Gtk.Window.maximize/unmaximize from ordinary application code;
    # it never uses the GTK-specific titlebar double-click extension.
    for name, mode in (("f9-maximize", "focused"), ("f9-restore", "floating")):
        label = f"{prefix}-{name}"
        key(smoke, label, "f9")
        evidence(smoke, None, label, mode, original)
    keyboard_cases(smoke, None, prefix, original)


def run(smoke, scale):
    prefix = f"{smoke.kind}-{scale}"
    set_viewport(smoke, x=0, y=0, scale=1)
    smoke.control.request(f"focus-window {smoke.target}")
    smoke.control.request(f"move-window {smoke.target} 170 110")
    set_viewport(smoke, x=0, y=0, scale=scale)
    settled(smoke)
    original = wait_state(smoke, f"{prefix}-initial", "floating", None)
    if smoke.kind == "generic":
        generic_cases(smoke, prefix, original)
        return
    gtk_cases(smoke, prefix, original)
