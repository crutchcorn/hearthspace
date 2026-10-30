"""Temporary Meta+primary acceptance using the existing real Foot fixture."""

from . import canvas_pan_checks as checks
from . import canvas_pan_fixture as fixture
from . import canvas_pan_ui as ui
from .control import ControlError
from .input import BUTTONS, Input, keycode
from .shell_surfaces import observed_status
from .viewport_fixture import set_viewport
from .visual import verify_move


def observe(smoke, name, before, delta, *, window_move=False):
    """Check both sides of presentation; command acknowledgments are not proof."""
    for presented in (False, True):
        if presented:
            image = smoke.capture(name)
        status = smoke.control.status()
        if window_move:
            if not checks.same_view(before["viewport"], status["viewport"]):
                raise ControlError("Meta titlebar drag panned the canvas")
        else:
            checks.verify_pan(before, status, delta)
        if smoke.control.query("layout-status").get("canvas_tool") != "select":
            raise ControlError(f"{name} changed the selected tool")
        if status.get("shell_panels") or smoke.control.query("desktop-status").get(
            "context"
        ):
            raise ControlError(f"{name} opened Apps List or a context menu")
    smoke.record("canvas-temporary-phase", name=name, delta=delta, status=status)
    return status, image


def marker(smoke, before, image, after, captured):
    old = next(item for item in before["windows"] if item["id"] == smoke.target)
    new = next(item for item in after["windows"] if item["id"] == smoke.target)
    return checks.verify_marker(
        image, captured, old, new, before["viewport"], after["viewport"]
    )


def unmodified(smoke, name):
    start = fixture.blank(smoke, (23, 17))
    before = smoke.control.status()
    with Input(smoke.control) as inputs:
        inputs.motion(*start)
        inputs.down("pointer-button", BUTTONS["left"])
        inputs.motion(start[0] + 23, start[1] + 17)
        observe(smoke, f"{name}-held", before, (0, 0))
        inputs.up("pointer-button", BUTTONS["left"])
        inputs.motion(start[0] + 28, start[1] + 22)
        observe(smoke, f"{name}-released", before, (0, 0))


def drag(smoke, name, *, meta_first):
    delta = (43, -27)
    start = fixture.blank(smoke, (55, -35))
    before = smoke.control.status()
    with Input(smoke.control) as inputs:
        inputs.motion(*start)
        inputs.down("key", keycode("meta"))
        _, image = observe(smoke, f"{name}-meta-down", before, (0, 0))
        inputs.down("pointer-button", BUTTONS["left"])
        observe(smoke, f"{name}-primary-down", before, (0, 0))
        for fraction in (0.13, 0.37, 0.61, 1):
            inputs.motion(
                start[0] + delta[0] * fraction, start[1] + delta[1] * fraction
            )
        active, captured = observe(smoke, f"{name}-active", before, delta)
        marker(smoke, before, image, active, captured)
        if meta_first:
            inputs.up("key", keycode("meta"))
            observe(smoke, f"{name}-meta-up", before, delta)
            # Continue from the original pointer/viewport origins after Meta up.
            delta = (55, -35)
            inputs.motion(start[0] + delta[0], start[1] + delta[1])
            active, captured = observe(smoke, f"{name}-continued", before, delta)
            marker(smoke, before, image, active, captured)
        inputs.up("pointer-button", BUTTONS["left"])
        inputs.motion(start[0] + 60, start[1] - 40)
        observe(smoke, f"{name}-primary-up", before, delta)
        if not meta_first:
            # The mouse has stopped the drag even though Meta is still held.
            inputs.up("key", keycode("meta"))
        inputs.motion(start[0] + 64, start[1] - 44)
        after, captured = observe(smoke, f"{name}-released", before, delta)
        proof = marker(smoke, before, image, after, captured)
    smoke.record(
        "canvas-temporary-drag-verified", name=name, meta_first=meta_first, **proof
    )
    unmodified(smoke, f"{name}-next-unmodified")


def cancelled(smoke):
    start = fixture.blank(smoke, (32, 20))
    before = smoke.control.status()
    with Input(smoke.control) as inputs:
        inputs.motion(*start)
        inputs.down("key", keycode("meta"))
        _, image = observe(smoke, "canvas-temporary-cancel-origin", before, (0, 0))
        inputs.down("pointer-button", BUTTONS["left"])
        inputs.motion(start[0] + 32, start[1] + 20)
        active, captured = observe(
            smoke, "canvas-temporary-cancel-active", before, (32, 20)
        )
        marker(smoke, before, image, active, captured)
        inputs.down("pointer-button", BUTTONS["right"])
        inputs.chord("esc")
        inputs.up("key", keycode("meta"))
        observe(smoke, "canvas-temporary-escape", before, (32, 20))
        window = smoke.window(smoke.target)
        if window["titlebar_height"] <= 0:
            raise ControlError("temporary pan drain requires a decorated Foot")
        target = (
            window["x"] + window["width"] / 2,
            window["y"] + window["titlebar_height"] / 2,
        )
        inputs.motion(*target)
        inputs.up("pointer-button", BUTTONS["left"])
        observe(smoke, "canvas-temporary-drain-primary", before, (32, 20))
        # A fresh primary press over chrome is still owned by the cancelled
        # sequence until every held button (including this one) has drained.
        inputs.down("pointer-button", BUTTONS["left"])
        inputs.motion(target[0] + 20, target[1] + 10)
        observe(smoke, "canvas-temporary-drain-repress", before, (32, 20))
        inputs.up("pointer-button", BUTTONS["right"])
        inputs.motion(target[0] + 40, target[1] + 20)
        observe(smoke, "canvas-temporary-drain-secondary", before, (32, 20))
        inputs.up("pointer-button", BUTTONS["left"])
        after, captured = observe(smoke, "canvas-temporary-drained", before, (32, 20))
        marker(smoke, before, image, after, captured)
    drag(smoke, "canvas-temporary-fresh-after-cancel", meta_first=True)


