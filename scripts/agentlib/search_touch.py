"""Native search swipes and Settings scrolling through owned libinput touch."""

import contextlib
from pathlib import Path

from . import shell_checks as checks
from .control import ControlError
from .input import Input
from .map_checks import button_center, corner_surface
from .pinch_touchscreen import to_device
from .shell_surfaces import observed_status
from .touchpad import TouchDevice


def scroll_apps(applications):
    """Enough harmless installed apps to require scrolling the Settings page."""
    for index in range(20):
        (applications / f"hearthspace-scroll-{index:02}.desktop").write_text(
            "[Desktop Entry]\nType=Application\n"
            f"Name=Hearthspace Scroll Fixture {index:02}\nExec=true\n"
            "OnlyShowIn=Hearthspace;\n"
        )


@contextlib.contextmanager
def touchscreen(smoke, name):
    trace = smoke.artifacts / f"libinput-search-{name}.log"
    with (
        trace.open("w") as log,
        TouchDevice(
            smoke.directory, log, "touchscreen", deadline=smoke.control.deadline
        ) as device,
    ):
        yield device
    if Path(device.device["device"]).exists():
        raise ControlError("owned search touchscreen survived cleanup")
    if "TOUCH_DOWN" not in trace.read_text():
        raise ControlError("search acceptance lacks native touch events")


def stroke(device, output, start, end):
    device.request("begin", fingers=1, points=to_device([start], output))
    try:
        # The owned uinput worker already interpolates forty physical frames.
        device.request("move", points=to_device([end], output))
    finally:
        device.request("end")


def apps_corner(smoke):
    checks.closed(smoke)
    status = observed_status(smoke)
    output = status["outputs"][0]
    smoke.capture("apps-before-dock-swipe")
    search = smoke.until("Apps List icon", lambda: checks.elements(smoke, "Apps List"))[
        0
    ]
    start = button_center(search["bounds"], output, corner_surface(status, "apps"))
    before = smoke.control.status()["viewport"]
    with touchscreen(smoke, "apps-corner") as device:
        stroke(device, output, start, (start[0] - 90, start[1]))
        checks.closed(smoke)
        stroke(device, output, start, (start[0] + 5, start[1] - 110))
        checks.opened(smoke, "search")
        # Opening is idempotent; another upward swipe must not close the list.
        stroke(device, output, start, (start[0], start[1] - 110))
        checks.opened(smoke, "search")
        device.request("begin", fingers=1, points=to_device([start], output))
        device.request("end")
        checks.closed(smoke)
    after = smoke.control.status()["viewport"]
    if any(before[key] != after[key] for key in ("x", "y", "scale")):
        raise ControlError("Apps List swipe unexpectedly moved the canvas")
    smoke.capture("apps-dock-tap-restored")
    smoke.record("apps-corner-swipe", rejected_sideways=True, tap_preserved=True)


def settings_scroll(smoke):
    from .favorite_picker import dismiss, opened, selected
    from .favorite_ui import (
        settings_click,
        settings_elements,
        settings_surface,
    )

    saved = smoke.service_fixture.favorites.read_bytes()
    settings_click(smoke, "Add gesture")
    settings_click(smoke, "Choose app")
    original = smoke.until("selected gesture app", lambda: selected(smoke))
    settings_click(smoke, "Open this app", "combo box")
    with Input(smoke.control) as inputs:
        inputs.chord("ctrl+a")
        inputs.type("Hearthspace Scroll Fixture")
    name = "Hearthspace Scroll Fixture 00"
    row = smoke.until(
        "first scroll fixture", lambda: settings_elements(smoke, name, "list item")
    )[0]
    lower = smoke.until(
        "lower visible scroll fixture",
        lambda: settings_elements(smoke, "Hearthspace Scroll Fixture 04", "list item"),
    )[0]
    output = smoke.control.status()["outputs"][0]
    surface = settings_surface(smoke)
    start = button_center(lower["bounds"], output, surface)
    # Start on an app choice: scrolling must cancel its tap and preserve the draft app.
    end = (start[0], max(surface["y"] + 60, start[1] - 220))
    if start[1] - end[1] < 50:
        raise ControlError("Settings scroll fixture has insufficient drag room")
    smoke.capture("search-settings-before-touch-scroll")
    viewport = smoke.control.status()["viewport"]
    with touchscreen(smoke, "settings-scroll") as device:
        stroke(device, output, start, end)
        smoke.until(
            "Settings content scrolled by one finger",
            lambda: (
                not (rows := settings_elements(smoke, name, "list item"))
                or rows[0]["bounds"]["y"] < row["bounds"]["y"] - 30 * output["scale"]
            ),
        )
    if not opened(smoke):
        raise ControlError("scrolling an app choice unexpectedly closed the picker")
    if settings_surface(smoke) != surface:
        raise ControlError(
            "Settings scrolling unexpectedly moved or resized its window"
        )
    after = smoke.control.status()["viewport"]
    if any(viewport[key] != after[key] for key in ("x", "y", "scale")):
        raise ControlError("Settings scrolling unexpectedly moved the canvas")
    smoke.capture("search-settings-after-touch-scroll")
    smoke.record(
        "search-settings-touch-scroll",
        one_finger=True,
        tap_cancelled=True,
        selected_app_unchanged=original,
    )
    dismiss(smoke)
    settings_click(smoke, "Choose app")
    smoke.until(
        "selected app preserved after scrolling", lambda: selected(smoke) == original
    )
    dismiss(smoke)
    settings_click(smoke, "Back to gestures")
    if smoke.service_fixture.favorites.read_bytes() != saved:
        raise ControlError("browsing gesture apps unexpectedly changed saved shortcuts")


