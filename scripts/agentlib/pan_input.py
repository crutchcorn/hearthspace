"""Inject owned translated contacts and retain independently observed deltas."""

from .control import ControlError
from .gesture_trace import gesture
from .input import Input
from .pinch_touchscreen import centroid, from_device, scaled, to_device
from .touchpad import checked_points


def begin(smoke, device, case, window):
    anchor = (window["x"] + window["width"] / 2, window["y"] + window["height"] / 2)
    with Input(smoke.control) as inputs:
        inputs.motion(*anchor)
    fingers = case.get("fingers", 3)
    if device.kind == "touchpad":
        device.request("begin", fingers=fingers, distance=900)
        return {"anchor": anchor, "fingers": fingers}
    points = checked_points(
        to_device(
            [
                (window["x"] + window["width"] * x, window["y"] + window["height"] * y)
                for x, y in ((0.3, 0.4), (0.7, 0.4), (0.5, 0.7))[:fingers]
            ],
            smoke.output,
        )
    )
    device.request("begin", fingers=fingers, points=points)
    return {
        "anchor": centroid(from_device(points, smoke.output)),
        "points": points,
        "fingers": fingers,
    }


def move(smoke, device, case, state):
    ratio = case.get("ratio", 1)
    dx, dy = case["delta"]
    if device.kind == "touchpad":
        if ratio != 1:
            # Establish libinput's pinch classification before translation makes
            # every contact travel together (which correctly starts a swipe).
            # Both phases belong to the same uninterrupted contact sequence.
            device.request("move", distance=720, translation=[0, 0])
        device.request("move", distance=900 * ratio, translation=[dx * 2, dy * 2])
        return
    raw = scaled(state["points"], ratio)
    moved = checked_points(
        [
            (
                x + dx * 2000 / smoke.output["logical_width"],
                y + dy * 1400 / smoke.output["logical_height"],
            )
            for x, y in raw
        ]
    )
    device.request("move", points=moved)
    state["moved"] = moved


def observed(smoke, device, case, state, path, *, ended=False):
    if device.kind == "touchpad":
        kind = "PINCH" if case.get("ratio", 1) != 1 else "SWIPE"
        return gesture(path.read_text(), kind, state["fingers"], require_end=ended)
    start, end = (from_device(state[key], smoke.output) for key in ("points", "moved"))
    old, new = centroid(start), centroid(end)

    def radius(points, center):
        return (
            sum(
                sum((axis - value) ** 2 for axis, value in zip(point, center))
                for point in points
            )
            / len(points)
        ) ** 0.5

    return {
        "translation": [b - a for a, b in zip(old, new)],
        "scale": radius(end, new) / radius(start, old),
    }


def cancel(device, state):
    extra = {"point": centroid(state["moved"])} if device.kind == "touchscreen" else {}
    device.request("add-finger", **extra)


def verify_trace(path, kind, case, state):
    trace = path.read_text()
    if kind == "touchpad":
        event = "PINCH" if case.get("ratio", 1) != 1 else "SWIPE"
        proof = gesture(trace, event, state["fingers"], require_end=True)
        if proof["cancelled"] != bool(case.get("cancel")):
            raise ControlError(
                "libinput cancellation does not match the injected gesture"
            )
        return proof
    for event in (
        "TOUCH_DOWN",
        "TOUCH_MOTION",
        "TOUCH_UP",
        "TOUCH_FRAME",
        "DEVICE_REMOVED",
    ):
        if event not in trace:
            raise ControlError(f"touchscreen trace lacks {event}")
    return {"raw_touch_frames": True}
