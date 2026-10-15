"""Real GTK CSD gestures, private accessibility, and rendered content evidence."""

import ast
import subprocess
import time
from pathlib import Path

from . import gtk_touch_pixels
from . import touch_drag_assertions as verify
from .control import ControlError
from .daily_checks import a11y
from .input import Input
from .pinch_touchscreen import settled
from .touch_drag_checks import point
from .touchpad import TouchDevice
from .viewport_fixture import set_viewport


def selector(smoke):
    # GTK uses its executable name for the AT-SPI application. The owned session
    # has exactly one GTK fixture at a time; never search the host desktop bus.
    name = "alculator" if smoke.kind == "calculator" else "gtk_touch.py"
    return f"//*[@role='application' and contains(@name,'{name}')]"


def snapshot(smoke, label):
    value = a11y(smoke, "snapshot")
    destination = smoke.artifacts / f"{label}.xml"
    destination.write_text(value["snapshot"])
    elements = a11y(smoke, "find", "//*[@role='application']")["elements"]
    smoke.record("gtk-accessibility", artifact=str(destination), applications=elements)
    smoke.capture(label)


def call(smoke, address, owner, path, method, *arguments):
    result = subprocess.run(
        [
            "gdbus",
            "call",
            "--address",
            address,
            "--dest",
            owner,
            "--object-path",
            path,
            "--method",
            method,
            "--",
            *arguments,
        ],
        capture_output=True,
        text=True,
        check=True,
        timeout=smoke.control.deadline.remaining(),
    )
    return ast.literal_eval(result.stdout)[0]


def bus(smoke):
    return call(
        smoke,
        smoke.manifest["dbus_session_bus_address"],
        "org.a11y.Bus",
        "/org/a11y/bus",
        "org.a11y.Bus.GetAddress",
    )


def local_center(bounds, window, scale, origin=(0, 0)):
    x, y, width, height = bounds
    x, y = x - origin[0], y - origin[1]
    if width <= 0 or height <= 0 or x < 0 or y < 0:
        raise ControlError("GTK control has no window-relative accessible bounds")
    if (
        x + width > window["content_width"] + 1
        or y + height > window["content_height"] + 1
    ):
        raise ControlError("GTK accessible control lies outside its client window")
    return window["x"] + (x + width / 2) * scale, window["y"] + (y + height / 2) * scale


def button(smoke, names):
    alternatives = " or ".join(f"@name='{name}'" for name in names)
    query = f"{selector(smoke)}//*[contains(@role,'button') and ({alternatives})]"
    matches = a11y(smoke, "find", query)["elements"]
    if len(matches) != 1:
        return None
    node = matches[0]
    bounds = call(
        smoke,
        bus(smoke),
        node["bus_name"],
        node["object_path"],
        "org.a11y.atspi.Component.GetExtents",
        "1",
    )
    origin = (0, 0)
    if smoke.kind == "gtk3":
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
        titlebar = call(
            smoke,
            bus(smoke),
            header["bus_name"],
            header["object_path"],
            "org.a11y.atspi.Component.GetExtents",
            "1",
        )
        if titlebar[2] != smoke.window(smoke.target)["content_width"]:
            raise ControlError("GTK 3 titlebar width disagrees with window geometry")
        origin = titlebar[:2]
    return node, bounds, origin


def tap(smoke, device, names):
    node, bounds, origin = smoke.until(
        f"GTK button {names}", lambda: button(smoke, names)
    )
    view = smoke.control.status()["viewport"]
    window = smoke.window(smoke.target)
    target = local_center(bounds, window, view["scale"], origin)
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
    if smoke.kind == "gtk3":
        current = getattr(smoke, "gtk_count", 0)
        tap(smoke, device, ("Count touch",))
        expected = f"Touch count: {current + 1}"
        smoke.until(
            expected,
            lambda: a11y(smoke, "find", f"{selector(smoke)}//*[@name='{expected}']")[
                "elements"
            ],
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
                text = call(
                    smoke,
                    bus(smoke),
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
