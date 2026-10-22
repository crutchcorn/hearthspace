"""Native acceptance for standalone horizontal and vertical overflowing trays."""

import json
from pathlib import Path

from . import shell_checks, shell_keyboard
from . import tray_overflow_checks as checks
from .control import ControlError
from .input import Input
from .map_checks import button_center
from .pinch_touchscreen import to_device
from .touchpad import TouchDevice
from .tray_overflow_fixture import fixture, title


def place(smoke, edge):
    smoke.control.request("layout-tool")
    editing = smoke.control.query("layout-status")

    def command(action, kind="widget", name="tray", **values):
        smoke.control.request(
            "layout-command "
            + json.dumps(
                {
                    "session": editing["session"],
                    "output": editing["output"],
                    "action": action,
                    "target": {"kind": kind, "id": name},
                    **values,
                }
            )
        )

    command("ungroup")
    # Isolate the strip from automatic overlap grouping. Keep one other frame
    # for reverse keyboard entry. All changes live in this fresh session/layout.json.
    command("allow-hide-tools", value=True)
    output = next(
        item for item in editing["outputs"] if item["output"] == editing["output"]
    )
    for widget in output["widgets"]:
        if widget["id"] not in ("tray", "apps"):
            command("remove", name=widget["id"])
    command(
        "place",
        name="apps",
        placement={"surface": "frame", "edge": "right", "position": 0.5},
    )
    command("place", placement={"surface": "frame", "edge": edge, "position": 0.5})
    command("done")
    smoke.until(
        "standalone layout settles",
        lambda: not smoke.control.query("layout-status")["animating"],
    )
    shell_checks.closed(smoke)


