"""Observe the live shell reveal/hide at its reported surface rectangles."""

import time

from . import csd_shell_regions
from .control import ControlError
from .csd_shell_pixels import foreground_disappeared
from .daily_checks import a11y
from .input import Input
from .shell_presentation import Frame, compatible, difference

SHELL = "//*[@role='application' and @name='hearthspace-shell']"


def hidden_regions(revealed, hidden, surfaces, *, initial=False):
    before, after = Frame(revealed), Frame(hidden)
    compatible(before, after)
    earlier, later = (after, before) if initial else (before, after)
    if (
        before.metadata["output"] != after.metadata["output"]
        or later.metadata["frame_id"] <= earlier.metadata["frame_id"]
    ):
        raise ControlError("shell visibility needs a fresh frame from the same output")
    changes = {}
    for name, surface in surfaces.items():
        left, top, right, bottom = before.rect(surface)
        changed = sum(
            difference(before.rgb(x, y), after.rgb(x, y)) > 8
            for y in range(top, bottom)
            for x in range(left, right)
        )
        fraction = changed / ((right - left) * (bottom - top))
        # Test whole observed shell regions. A cursor, caret or clock tick must
        # not stand in for independently placed controls disappearing.
        if fraction < 0.05:
            raise ControlError(f"focused view still presents the {name} shell region")
        if name.startswith("system-"):
            foreground_disappeared(before, after, surface, name)
        changes[name] = fraction
    if not changes:
        raise ControlError("shell visibility evidence has no observed regions")
    return changes


def park_before_entry(smoke, output=None):
    """End any existing hover before the action that enters focused view."""
    if output is None:
        outputs = smoke.control.status()["outputs"]
        if len(outputs) != 1:
            raise ControlError("focused shell acceptance requires one active output")
        output = outputs[0]
    with Input(smoke.control) as inputs:
        inputs.motion(
            output["logical_x"] + output["logical_width"] / 2,
            output["logical_y"] + output["logical_height"] / 2,
        )
    # Only passive waiting is permitted after entry. This timestamp accounts for
    # the compositor's existing 450ms hover grace without correcting its state.
    smoke.csd_shell_grace_until = time.monotonic() + 0.45


def contact_before_entry(smoke, location, touch):
    """Record an unavoidable titlebar contact in the output's reveal corner."""
    output = smoke.output
    x = location[0] - output["logical_x"]
    y = location[1] - output["logical_y"]
    # These are the existing CORNER_DISTANCE/TOUCH_GRACE values in
    # focused_shell_peek.rs, not guessed control positions or added timeouts.
    if (x <= 72 or x >= output["logical_width"] - 72) and (
        y <= 72 or y >= output["logical_height"] - 72
    ):
        smoke.csd_shell_grace_until = time.monotonic() + (2.5 if touch else 0.45)


def unchanged_client(smoke, before):
    after = smoke.window(smoke.target)
    fields = ("focused", "focused_view", "fullscreen", "x", "y", "width", "height")
    if not after or any(before.get(key) != after.get(key) for key in fields):
        raise ControlError("shell visibility probe changed the focused client")


def shell_status_hidden(status):
    return not any(
        status.get(key) for key in ("shell_corners", "shell_panels", "canvas_maps")
    )


def focused_hidden(smoke, label):
    status = smoke.control.status()
    outputs = status["outputs"]
    if len(outputs) != 1:
        raise ControlError("focused shell acceptance requires one active output")
    output = outputs[0]
    before = smoke.window(smoke.target)
    if (
        not before
        or not before.get("focused")
        or not before.get("focused_view")
        or before.get("fullscreen")
    ):
        raise ControlError("shell hide evidence requires non-fullscreen focused view")
    # Keep this before ALL input, including application-content checks. A reveal
    # followed by a motion away can repair an initially visible shell regression.
    initial = smoke.capture(f"{label}-shell-entry")
    entry_status = smoke.control.status()
    unchanged_client(smoke, before)
    smoke.record("csd-shell-entry", name=label, status=entry_status)
    grace = max(0, getattr(smoke, "csd_shell_grace_until", 0) - time.monotonic())
    initial_status = entry_status
    if grace:
        time.sleep(min(grace, smoke.control.deadline.remaining()))
        smoke.control.deadline.remaining()
        initial = smoke.capture(f"{label}-shell-entry-after-grace")
        initial_status = smoke.control.status()
        unchanged_client(smoke, before)
        smoke.record(
            "csd-shell-entry-after-grace",
            name=label,
            grace_seconds=grace,
            status=initial_status,
        )
    if not shell_status_hidden(initial_status):
        raise ControlError("initial focused entry still exposes shell surfaces")
    with Input(smoke.control) as inputs:
        # Reveal without clicking or changing focus. Coordinates are the live
        # output corner, not a guessed location of any shell control.
        inputs.motion(
            output["logical_x"] + 1, output["logical_y"] + output["logical_height"] - 1
        )
        try:
            controls = smoke.until(
                "live shell controls for focused visibility evidence",
                lambda: a11y(smoke, "find", f"{SHELL}//*[@name='Canvas map']")[
                    "elements"
                ],
            )
            surfaces = csd_shell_regions.surfaces(smoke)
            revealed = smoke.capture(f"{label}-shell-revealed")
            unchanged_client(smoke, before)
        finally:
            inputs.motion(
                output["logical_x"] + output["logical_width"] / 2,
                output["logical_y"] + output["logical_height"] / 2,
            )
    # An initially visible shell cannot pass just because moving out of the
    # deliberate reveal corner subsequently made it hide.
    try:
        initial_proof = hidden_regions(revealed, initial, surfaces, initial=True)
    except ControlError as error:
        raise ControlError(f"initial focused shell was not hidden: {error}") from error

    last_error = None

    def hidden():
        nonlocal last_error
        image = smoke.capture(f"{label}-shell-hidden")
        unchanged_client(smoke, before)
        try:
            proof = hidden_regions(revealed, image, surfaces)
        except ControlError as error:
            last_error = error
            return None
        status = smoke.control.status()
        if not shell_status_hidden(status):
            return None
        return proof

    try:
        proof = smoke.until("focused client hides every observed shell corner", hidden)
    except ControlError as error:
        raise ControlError(f"{error}; shell visibility: {last_error}") from error
    smoke.record(
        "csd-shell-hidden",
        name=label,
        controls=controls,
        surfaces=surfaces,
        initial_changed_fractions=initial_proof,
        changed_fractions=proof,
    )
