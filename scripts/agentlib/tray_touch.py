"""Verify tray menu taps through the owned native kernel touchscreen path."""

from pathlib import Path

from . import shell_checks, tray_checks
from .control import ControlError
from .pinch_touchscreen import to_device
from .shell_presentation import park_pointer
from .touchpad import TouchDevice


def run(smoke, peer):
    status = smoke.control.status()
    if status["backend"] != "native":
        return
    output = status["outputs"][0]
    park_pointer(smoke, output)
    path = smoke.artifacts / "libinput-tray-touchscreen.log"
    with (
        path.open("w") as log,
        TouchDevice(
            smoke.directory, log, "touchscreen", deadline=smoke.control.deadline
        ) as device,
    ):
        smoke.record("tray-touchscreen-created", **device.device)

        def tap(name, *, menu=False, hold=False):
            point = tray_checks.point(smoke, name, menu=menu)
            device.request("begin", fingers=1, points=to_device([point], output))
            try:
                # Wait for a real presentation while the contact is held.
                if hold:
                    smoke.until(
                        "touch hold opens app menu",
                        lambda: shell_checks.panel(smoke, "tray"),
                    )
                else:
                    smoke.control.capture(output["name"])
            finally:
                device.request("end")

        before = len(peer.events())
        tap(tray_checks.TITLE)
        peer.expect("activate", after=before)
        tap(tray_checks.TITLE, hold=True)
        shell_checks.opened(smoke, "tray")
        smoke.until(
            "touch opens application menu",
            lambda: tray_checks.visible(smoke, "Open fixture"),
        )
        tray_checks.snapshot(smoke, "tray-touch-menu")
        before = len(peer.events())
        tap("Open fixture", menu=True)
        peer.expect("menu", after=before, id=1, event="clicked")
        shell_checks.closed(smoke)
    if Path(device.device["device"]).exists():
        raise ControlError("owned tray touchscreen survived cleanup")
    trace = path.read_text()
    if any(event not in trace for event in ("TOUCH_DOWN", "TOUCH_UP", "TOUCH_FRAME")):
        raise ControlError("tray taps lack real kernel touchscreen evidence")
    smoke.record("tray-touchscreen-verified", trace=str(path), removed=True)
