"""Real GTK CSD gestures, private accessibility, and rendered content evidence."""

import json
import time
from pathlib import Path

from . import client_accessibility, gtk_touch_pixels
from . import touch_drag_assertions as verify
from .client_accessibility import local_center, presentation_scale
from .control import ControlError
from .daily_checks import a11y
from .input import Input
from .pinch_touchscreen import settled
from .touch_drag_checks import point
from .touchpad import TouchDevice
from .viewport_fixture import set_viewport


def selector(smoke):
    if smoke.kind in ("gtk3", "gtk4") and getattr(smoke, "client_identity", None):
        return f"//*[@role='application' and @name='{smoke.client_identity['app_id']}']"
    # Discovery remains on the owned session's private accessibility bus.
    name = "alculator" if smoke.kind == "calculator" else "gtk_touch.py"
    return f"//*[@role='application' and contains(@name,'{name}')]"


def client_state(smoke):
    """Only the live launched fixture may supply protocol/content evidence."""
    process = smoke.client_process
    if process.poll() is not None:
        raise ControlError(f"{smoke.kind} exited early: {process.returncode}")
    try:
        state = json.loads(smoke.client_state.read_text())
    except FileNotFoundError:
        return None
    if smoke.kind in ("gtk3", "gtk4"):
        expected = {**smoke.client_identity, "pid": process.pid}
        if any(state.get(key) != value for key, value in expected.items()):
            raise ControlError("GTK status does not belong to the launched fixture")
    # The generic fixture receives its fresh private status path as argv[1].
    return state


def snapshot(smoke, label):
    value = a11y(smoke, "snapshot")
    destination = smoke.artifacts / f"{label}.xml"
    destination.write_text(value["snapshot"])
    elements = a11y(smoke, "find", "//*[@role='application']")["elements"]
    smoke.record("gtk-accessibility", artifact=str(destination), applications=elements)
    smoke.capture(label)


def accessible(smoke, query):
    matches = a11y(smoke, "find", query)["elements"]
    if len(matches) != 1:
        return None
    node = matches[0]
    bounds = client_accessibility.call(
        smoke,
        client_accessibility.bus(smoke),
        node["bus_name"],
        node["object_path"],
        "org.a11y.atspi.Component.GetExtents",
        "1",
    )
    origin = (0, 0)
    if smoke.kind == "gtk3" and not smoke.window(smoke.target).get("fullscreen"):
        # GTK 3 includes its CSD shadow in AT-SPI WINDOW coordinates, whereas
        # the compositor reports the xdg window geometry without that shadow.
        # The fixture's full-width HeaderBar locates the geometry origin without
        # hardcoding theme-dependent shadow margins or assuming symmetric ones.
        headers = a11y(
            smoke, "find", f"{selector(smoke)}//*[@name='GTK3 fixture titlebar']"
        )["elements"]
        if len(headers) != 1:
            return None
        header = headers[0]
        titlebar = client_accessibility.call(
            smoke,
            client_accessibility.bus(smoke),
            header["bus_name"],
            header["object_path"],
            "org.a11y.atspi.Component.GetExtents",
            "1",
        )
        if titlebar[2] != smoke.window(smoke.target)["content_width"]:
            raise ControlError("GTK 3 titlebar width disagrees with window geometry")
        origin = titlebar[:2]
    # Fullscreen GTK 3 has neither a HeaderBar nor CSD shadow: WINDOW extents
    # already use the presented content origin. Never reuse pre-fullscreen bounds.
    return node, bounds, origin


def button(smoke, names):
    alternatives = " or ".join(f"@name='{name}'" for name in names)
    return accessible(
        smoke,
        f"{selector(smoke)}//*[contains(@role,'button') and ({alternatives})]",
    )


def tap(smoke, device, names):
    smoke.capture(f"{smoke.kind}-before-content-target-{len(smoke.results)}")
    node, bounds, origin = smoke.until(
        f"GTK button {names}", lambda: button(smoke, names)
    )
    view = smoke.control.status()["viewport"]
    window = smoke.window(smoke.target)
    target = local_center(bounds, window, presentation_scale(window, view), origin)
    points, actual = point(smoke, target)
    smoke.record(
        "gtk-touch-target",
        names=names,
        node=node,
        bounds=bounds,
        geometry_origin=origin,
        point=actual,
    )
    device.request("begin", fingers=1, points=points)
    try:
        smoke.control.capture()
    finally:
        device.request("end")


