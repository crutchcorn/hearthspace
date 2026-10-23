"""Focused Firefox retains usable, rendered browser chrome above its document."""

import math

from . import firefox_assertions as verify
from .client_accessibility import bus, call, local_center
from .content_pixels import content_changed
from .control import ControlError
from .csd_shell import focused_hidden
from .daily_checks import a11y
from .input import Input

FIREFOX = "//*[@role='application' and contains(@name,'Firefox')]"


def chrome(smoke):
    address = bus(smoke)

    def controls():
        restore = a11y(
            smoke,
            "find",
            f"{FIREFOX}//*[@role='button' and @showing='true' and @name='Restore Down']",
        )["elements"]
        entries = a11y(
            smoke, "find", f"{FIREFOX}//*[@showing='true' and @editable='true']"
        )["elements"]
        if len(entries) != 1:
            return None
        window = smoke.window(smoke.target)
        if not window:
            return None
        contained = []
        for node in restore:
            # Firefox may expose a clipped duplicate as showing. Query current
            # WINDOW-relative geometry; snapshot bounds are not physical proof.
            bounds = call(
                smoke,
                address,
                node["bus_name"],
                node["object_path"],
                "org.a11y.atspi.Component.GetExtents",
                "1",
            )
            if (
                len(bounds) == 4
                and all(math.isfinite(value) for value in bounds)
                and 0 <= bounds[0] < bounds[0] + bounds[2] <= window["content_width"]
                and 0 <= bounds[1] < bounds[1] + bounds[3] <= window["content_height"]
            ):
                contained.append((node, bounds))
        if len(contained) != 1:
            return None
        node, bounds = contained[0]
        return node, bounds, entries[0]

    restore, restore_bounds, entry = smoke.until(
        "retained Firefox restore control and address bar", controls
    )

    def text():
        return call(
            smoke,
            address,
            entry["bus_name"],
            entry["object_path"],
            "org.a11y.atspi.Text.GetText",
            "0",
            "-1",
        )

    original = text()
    with Input(smoke.control) as inputs:
        try:
            inputs.chord("ctrl+l")
            inputs.type("iiiiiiiiiiiiiiii CSD chrome")
            smoke.until(
                "Firefox chrome receives baseline text",
                lambda: text() == "iiiiiiiiiiiiiiii CSD chrome",
            )
            image = smoke.capture("csd-focused-chrome-before")
            bounds = call(
                smoke,
                address,
                entry["bus_name"],
                entry["object_path"],
                "org.a11y.atspi.Component.GetExtents",
                "1",
            )
            window = smoke.window(smoke.target)
            center = local_center(bounds, window, 1)
            region = (
                center[0] - bounds[2] / 2,
                center[1] - bounds[3] / 2,
                center[0] + bounds[2] / 2,
                center[1] + bounds[3] / 2,
            )
            inputs.chord("ctrl+a")
            inputs.type("MMMMMMMMMMMMMMMM CSD chrome")
            smoke.until(
                "Firefox chrome receives new text",
                lambda: text() == "MMMMMMMMMMMMMMMM CSD chrome",
            )

            def rendered():
                captured = smoke.capture("csd-focused-chrome-after")
                try:
                    return content_changed(image, captured, region)
                except ControlError:
                    return None

            changed = smoke.until(
                "Firefox retained chrome repaints its new text", rendered
            )
        finally:
            # Never submit or navigate away from the owned local marker page.
            inputs.chord("esc")
            inputs.chord("esc")
    smoke.until(
        "Firefox address bar restores the local fixture URL", lambda: text() == original
    )
    status = smoke.control.status()
    if not verify.focused_at_output(smoke.window(smoke.target), status["outputs"]):
        raise ControlError("Firefox chrome interaction changed focused presentation")
    smoke.record(
        "csd-focused-client-chrome",
        restore=restore,
        restore_bounds=restore_bounds,
        entry=entry,
        bounds=bounds,
        changed_pixels=changed,
    )


def evidence(smoke):
    focused_hidden(smoke, "firefox-focused")
    chrome(smoke)