def rejected(smoke):
    for mode in ("ctrl", "alt", "shift", "late-meta", "middle"):
        start = fixture.blank(smoke, (30, 20))
        before = smoke.control.status()
        button = BUTTONS["middle" if mode == "middle" else "left"]
        with Input(smoke.control) as inputs:
            inputs.motion(*start)
            if mode in ("ctrl", "alt", "shift"):
                inputs.down("key", keycode(mode))
            if mode != "late-meta":
                inputs.down("key", keycode("meta"))
            inputs.down("pointer-button", button)
            inputs.motion(start[0] + 15, start[1] + 10)
            observe(smoke, f"canvas-temporary-reject-{mode}-held", before, (0, 0))
            if mode == "late-meta":
                inputs.down("key", keycode("meta"))
            elif mode in ("ctrl", "alt", "shift"):
                inputs.up("key", keycode(mode))
            # Qualifying modifiers arriving mid-sequence cannot take it over.
            inputs.motion(start[0] + 30, start[1] + 20)
            observe(smoke, f"canvas-temporary-reject-{mode}-changed", before, (0, 0))
            inputs.up("pointer-button", button)
            inputs.up("key", keycode("meta"))
            observe(smoke, f"canvas-temporary-reject-{mode}-released", before, (0, 0))


def client_routing(smoke):
    destination = fixture.receive(smoke, "canvas-temporary-client")
    with Input(smoke.control) as inputs:
        inputs.click(*fixture.blank(smoke))
    smoke.until(
        "desktop owns focus before Meta client press",
        lambda: not any(item["focused"] for item in smoke.control.status()["windows"]),
    )
    before = smoke.control.status()
    start = fixture.client_point(smoke)
    with Input(smoke.control) as inputs:
        inputs.motion(*start)
        inputs.down("key", keycode("meta"))
        inputs.down("pointer-button", BUTTONS["left"])
        inputs.motion(start[0] + 15, start[1] + 10)
        observe(smoke, "canvas-temporary-client-held", before, (0, 0))
        inputs.up("key", keycode("meta"))
        inputs.motion(start[0] + 25, start[1] + 15)
        inputs.up("pointer-button", BUTTONS["left"])
        observe(smoke, "canvas-temporary-client-released", before, (0, 0))
        inputs.type("temporary-client\n")
    fixture.received(smoke, destination, "temporary-client")
    fixture.prepare(smoke, 1)
    before = smoke.control.status()
    window = smoke.window(smoke.target)
    if window["titlebar_height"] <= 0:
        raise ControlError("Meta titlebar acceptance requires a decorated Foot")
    image = smoke.capture("canvas-temporary-titlebar-before")
    x, y = (
        window["x"] + window["width"] / 2,
        window["y"] + window["titlebar_height"] / 2,
    )
    with Input(smoke.control) as inputs:
        inputs.motion(x, y)
        inputs.down("key", keycode("meta"))
        inputs.down("pointer-button", BUTTONS["left"])
        inputs.motion(x + 24, y + 18)
        observe(
            smoke, "canvas-temporary-titlebar-held", before, (0, 0), window_move=True
        )
        inputs.up("pointer-button", BUTTONS["left"])
        inputs.up("key", keycode("meta"))
        _, captured = observe(
            smoke,
            "canvas-temporary-titlebar-released",
            before,
            (0, 0),
            window_move=True,
        )
    moved = smoke.window(smoke.target)
    for axis, distance in (("x", 24), ("y", 18)):
        if abs(moved[axis] - window[axis] - distance) > 1:
            raise ControlError("Meta titlebar drag did not move the real client")
    verify_move(image, captured, window, moved)
    smoke.record("canvas-temporary-client-routing", typed="temporary-client")


