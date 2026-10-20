"""App gestures configured in Settings and drawn in the Apps List."""

import contextlib
import itertools
import json
import math
import time
from pathlib import Path
from types import SimpleNamespace

from . import search_touch
from . import shell_checks as checks
from .control import ControlError
from .daily_checks import a11y
from .input import Input
from .map_checks import SHELL, button_center

APP_ID = "hearthspace-favorite-fixture.desktop"
APP_NAME = "Hearthspace Favorite Fixture"
WINDOW_TITLE = "HEARTHSPACE_FAVORITE_LAUNCH"
SETTINGS = "//*[@role='application' and @name='hearthspace-settings']"


@contextlib.contextmanager
def services(directory, _evidence, env, _deadline):
    """The generic shell harness inherits config; isolate all favorite writes."""
    config, data = directory / "config", directory / "data"
    applications = data / "applications"
    applications.mkdir(parents=True)
    config.mkdir()
    search_touch.scroll_apps(applications)
    env.update(XDG_CONFIG_HOME=str(config), XDG_DATA_HOME=str(data))
    (applications / APP_ID).write_text(
        "[Desktop Entry]\nType=Application\n"
        f"Name={APP_NAME}\nExec=foot --title={WINDOW_TITLE}\n"
        "Categories=Utility;\nOnlyShowIn=Hearthspace;\n"
    )
    yield SimpleNamespace(favorites=config / "hearthspace" / "favorites.json")


def settings_elements(smoke, name, role=None):
    role_filter = f"@role='{role}' and " if role else ""
    if role == "button":
        role_filter += "@focusable='true' and "
    query = f"({SETTINGS}//*[{role_filter}@showing='true' and @name='{name}'])[1]"
    return a11y(smoke, "find", query)["elements"]


def settings_surface(smoke):
    window = smoke.window(smoke.settings_id)
    if not window:
        raise ControlError("owned Settings window disappeared")
    return {
        "x": window["x"],
        "y": window["y"] + window["titlebar_height"],
        "width": window["content_width"],
        "height": window["content_height"],
    }


def settings_click(smoke, name, role="button"):
    element = smoke.until(
        f"Settings {name}", lambda: settings_elements(smoke, name, role)
    )[0]
    point = button_center(
        element.get("bounds"),
        smoke.control.status()["outputs"][0],
        settings_surface(smoke),
    )
    with Input(smoke.control) as inputs:
        inputs.click(*point)
    smoke.record("favorite-settings-control", name=name, point=point)


def l_shape(rect, *, scale=1.0, shifted=False):
    x, y, width, height = rect
    length = min(width, height) * 0.55 * scale
    origin = (x + width * (0.45 if shifted else 0.2), y + height * 0.17)
    return [
        origin,
        (origin[0], origin[1] + length),
        (origin[0] + length * 0.7, origin[1] + length),
    ]


def draw(smoke, points):
    """Ordinary primary-button drag is also the one-finger trackpad path."""
    with Input(smoke.control) as inputs:
        inputs.motion(*points[0])
        inputs.down("pointer-button", 272)
        for start, end in itertools.pairwise(points):
            for step in range(1, 13):
                inputs.motion(*(a + (b - a) * step / 12 for a, b in zip(start, end)))
                time.sleep(min(0.012, smoke.control.deadline.remaining()))
        inputs.up("pointer-button", 272)


