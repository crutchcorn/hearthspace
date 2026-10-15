"""Semantic and rendered assertions shared by native pinch fixtures."""

import re

from .control import ControlError
from .visual import marker_bounds


def verify_zoom(
    before,
    after,
    anchor,
    before_window,
    after_window,
    before_image,
    after_image,
    *,
    translation=(0, 0),
):
    ratio = after["scale"] / before["scale"]
    for axis, coordinate, delta in zip(("x", "y"), anchor, translation):
        old_canvas = coordinate / before["scale"] + before[axis]
        new_canvas = (coordinate + delta) / after["scale"] + after[axis]
        if abs(new_canvas - old_canvas) > 1.1:
            raise ControlError(
                "gesture anchor did not follow the observed contact translation"
            )
    if any(
        abs(after_window[key] - before_window[key] * ratio) > 2
        for key in ("width", "height")
    ):
        raise ControlError("window geometry did not follow the pinch scale")
    for axis, coordinate, delta in zip(("x", "y"), anchor, translation):
        expected = (before_window[axis] - coordinate) * ratio + coordinate + delta
        if abs(after_window[axis] - expected) > 2:
            raise ControlError("window geometry did not follow gesture translation")
    old_bounds = marker_bounds(before_image, before_window)
    new_bounds = marker_bounds(after_image, after_window)
    for start, end in ((0, 2), (1, 3)):
        old_size = (old_bounds[end] - old_bounds[start]) / before_image[0]["scale"]
        new_size = (new_bounds[end] - new_bounds[start]) / after_image[0]["scale"]
        if abs(new_size - old_size * ratio) > 2:
            raise ControlError(
                "presented terminal marker did not scale with the viewport"
            )
    for index, (old, new) in enumerate(zip(old_bounds, new_bounds)):
        axis = index % 2
        origin = "logical_x" if axis == 0 else "logical_y"
        old_logical = old / before_image[0]["scale"] + before_image[0][origin]
        new_logical = new / after_image[0]["scale"] + after_image[0][origin]
        expected = (
            (old_logical - anchor[axis]) * ratio + anchor[axis] + translation[axis]
        )
        if abs(new_logical - expected) > 2:
            raise ControlError("presented marker did not follow gesture translation")
    return {
        "scale_ratio": ratio,
        "before_marker": old_bounds,
        "after_marker": new_bounds,
        "translation": translation,
    }


def same_viewport(first, second):
    return all(abs(first[key] - second[key]) < 1e-8 for key in ("x", "y", "scale"))


def verify_removal(trace, before, active, after):
    events = re.findall(
        r"^\s*(\S+)\s+(GESTURE_PINCH_BEGIN|GESTURE_PINCH_END|DEVICE_REMOVED)\b([^\n]*)$",
        trace,
        re.MULTILINE,
    )
    final = events[-3:]
    if (
        [event[1] for event in final]
        != ["GESTURE_PINCH_BEGIN", "GESTURE_PINCH_END", "DEVICE_REMOVED"]
        or len({event[0] for event in final}) != 1
        or not re.fullmatch(r"\s+\+\S+s\s+3\s*", final[0][2])
    ):
        raise ControlError(
            "removal trace requires the final three-finger BEGIN, END, then DEVICE_REMOVED"
        )
    end = re.fullmatch(r"\s+\+\S+s\s+3(?:\s+(cancelled))?\s*", final[1][2])
    if end is None:
        raise ControlError("removal trace has an unrecognized final pinch END")
    cancelled = end.group(1) is not None
    expected = {**(before if cancelled else active), "pinch_active": False}
    if after["pinch_active"] or not same_viewport(expected, after):
        raise ControlError(
            "viewport after removal does not match the observed final pinch END"
        )
    return {"end_cancelled": cancelled, "expected": expected, "final_events": final}


def verify_touch_removal(trace, before, active, after):
    events = re.findall(
        r"^\s*(\S+)\s+(TOUCH_DOWN|TOUCH_UP|TOUCH_CANCEL|DEVICE_REMOVED)\b([^\n]*)$",
        trace,
        re.MULTILINE,
    )
    downs = [index for index, event in enumerate(events) if event[1] == "TOUCH_DOWN"]
    if len(downs) < 3:
        raise ControlError("touchscreen removal trace lacks three contacts")
    final = events[downs[-3] :]
    kinds = [event[1] for event in final]
    if (
        kinds[:3] != ["TOUCH_DOWN"] * 3
        or kinds[-1] != "DEVICE_REMOVED"
        or "DEVICE_REMOVED" in kinds[:-1]
        or len({event[0] for event in final}) != 1
    ):
        raise ControlError("touchscreen removal trace has missing or misordered events")
    ending = kinds[3]  # The first lift completes; cancellation/removal rolls back.
    expected = {**(active if ending == "TOUCH_UP" else before), "pinch_active": False}
    if after["pinch_active"] or not same_viewport(expected, after):
        raise ControlError(
            "viewport after touchscreen removal disagrees with its first ending event"
        )
    return {"first_ending": ending, "expected": expected, "final_events": final}
