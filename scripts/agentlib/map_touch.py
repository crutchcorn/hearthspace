"""Optional owned native uinput touchscreen path for minimap viewport dragging."""

from pathlib import Path

from .control import ControlError
from .pinch_touchscreen import from_device, to_device
from .touchpad import TouchDevice


def drag(smoke, output, start, delta):
    path = smoke.artifacts / "libinput-map-drag-touchscreen.log"
    points = to_device([start], output)
    actual_start = from_device(points, output)[0]
    with (
        path.open("w") as log,
        TouchDevice(
            smoke.directory, log, "touchscreen", deadline=smoke.control.deadline
        ) as device,
    ):
        smoke.record("map-touchscreen-created", **device.device)
        device.request("begin", fingers=1, points=points)
        try:
            for fraction in (0.25, 0.5, 0.75, 1):
                destination = [start[axis] + delta[axis] * fraction for axis in (0, 1)]
                points = to_device([destination], output)
                device.request("move", points=points)
        finally:
            device.request("end")
    if Path(device.device["device"]).exists():
        raise ControlError("owned map touchscreen survived cleanup")
    trace = path.read_text()
    if any(
        event not in trace
        for event in ("TOUCH_DOWN", "TOUCH_MOTION", "TOUCH_UP", "TOUCH_FRAME")
    ):
        raise ControlError("minimap touch drag trace lacks real touchscreen events")
    smoke.record("map-touchscreen-removed", trace=str(path))
    actual_end = from_device(points, output)[0]
    return tuple(actual_end[axis] - actual_start[axis] for axis in (0, 1))
