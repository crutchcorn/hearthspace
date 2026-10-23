"""Resolve drawing coordinates only after native layout and capture agree."""

from .control import ControlError
from .daily_checks import a11y
from .map_checks import SHELL


def drawing_rect(smoke):
    pad = f"{SHELL}//*[@role='canvas' and @name='Gesture drawing area']"
    content = f"{pad}/ancestor::*[@name='Apps and windows'][1]//*[@showing='true']"
    selector = f"{pad} | {pad}/ancestor::*[@role='frame'][1] | {content}"
    previous = None

    def snapshot():
        status = smoke.control.status()
        panels = [p for p in status.get("shell_panels", []) if p["kind"] == "search"]
        state = status.get("favorite_gesture", {})
        if not panels or not state.get("armed"):
            return None
        if len(panels) != 1 or len(status["outputs"]) != 1:
            raise ControlError("drawing acceptance needs one owned popup and output")
        output = status["outputs"][0]
        if panels[0]["output"] != output["name"] or state["output"] != output["name"]:
            raise ControlError("drawing capture and popup disagree on their output")
        return panels[0], state["generation"], state["bounds"], output["scale"]

    def ready():
        nonlocal previous
        before = snapshot()
        if before is None:
            previous = None
            return None
        elements = a11y(smoke, "find", selector)["elements"]
        if before != snapshot():
            previous = None
            return None
        surface, generation, bounds, scale = before
        frames = [e for e in elements if e["role"] == "Frame"]
        pads = [e for e in elements if e["role"] == "Canvas"]
        if len(frames) != 1 or len(pads) != 1:
            previous = None
            return None
        frame, native = frames[0]["bounds"], pads[0]["bounds"]
        fits = all(frame[axis] == surface[axis] * scale for axis in ("width", "height"))
        agrees = all(
            abs(native[axis] / scale - value) <= 1 / scale
            for axis, value in zip(("x", "y", "width", "height"), bounds)
        )
        # The new pad may already exist inside the old, taller Apps allocation.
        # Its real semantic footer must fit the measured popup before drawing.
        content_bottom = max(
            (e["bounds"]["y"] + e["bounds"]["height"]) / scale
            for e in elements
            if e["role"] != "Frame" and e.get("bounds")
        )
        if not fits or not agrees or not 0 <= surface["height"] - content_bottom <= 24:
            previous = None
            return None
        stable, previous = previous == before, before
        if not stable:
            return None
        smoke.record(
            "favorite-drawing-ready",
            surface=surface,
            generation=generation,
            bounds=bounds,
            native_canvas=native,
        )
        x, y, width, height = bounds
        return [surface["x"] + x, surface["y"] + y, width, height]

    return smoke.until("settled native gesture area and capture generation", ready)
