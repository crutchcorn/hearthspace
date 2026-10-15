"""Assertions requiring both held selection semantics and visible live previews."""

from .control import ControlError
from .gesture_trace import gesture
from .overview_assertions import presented
from .pinch_assertions import same_viewport


def selection(status, target, *, active):
    switcher = status["window_switcher"]
    if switcher["active"] != active:
        raise ControlError("window switcher has the wrong held/released lifetime")
    if active and switcher["selected_window_id"] != target:
        raise ControlError("window switcher selected the wrong client")
    focused = [item["id"] for item in status["windows"] if item["focused"]]
    if focused != [target]:
        raise ControlError("switcher selection and actual client focus differ")
    if status["overview"]["active"]:
        raise ControlError("horizontal switching accidentally opened overview")


def previews(status, capture, targets):
    # Both overlays publish the same live-preview geometry. Reuse pixel proof,
    # requiring each marker inside its own preview, not merely colored chrome.
    return presented(
        {**status, "overview": status["window_switcher"]}, capture, targets
    )


def smooth_pan(before, samples, after):
    old, new = before["viewport"], after["viewport"]
    if same_viewport(old, new):
        raise ControlError("window switching never panned to the off-canvas client")
    if old["scale"] != new["scale"]:
        raise ControlError("window switching changed the canvas zoom")
    interior = []
    for sample in samples:
        view = sample["viewport"]
        between = all(
            min(old[axis], new[axis]) <= view[axis] <= max(old[axis], new[axis])
            for axis in ("x", "y")
        )
        if (
            between
            and view["animating"]
            and not same_viewport(old, view)
            and not same_viewport(new, view)
        ):
            interior.append(view)
    if not interior:
        raise ControlError("switcher pan lacks an observed intermediate animated view")
    return {"intermediate_views": interior, "start": old, "end": new}


def input_proof(path, kind, fingers, direction):
    trace = path.read_text()
    if kind == "touchpad":
        proof = gesture(trace, "SWIPE", fingers, require_end=True)
        dx, dy = proof["translation"]
        if proof["cancelled"] or dx * direction <= 0 or not 32 <= abs(dx) < 128:
            raise ControlError(
                "independent swipe input did not describe one intended step"
            )
        if abs(dy) > abs(dx) / 1.5:
            raise ControlError("independent swipe input was not horizontal")
        return proof
    for event in (
        "TOUCH_DOWN",
        "TOUCH_MOTION",
        "TOUCH_UP",
        "TOUCH_FRAME",
        "DEVICE_REMOVED",
    ):
        if event not in trace:
            raise ControlError(f"window switcher touch trace lacks {event}")
    contacts = sum(" TOUCH_DOWN " in line for line in trace.splitlines())
    if contacts != fingers:
        raise ControlError("window switcher trace has the wrong contact count")
    return {"raw_touch_frames": True, "contacts": contacts}