def configure(smoke):
    before = window_ids(smoke)
    if not checks.panel(smoke, "search"):
        checks.click(smoke, "Apps List")
        checks.opened(smoke, "search")
    checks.click(smoke, "Settings", "search")
    window = smoke.until(
        "owned Settings window",
        lambda: next(
            (
                w
                for w in smoke.control.status()["windows"]
                if w["id"] not in before
                and w["title"] == "Hearthspace Settings"
                and w["mapped"]
            ),
            None,
        ),
    )
    smoke.settings_id = window["id"]
    smoke.until(
        "Settings opened directly on Gestures",
        lambda: settings_elements(smoke, "Find apps"),
    )
    smoke.record("search-settings-direct-page", page="Gestures")
    settings_click(smoke, "Find apps", role=None)
    with Input(smoke.control) as inputs:
        inputs.type(APP_NAME)
    settings_click(smoke, f"Add gesture for {APP_NAME}")
    element = smoke.until(
        "Settings gesture drawing area",
        lambda: settings_elements(smoke, "Gesture drawing area"),
    )[0]
    surface = settings_surface(smoke)
    scale = smoke.control.status()["outputs"][0]["scale"]
    bounds = element["bounds"]
    rect = [
        surface["x"] + bounds["x"] / scale,
        surface["y"] + bounds["y"] / scale,
        bounds["width"] / scale,
        bounds["height"] / scale,
    ]
    draw(smoke, l_shape(rect))
    smoke.capture("favorite-gesture-recorded-in-settings")
    settings_click(smoke, "Save gesture")
    smoke.until(
        "persisted assigned gesture",
        lambda: settings_elements(smoke, f"Change gesture for {APP_NAME}", "button"),
    )
    path = smoke.service_fixture.favorites
    document = json.loads(path.read_text())
    entries = document["favorites"]
    if (
        document["version"] != 1
        or len(entries) != 1
        or entries[0]["app_id"] != APP_ID
        or len(entries[0]["gesture"]["points"]) != 64
    ):
        raise ControlError("Settings did not persist the assigned favorite shape")
    if path.stat().st_mode & 0o077:
        raise ControlError("favorites file is not private to its owner")
    smoke.record("favorite-persisted", path=str(path), app_id=APP_ID)
    smoke.capture("favorite-settings-saved")
    if smoke.control.status()["backend"] == "native":
        search_touch.recording_cancel(smoke)
        search_touch.settings_scroll(smoke)
    smoke.control.request(f"close-window {smoke.settings_id}")
    smoke.until("Settings closed", lambda: smoke.window(smoke.settings_id) is None)


def window_ids(smoke):
    return {window["id"] for window in smoke.control.status()["windows"]}


def launch_mode(smoke):
    if not checks.panel(smoke, "search"):
        checks.click(smoke, "Apps List")
        checks.opened(smoke, "search")
    smoke.until(
        "assigned app remains browsable in Apps List",
        lambda: a11y(
            smoke, "find", f"{SHELL}//*[@role='button' and @name='Open: {APP_NAME}']"
        )["elements"],
    )
    checks.elements(smoke, "Settings")
    checks.click(smoke, "Draw to launch", "search")
    state = smoke.until(
        "armed search drawing area",
        lambda: (
            value
            if (value := smoke.control.status().get("favorite_gesture", {})).get(
                "armed"
            )
            else None
        ),
    )
    smoke.until(
        "drawing readiness presented in the search UI",
        lambda: checks.elements(
            smoke,
            "Lift your finger or release the button to launch a clear match.",
            role=None,
        ),
    )
    panel = checks.opened(smoke, "search")
    x, y, width, height = state["bounds"]
    return [panel["x"] + x, panel["y"] + y, width, height]


def no_launch(smoke, before, viewport, label):
    time.sleep(min(0.18, smoke.control.deadline.remaining()))
    if window_ids(smoke) != before:
        raise ControlError(f"{label} unexpectedly launched or closed an application")
    after = smoke.control.status()["viewport"]
    if any(after[key] != viewport[key] for key in ("x", "y", "scale")):
        raise ControlError(f"{label} unexpectedly moved the canvas")
    checks.opened(smoke, "search")
    smoke.record("favorite-no-launch", reason=label)


def launched(smoke, before, label):
    window = smoke.until(
        f"favorite launched by {label}",
        lambda: next(
            (
                w
                for w in smoke.control.status()["windows"]
                if w["id"] not in before
                and w["title"] == WINDOW_TITLE
                and w["mapped"]
                and w["focused"]
            ),
            None,
        ),
    )
    checks.closed(smoke)
    if smoke.control.status().get("favorite_gesture", {}).get("armed"):
        raise ControlError("drawing capture survived a successful launch")
    smoke.capture(f"favorite-launched-{label}")
    smoke.record("favorite-immediate-launch", input=label, window=window)
    smoke.control.request(f"close-window {window['id']}")
    smoke.until("owned favorite app closed", lambda: smoke.window(window["id"]) is None)