def content(smoke, device, label):
    if smoke.kind in ("gtk3", "gtk4"):
        current = getattr(smoke, "gtk_count", 0)
        if (client_state(smoke) or {}).get("count") != current:
            raise ControlError(
                "GTK content counter disagrees with the launched fixture"
            )
        tap(smoke, device, ("Count touch",))
        expected = f"Touch count: {current + 1}"
        smoke.until(
            expected,
            lambda: (
                (client_state(smoke) or {}).get("count") == current + 1
                and a11y(smoke, "find", f"{selector(smoke)}//*[@name='{expected}']")[
                    "elements"
                ]
            ),
        )
        smoke.gtk_count = current + 1
    else:
        for names in (("C",), ("2",), ("+",), ("3",), ("=",)):
            tap(smoke, device, names)

        def result():
            nodes = a11y(smoke, "find", f"{selector(smoke)}//*[@editable='true']")[
                "elements"
            ]
            for node in nodes:
                text = client_accessibility.call(
                    smoke,
                    client_accessibility.bus(smoke),
                    node["bus_name"],
                    node["object_path"],
                    "org.a11y.atspi.Text.GetText",
                    "0",
                    "-1",
                )
                if text.strip() == "5":
                    return {"node": node, "text": text}
            return None

        expected = smoke.until("Calculator receives touchscreen 2 + 3 = 5", result)
    snapshot(smoke, f"{label}-content")
    smoke.record("gtk-content-touch-verified", kind=smoke.kind, result=expected)


def release(smoke, before, name):
    with Input(smoke.control) as inputs:
        inputs.motion(smoke.output["logical_x"] + 20, smoke.output["logical_y"] + 75)
    time.sleep(min(0.06, smoke.control.deadline.remaining()))
    verify.unchanged(before, smoke.window(smoke.target))
    smoke.capture(f"{name}-released")
    smoke.record("gtk-release-verified", name=name, window=before)


def drag(smoke, device, name, delta):
    before = smoke.window(smoke.target)
    view = settled(smoke)
    # Calculator's centered Basic label is a mode dropdown. Its blank header
    # space between Undo/Redo and that dropdown is the draggable area.
    title_x = (
        100 * view["scale"] if smoke.kind == "calculator" else before["width"] * 0.5
    )
    start = (before["x"] + title_x, before["y"] + 23 * view["scale"])
    start_device, actual_start = point(smoke, start)
    image = smoke.capture(f"{name}-before")
    smoke.record(
        "gtk-drag-target", name=name, before=before, point=start, viewport=view
    )
    device.request("begin", fingers=1, points=start_device)
    try:
        for phase, fraction in (("active", 0.5), ("continued", 1)):
            destination = tuple(start[axis] + delta[axis] * fraction for axis in (0, 1))
            points, actual = point(smoke, destination)
            expected = tuple(actual[axis] - actual_start[axis] for axis in (0, 1))
            device.request("move", points=points)

            def follows(expected=expected):
                after = smoke.window(smoke.target)
                try:
                    verify.moved(before, after, expected, view["scale"])
                except ControlError:
                    return None
                return after

            after = smoke.until(f"{name} {phase} follows held contact", follows)
            capture = smoke.capture(f"{name}-{phase}")
            proof = gtk_touch_pixels.translated(before, after, image, capture)
            verify.viewport_unchanged(view, smoke.control.status()["viewport"])
            smoke.record(
                "gtk-drag-verified",
                name=name,
                phase=phase,
                before=before,
                after=after,
                **proof,
            )
    finally:
        device.request("end")
    release(smoke, after, name)


def run(smoke, scale):
    set_viewport(smoke, x=0, y=0, scale=1)
    smoke.control.request(f"restore-window {smoke.target}")
    smoke.control.request(f"focus-window {smoke.target}")
    smoke.control.request(f"move-window {smoke.target} 170 90")
    set_viewport(smoke, x=0, y=0, scale=scale)
    settled(smoke)
    name = f"{smoke.kind}-{scale}"
    path = smoke.artifacts / f"libinput-{name}.log"
    with (
        path.open("w") as log,
        TouchDevice(
            smoke.directory, log, "touchscreen", deadline=smoke.control.deadline
        ) as device,
    ):
        smoke.record("gtk-touchscreen-created", name=name, **device.device)
        drag(smoke, device, f"{name}-first", (90, 50))
        drag(smoke, device, f"{name}-repeat", (-70, -35))
        content(smoke, device, name)
    if Path(device.device["device"]).exists():
        raise ControlError("owned GTK touchscreen survived cleanup")
    trace = path.read_text()
    if any(
        event not in trace
        for event in ("TOUCH_DOWN", "TOUCH_MOTION", "TOUCH_UP", "TOUCH_FRAME")
    ):
        raise ControlError("GTK drag trace lacks real touchscreen evidence")
    smoke.record("gtk-touchscreen-removed", name=name, trace=str(path))
