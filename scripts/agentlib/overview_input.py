"""Four/five-contact input and independent libinput evidence for overview."""

from .control import ControlError
from .gesture_trace import gesture
from .pinch_touchscreen import centroid, scaled
from .touchpad import checked_points, contact_positions


def begin(device, fingers, *, outward=False, stagger=False):
    width = 600 if outward else 1000
    if device.kind == "touchpad":
        device.request("begin", fingers=fingers, distance=width)
        return {"fingers": fingers, "width": width}
    points = checked_points(contact_positions(width, fingers))
    if stagger:
        device.request("begin", fingers=1, points=points[:1])
        for point in points[1:]:
            device.request("add-finger", point=point)
    else:
        device.request("begin", fingers=fingers, points=points)
    return {"fingers": fingers, "points": points}


def move(device, state, ratio):
    if device.kind == "touchpad":
        device.request("move", distance=state["width"] * ratio)
    else:
        state["moved"] = checked_points(scaled(state["points"], ratio))
        device.request("move", points=state["moved"])


def cancel(device, state):
    if state["fingers"] != 4:
        raise ControlError("overview cancellation fixture requires four contacts")
    extra = {"point": centroid(state["moved"])} if device.kind == "touchscreen" else {}
    device.request("add-finger", **extra)


def trace_proof(path, kind, fingers, *, cancelled=False, ratio=None):
    trace = path.read_text()
    if kind == "touchpad":
        proof = gesture(trace, "PINCH", fingers, require_end=True)
        if proof["cancelled"] != cancelled:
            raise ControlError("overview input cancellation differs from libinput")
        if ratio is not None and abs(proof["scale"] - ratio) > 0.06:
            raise ControlError("overview pinch scale differs from libinput evidence")
        return proof
    for event in ("TOUCH_DOWN", "TOUCH_MOTION", "TOUCH_UP", "TOUCH_FRAME"):
        if event not in trace:
            raise ControlError(f"overview touchscreen trace lacks {event}")
    down_count = sum(" TOUCH_DOWN " in line for line in trace.splitlines())
    if down_count != fingers + int(cancelled):
        raise ControlError("overview touchscreen trace has the wrong contact count")
    return {"raw_touch_frames": True, "contacts": down_count}