def touch_cases(smoke, rect):
    """Optional real libinput coverage, only in the harness-owned native VT."""
    from .pinch_touchscreen import to_device
    from .touchpad import TouchDevice

    output = smoke.control.status()["outputs"][0]
    before, viewport = window_ids(smoke), smoke.control.status()["viewport"]
    points = l_shape(rect, scale=0.8, shifted=True)
    trace = smoke.artifacts / "libinput-favorite-touchscreen.log"
    with (
        trace.open("w") as log,
        TouchDevice(
            smoke.directory, log, "touchscreen", deadline=smoke.control.deadline
        ) as device,
    ):
        device.request("begin", fingers=1, points=to_device(points[:1], output))
        device.request(
            "add-finger",
            point=to_device([(points[0][0] + 30, points[0][1])], output)[0],
        )
        device.request("end")
        no_launch(smoke, before, viewport, "second touchscreen contact")
        device.request("begin", fingers=1, points=to_device(points[:1], output))
        try:
            for point in points[1:]:
                device.request("move", points=to_device([point], output))
        finally:
            device.request("end")
    if Path(device.device["device"]).exists():
        raise ControlError("owned favorite touchscreen survived cleanup")
    text = trace.read_text()
    if any(event not in text for event in ("TOUCH_DOWN", "TOUCH_MOTION", "TOUCH_UP")):
        raise ControlError("favorite touch acceptance lacks real device events")
    launched(smoke, before, "touchscreen")


def run(smoke):
    if smoke.control.status()["backend"] == "native":
        search_touch.apps_corner(smoke)
    configure(smoke)
    rect = launch_mode(smoke)
    before, viewport = window_ids(smoke), smoke.control.status()["viewport"]
    x, y, width, height = rect
    radius = min(width, height) * 0.25
    circle = [
        (
            x + width / 2 + radius * math.cos(i * math.tau / 24),
            y + height / 2 + radius * math.sin(i * math.tau / 24),
        )
        for i in range(25)
    ]
    draw(smoke, circle)
    smoke.until(
        "unassigned shape feedback",
        lambda: checks.elements(
            smoke, "No matching gesture. Try again, or return to Apps List.", role=None
        ),
    )
    no_launch(smoke, before, viewport, "unassigned circle")
    smoke.capture("favorite-unknown-shape")
    # Even reentering before release cannot complete a cancelled stroke.
    points = l_shape(rect)
    draw(smoke, [points[0], (x - 8, points[0][1]), *points[1:]])
    no_launch(smoke, before, viewport, "stroke leaves drawing area")
    checks.click(smoke, "Back to apps list", "search")
    smoke.until(
        "drawing capture disarmed after Back to apps list",
        lambda: not smoke.control.status().get("favorite_gesture", {}).get("armed"),
    )
    no_launch(smoke, before, viewport, "drawing mode closed")
    rect = launch_mode(smoke)
    points = l_shape(rect)
    with Input(smoke.control) as inputs:
        inputs.motion(*points[0])
        inputs.down("pointer-button", 272)
        inputs.motion(*points[1])
        inputs.chord("escape")
        inputs.up("pointer-button", 272)
    checks.closed(smoke)
    if window_ids(smoke) != before:
        raise ControlError("Escape during a held drawing stroke launched an app")
    if smoke.control.status().get("favorite_gesture", {}).get("armed"):
        raise ControlError("Escape left drawing capture armed")
    smoke.record("favorite-no-launch", reason="Escape during held stroke")
    rect = launch_mode(smoke)
    smoke.capture("favorite-search-drawing-mode")
    draw(smoke, l_shape(rect, scale=0.75, shifted=True))
    launched(smoke, before, "pointer")
    if smoke.control.status()["backend"] == "native":
        touch_cases(smoke, launch_mode(smoke))
    smoke.record(
        "favorites-verified",
        settings_assignment=True,
        persistence=True,
        unknown_rejected=True,
        cancelled_rejected=True,
        scaled_translated_match=True,
        immediate_launch=True,
    )