def recording_cancel(smoke):
    """Native Settings drawing remains a draft, including multi-touch cancel."""
    import time

    from .favorite_ui import (
        APP_NAME,
        l_shape,
        settings_click,
        settings_elements,
        settings_surface,
    )

    path = smoke.service_fixture.favorites
    saved = path.read_bytes()
    viewport = smoke.control.status()["viewport"]
    settings_click(smoke, f"Change gesture for {APP_NAME}")
    pad = smoke.until(
        "Settings touch drawing area",
        lambda: settings_elements(smoke, "Gesture drawing area"),
    )[0]
    surface = settings_surface(smoke)
    output = smoke.control.status()["outputs"][0]
    bounds, scale = pad["bounds"], output["scale"]
    rect = [
        surface["x"] + bounds["x"] / scale,
        surface["y"] + bounds["y"] / scale,
        bounds["width"] / scale,
        bounds["height"] / scale,
    ]
    points = l_shape(rect, scale=0.8, shifted=True)
    if settings_elements(smoke, "Save shortcut", "button"):
        raise ControlError("Settings enabled Save before a new touch recording")
    with touchscreen(smoke, "settings-recording") as device:
        device.request("begin", fingers=1, points=to_device(points[:1], output))
        try:
            for point in points[1:]:
                device.request("move", points=to_device([point], output))
            if settings_elements(smoke, "Save shortcut", "button"):
                raise ControlError("Settings completed a held touchscreen stroke")
        finally:
            device.request("end")
        # Disabled Masonry buttons are not focusable. Querying that state avoids
        # the upstream AccessKit AT-SPI enabled-flag issue used elsewhere here.
        smoke.until(
            "completed Settings touch shape and enabled Save",
            lambda: settings_elements(smoke, "Save shortcut", "button"),
        )
        current = settings_elements(smoke, "Gesture drawing area")[0]
        if current["bounds"] != bounds:
            raise ControlError("touch drawing scrolled the Settings page")
        smoke.capture("search-settings-touch-shape-ready")
        if path.read_bytes() != saved:
            raise ControlError("Settings persisted a touch shape before Save")

        settings_click(smoke, "Retry")
        smoke.until(
            "Settings retry disabled Save",
            lambda: not settings_elements(smoke, "Save shortcut", "button"),
        )
        device.request("begin", fingers=1, points=to_device(points[:1], output))
        try:
            device.request("move", points=to_device([points[1]], output))
            device.request(
                "add-finger",
                point=to_device([(points[0][0] + 24, points[0][1])], output)[0],
            )
        finally:
            device.request("end")
        # Give a wrongly emitted completion time to reach Xilem before asserting
        # absence; the successful stroke above proves this exact path is active.
        time.sleep(min(0.18, smoke.control.deadline.remaining()))
        if settings_elements(smoke, "Save shortcut", "button"):
            raise ControlError("second contact completed a Settings touch stroke")
        smoke.capture("search-settings-extra-contact-cancelled")
    settings_click(smoke, "Back to gestures")
    smoke.until(
        "Settings returned to gesture assignments",
        lambda: settings_elements(smoke, f"Change gesture for {APP_NAME}", "button"),
    )
    if path.read_bytes() != saved:
        raise ControlError("cancelling Settings touch recording changed its saved file")
    after = smoke.control.status()["viewport"]
    if any(viewport[key] != after[key] for key in ("x", "y", "scale")):
        raise ControlError("Settings touch recording unexpectedly moved the canvas")
    smoke.record(
        "search-settings-touch-recording",
        completed_on_release=True,
        extra_contact_cancelled=True,
        saved_file_unchanged=True,
    )
