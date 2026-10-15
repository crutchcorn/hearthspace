"""Four/five-contact input and independent libinput evidence for overview."""

from .control import ControlError
from .gesture_trace import gesture
from .pinch_touchscreen import centroid, scaled
from .touchpad import checked_points, contact_positions


def begin(device, fingers, *, outward=False, stagger=False):
    # libinput tracks the highest/lowest contacts and requires both to move
    # 2.5/3 mm for four/five fingers. The highest contact moves width_delta/3
    # at 20 units/mm; leave enough spread for a recognized subthreshold pinch.
    width = 1000 if outward else 1600
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
        if ratio is not None:
            # libinput starts its scale baseline when motion is classified,
            # after the initial threshold. Assert the independently observed
            # gesture crosses (or deliberately avoids) Hearthspace's threshold.
            observed = proof["scale"]
            if ratio <= 0.8:
                matches = 0 < observed <= 0.8
            elif ratio < 1:
                matches = 0.8 < observed < 1
            elif ratio < 1.25:
                matches = 1 < observed < 1.25
            else:
                matches = observed >= 1.25
            if not matches:
                raise ControlError(
                    "overview pinch threshold differs from libinput evidence"
                )
            proof["requested_ratio"] = ratio
        return proof
    for event in ("TOUCH_DOWN", "TOUCH_MOTION", "TOUCH_UP", "TOUCH_FRAME"):
        if event not in trace:
            raise ControlError(f"overview touchscreen trace lacks {event}")
    down_count = sum(" TOUCH_DOWN " in line for line in trace.splitlines())
    if down_count != fingers + int(cancelled):
        raise ControlError("overview touchscreen trace has the wrong contact count")
    return {"raw_touch_frames": True, "contacts": down_count}
