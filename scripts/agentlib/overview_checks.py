"""Native overview gestures and selection paired with presented client markers."""

from pathlib import Path

from . import overview_input
from .control import ControlError
from .firefox_assertions import fullscreen_at_output
from .input import Input
from .overview_assertions import COLORS, marker_count, presented, preserved
from .pinch_touchscreen import to_device
from .touchpad import TouchDevice


def fixtures(smoke):
    smoke.targets = {}
    for index, color in enumerate(COLORS):
        old = {item["id"] for item in smoke.control.status()["windows"]}
        smoke.control.request("spawn foot")
        window = smoke.until(
            "new terminal mapped and focused",
            lambda old=old: next(
                (
                    item
                    for item in smoke.control.status()["windows"]
                    if item["id"] not in old and item["mapped"] and item["focused"]
                ),
                None,
            ),
        )
        target = window["id"]
        smoke.targets[target] = color
        title = f"{smoke.marker}_{index}"
        marker = (" " * 24 + r"\n") * 5
        with Input(smoke.control) as inputs:
            inputs.type("exec bash --noprofile --norc\n")
            inputs.type(
                "export PS1='$ '; unset PROMPT_COMMAND; "
                f"printf '\\033]0;{title}\\007\\033[2J\\033[H"
                f"\\033[48;2;{color[0]};{color[1]};{color[2]}m{marker}\\033[0m\\n'\n"
            )
        smoke.until(
            "unique terminal title",
            lambda target=target, title=title: smoke.window(target)["title"] == title,
        )

        def visible(target=target, color=color):
            image = smoke.control.capture()
            item = smoke.window(target)
            bounds = (
                item["x"],
                item["y"],
                item["x"] + item["width"],
                item["y"] + item["height"],
            )
            return marker_count(image, bounds, color) >= 100

        smoke.until("terminal color marker presented", visible)
        smoke.capture(f"fixture-{index}")
    first, second, third = smoke.targets
    smoke.control.request(f"move-window {first} 180 160")
    smoke.control.request(f"move-window {second} 220 190")
    smoke.control.request(f"move-window {third} 100000 100000")
    smoke.control.request(f"fullscreen-window {first} true")
    smoke.until(
        "fullscreen client committed",
        lambda: fullscreen_at_output(smoke.window(first), [smoke.output]),
    )
    smoke.capture("fixtures-arranged")
    smoke.record(
        "fixtures-arranged-state", targets=smoke.targets, status=smoke.control.status()
    )


def gesture(smoke, kind, fingers, *, outward=False, small=False, cancel=False):
    name = f"{kind}-{fingers}-" + ("close" if outward else "open")
    if small:
        name += "-subthreshold"
    if cancel:
        name += "-cancelled"
    before = smoke.control.status()
    initial = before["overview"]["active"]
    expected = initial if small or cancel else not outward
    if initial != outward:
        raise ControlError("overview gesture started in the wrong state")
    smoke.capture(f"{name}-before")
    path = smoke.artifacts / f"libinput-{name}.log"
    # Clear libinput's physical motion threshold while remaining short of
    # Hearthspace's 0.8/1.25 overview activation thresholds.
    ratio = (1.24 if outward else 0.85) if small else (1.5 if outward else 0.6)
    with (
        path.open("w") as log,
        TouchDevice(
            smoke.directory, log, kind, deadline=smoke.control.deadline
        ) as device,
    ):
        smoke.record("virtual-overview-device", name=name, **device.device)
        state = overview_input.begin(
            device, fingers, outward=outward, stagger=kind == "touchscreen"
        )
        overview_input.move(device, state, ratio)
        held = smoke.control.status()
        if held["overview"]["active"] != initial:
            raise ControlError("overview committed before successful gesture end")
        preserved(before, held)
        smoke.capture(f"{name}-held")
        if cancel:
            overview_input.cancel(
                device, state, trace_path=path, deadline=smoke.control.deadline
            )
            if kind == "touchpad":
                smoke.until(
                    "compositor received touchpad cancellation",
                    lambda: not smoke.control.status()["viewport"]["pinch_active"],
                )
        device.request("end")
        smoke.until(
            "overview gesture ending",
            lambda: (
                (state := smoke.control.status()["overview"])["active"] == expected
                and not state["animating"]
            ),
        )
        capture = smoke.capture(f"{name}-after")
        after = smoke.control.status()
        preserved(before, after)
        proof = presented(after, capture, smoke.targets) if expected else {}
        smoke.record(
            "overview-gesture-verified",
            name=name,
            before=before["overview"],
            after=after["overview"],
            thumbnails=proof,
        )
    if Path(device.device["device"]).exists():
        raise ControlError("overview input device survived cleanup")
    smoke.record(
        "overview-input-evidence",
        name=name,
        trace=str(path),
        device_removed=True,
        **overview_input.trace_proof(
            path, kind, fingers, cancelled=cancel, ratio=ratio
        ),
    )


def selection(smoke, kind):
    smoke.control.request("overview-on")
    smoke.until(
        "overview thumbnails finished appearing",
        lambda: not smoke.control.status()["overview"]["animating"],
    )
    status = smoke.control.status()
    image = smoke.capture(f"{kind}-selection-overview")
    presented(status, image, smoke.targets)
    target = list(smoke.targets)[-1 if kind == "touchscreen" else 1]
    item = next(
        item for item in status["overview"]["items"] if item["window_id"] == target
    )
    point = (item["x"] + item["width"] / 2, item["y"] + item["height"] / 2)
    if kind == "touchpad":
        with Input(smoke.control) as inputs:
            inputs.click(*point)
    else:
        path = smoke.artifacts / "libinput-thumbnail-tap.log"
        with (
            path.open("w") as log,
            TouchDevice(
                smoke.directory, log, "touchscreen", deadline=smoke.control.deadline
            ) as device,
        ):
            device.request("begin", fingers=1, points=to_device([point], smoke.output))
            if not smoke.control.status()["overview"]["active"]:
                raise ControlError("overview selected a thumbnail before touch release")
            device.request("end")
        if Path(device.device["device"]).exists():
            raise ControlError("thumbnail tap device survived cleanup")
    smoke.until(
        "thumbnail selected and focused",
        lambda: (
            not smoke.control.status()["overview"]["active"]
            and (window := smoke.window(target))
            and window["focused"]
            and window["visible"]
        ),
    )
    after = smoke.control.status()
    preserved(status, after, viewport=False)
    image = smoke.capture(f"{kind}-thumbnail-selected")
    window = smoke.window(target)
    bounds = (
        window["x"],
        window["y"],
        window["x"] + window["width"],
        window["y"] + window["height"],
    )
    if marker_count(image, bounds, smoke.targets[target]) < 100:
        raise ControlError("selected thumbnail did not reveal its client content")
    smoke.record(
        "overview-selection-verified", device=kind, window_id=target, window=window
    )