def wheel(smoke, edge, count, forward):
    observed = checks.observe(smoke, edge)
    # Aim at a real app icon, so a fitting-tray SNI Scroll regression is detected.
    icon = observed["visible"][len(observed["visible"]) // 2]
    point = button_center(icon["bounds"], observed["output"], observed["surface"])
    with Input(smoke.control) as inputs:
        inputs.motion(*point)
        # Vertical wheel input must scroll a horizontal standalone strip too.
        inputs.scroll(0, 10000 if forward else -10000)
    name = title(count - 1 if forward else 0)
    return smoke.until(
        f"vertical wheel reveals {name} on {edge}",
        lambda: checks.revealed(smoke, edge, name),
    )


def key(smoke, name):
    with Input(smoke.control) as inputs:
        inputs.chord(name)


def keyboard(smoke, edge, count):
    shell_keyboard.focus_control(smoke, title(0))
    geometry = checks.observe(smoke, edge)

    def traverse(chord, indices):
        for index in indices:
            key(smoke, chord)

            def reached(index=index):
                return any(
                    item["name"] == title(index)
                    and checks.fully_visible(
                        item, geometry["surface"], geometry["output"]["scale"]
                    )
                    for item in checks.elements(smoke, focused=True)
                )

            smoke.until(f"{chord} focuses and fully reveals {title(index)}", reached)

    traverse("tab", range(1, count))
    last = title(count - 1)
    observed = smoke.until(
        "Tab reveals and focuses final overflow icon",
        lambda: checks.revealed(smoke, edge, last, focused=True),
    )
    checks.evidence(smoke, edge, "tab-last", observed, [last])
    key(smoke, "tab")
    smoke.until(
        "Tab leaves tray at its last icon",
        lambda: not checks.elements(smoke, focused=True),
    )
    wheel(smoke, edge, count, False)
    key(smoke, "shift+tab")
    observed = smoke.until(
        "Shift-Tab enters tray at its revealed final icon",
        lambda: checks.revealed(smoke, edge, last, focused=True),
    )
    checks.evidence(smoke, edge, "shift-tab-last", observed, [last])
    traverse("shift+tab", range(count - 2, -1, -1))
    observed = smoke.until(
        "Shift-Tab reveals first overflow icon again",
        lambda: checks.revealed(smoke, edge, title(0), focused=True),
    )
    checks.evidence(smoke, edge, "shift-tab-first", observed, [title(0)])


def touchscreen(smoke, edge, peer, before, baseline):
    observed = checks.observe(smoke, edge)
    output, surface = observed["output"], observed["surface"]
    axis = 1 if edge == "left" else 0
    icons = sorted(
        observed["visible"], key=lambda item: item["bounds"]["y" if axis else "x"]
    )
    icon = icons[len(icons) * 3 // 4]
    start = button_center(icon["bounds"], output, surface)
    end = list(start)
    end[axis] -= min(240, surface["height" if axis else "width"] * 0.4)
    path = smoke.artifacts / f"libinput-tray-overflow-{edge}.log"
    with (
        path.open("w") as log,
        TouchDevice(
            smoke.directory,
            log,
            "touchscreen",
            deadline=smoke.control.deadline,
        ) as device,
    ):
        smoke.record(
            "tray-overflow-touch-created",
            edge=edge,
            start=start,
            end=end,
            **device.device,
        )
        device.request("begin", fingers=1, points=to_device([start], output))
        try:
            device.request("move", points=to_device([end], output))
        finally:
            device.request("end")

        initial_names = {item["name"] for item in observed["visible"]}

        def moved():
            after = checks.observe(smoke, edge)
            visible = {item["name"] for item in after["visible"]}
            return (
                after if visible - initial_names and title(0) not in visible else None
            )

        after = smoke.until("physical tray drag reveals later icons", moved)
        checks.unchanged(smoke, peer, before, baseline)
        new_names = sorted({item["name"] for item in after["visible"]} - initial_names)
        checks.evidence(smoke, edge, "touch-drag", after, [new_names[-1]])
    if Path(device.device["device"]).exists():
        raise ControlError("owned overflow touchscreen survived cleanup")
    trace = path.read_text()
    if any(
        event not in trace
        for event in (
            "TOUCH_DOWN",
            "TOUCH_MOTION",
            "TOUCH_UP",
            "TOUCH_FRAME",
            "DEVICE_REMOVED",
        )
    ):
        raise ControlError("overflow drag lacks real kernel touchscreen evidence")
    checks.unchanged(smoke, peer, before, baseline)
    smoke.record(
        "tray-overflow-touch-verified", edge=edge, trace=str(path), removed=True
    )


def edge_workflow(smoke, peer, count, edge):
    if edge != "top":
        place(smoke, edge)

    def ready():
        observed = checks.observe(smoke, edge)
        return observed if checks.require_overflow(observed, count) else None

    initial = smoke.until(
        "all fixtures loaded and the standalone tray truly overflows", ready
    )
    before, baseline = initial["viewport"], len(peer.events())
    checks.evidence(smoke, edge, "initial", initial, [title(0)])
    for forward, label in ((True, "wheel-last"), (False, "wheel-first")):
        observed = wheel(smoke, edge, count, forward)
        checks.evidence(
            smoke, edge, label, observed, [title(count - 1 if forward else 0)]
        )
        checks.unchanged(smoke, peer, before, baseline)
    keyboard(smoke, edge, count)
    checks.unchanged(smoke, peer, before, baseline)
    touchscreen(smoke, edge, peer, before, baseline)
    smoke.record(
        "tray-overflow-edge-verified",
        edge=edge,
        items=count,
        app_actions=peer.events()[baseline:],
        viewport_unchanged=True,
    )


def run(smoke):
    status = smoke.control.status()
    if status["backend"] != "native" or len(status["outputs"]) != 1:
        raise ControlError("overflow acceptance requires one owned native output")
    count = checks.item_count(status["outputs"][0])
    if checks.elements(smoke):
        raise ControlError("overflow fixture already exists")
    smoke.record(
        "tray-overflow-isolated-layout", path=str(smoke.directory / "layout.json")
    )
    # Detach before registration: a growing default system group can merge with
    # its neighbors before the native standalone test even begins.
    place(smoke, "top")
    with fixture(smoke, count) as peer:
        try:
            for edge in ("top", "left"):
                smoke.step(
                    f"tray-overflow-{edge}",
                    lambda s, edge=edge: edge_workflow(s, peer, count, edge),
                )
        except BaseException:
            smoke.failure_evidence("tray-overflow-failed")
            raise
    smoke.until(
        "all overflow app icons removed on peer exit",
        lambda: not checks.elements(smoke),
    )
    smoke.record(
        "tray-overflow-verified",
        items=count,
        edges=["top", "left"],
        fixture_removed=True,
    )