def icon_target(status, desktop, identity=None):
    """Choose an observed tile with a clear interior path, never a guessed icon."""
    if len(status["outputs"]) != 1:
        raise ControlError("mouse pan acceptance requires one active output")
    output, view = status["outputs"][0], status["viewport"]
    for item in desktop["items"]:
        if identity is not None and item["id"] != identity:
            continue
        x = (item["x"] + 44 - view["x"]) * view["scale"]
        y = (item["y"] + 48 - view["y"]) * view["scale"]
        path = (x - 8, y - 8, x + 32, y + 26)
        if not (
            output["logical_x"] + 32
            <= path[0]
            < path[2]
            <= output["logical_x"] + output["logical_width"] - 32
            and output["logical_y"] + 32
            <= path[1]
            < path[3]
            <= output["logical_y"] + output["logical_height"] - 32
        ):
            continue
        others = {
            "items": [other for other in desktop["items"] if other["id"] != item["id"]]
        }
        if not any(
            checks.overlaps(path, rect) for rect in checks.blockers(status, others)
        ):
            return item, (x, y)
    raise ControlError("no observed desktop icon clear of clients and shell")


def icon_routing(smoke):
    if not smoke.manifest.get("isolated_profile"):
        raise ControlError("desktop icon fixture requires an isolated profile")
    # The shell rediscovers XDG_DESKTOP_DIR every two seconds. Only this owned
    # profile is changed; an empty or nonexistent personal Desktop is irrelevant.
    directory = smoke.artifacts / "temporary-pan-desktop"
    directory.mkdir()
    name = "Temporary pan acceptance.txt"
    (directory / name).write_text("Owned native canvas-pan acceptance fixture.\n")
    config = smoke.directory / "profile/config/user-dirs.dirs"
    escaped = (
        str(directory)
        .replace("\\", "\\\\")
        .replace('"', '\\"')
        .replace("$", "\\$")
        .replace("`", "\\`")
    )
    with config.open("x") as output:
        output.write(f'XDG_DESKTOP_DIR="{escaped}"\n')
    identity = "file:" + name.encode().hex()
    item = smoke.until(
        "owned desktop file tile mapped",
        lambda: next(
            (
                entry
                for entry in smoke.control.query("desktop-status")["items"]
                if entry["id"] == identity
            ),
            None,
        ),
    )
    status = smoke.control.status()
    if len(status["outputs"]) != 1:
        raise ControlError("mouse pan acceptance requires one active output")
    output = status["outputs"][0]
    x = round(item["x"] + 44 - output["logical_x"] - output["logical_width"] / 2)
    y = round(item["y"] + 48 - output["logical_y"] - output["logical_height"] / 2)
    set_viewport(smoke, x=x, y=y, scale=1)
    # Move only the owned Foot fixture out of the tile's way before taking the
    # immutable application-geometry baseline for this gesture.
    fx, fy = x + output["logical_width"] + 100, y + output["logical_height"] + 100
    smoke.control.request(f"move-window {smoke.target} {fx} {fy}")
    smoke.until(
        "Foot clear of desktop tile",
        lambda: (
            (
                smoke.window(smoke.target)["canvas_x"],
                smoke.window(smoke.target)["canvas_y"],
            )
            == (fx, fy)
        ),
    )
    with Input(smoke.control) as inputs:
        inputs.click(*fixture.blank(smoke))
    before = observed_status(smoke)
    desktop = smoke.control.query("desktop-status")
    item, start = icon_target(before, desktop, identity)
    smoke.record("canvas-temporary-icon-target", item=item, point=start)
    with Input(smoke.control) as inputs:
        inputs.motion(*start)
        inputs.down("key", keycode("meta"))
        inputs.down("pointer-button", BUTTONS["left"])
        inputs.motion(start[0] + 24, start[1] + 18)
        observe(smoke, "canvas-temporary-icon-held", before, (0, 0))
        inputs.up("pointer-button", BUTTONS["left"])
        inputs.up("key", keycode("meta"))
        observe(smoke, "canvas-temporary-icon-released", before, (0, 0))
    moved = next(
        entry
        for entry in smoke.control.query("desktop-status")["items"]
        if entry["id"] == item["id"]
    )
    for axis, distance in (("x", 24), ("y", 18)):
        if moved[axis] - item[axis] != distance / before["viewport"]["scale"]:
            raise ControlError(
                "Meta desktop icon drag did not move its original target"
            )
    smoke.record("canvas-temporary-icon-routing", before=item, after=moved)


def select(smoke):
    ui.shortcut(smoke, "v", "select", meta_first=True)
    ui.wait_trigger(smoke, "Select desktop items")


def scaled(smoke, scale):
    fixture.prepare(smoke, scale)
    for meta_first in (True, False):
        order = "meta-first" if meta_first else "primary-first"
        drag(smoke, f"canvas-temporary-{scale}-{order}", meta_first=meta_first)


def ownership(smoke):
    fixture.prepare(smoke, 1)
    cancelled(smoke)
    rejected(smoke)
    client_routing(smoke)


def run(smoke):
    smoke.step("canvas-temporary-select", select)
    for scale in (0.8, 1.0, 2.0):
        smoke.step(
            f"canvas-temporary-{scale}",
            lambda current, scale=scale: scaled(current, scale),
        )
    smoke.step("canvas-temporary-ownership", ownership)
    smoke.step("canvas-temporary-icon", icon_routing)
    smoke.record(
        "canvas-temporary-verified",
        scales=[0.8, 1.0, 2.0],
        selected_tool="select",
        release_orders=["meta-first", "primary-first"],
        escape_secondary_drained=True,
        client_titlebar_and_icon_routing=True,
    )
