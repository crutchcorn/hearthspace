"""Assertions requiring both held selection semantics and visible live previews."""

from .control import ControlError
from .gesture_trace import gesture
from .window_preview_assertions import presented, preserved


def selection(status, target, *, active, focused=None):
    switcher = status["window_switcher"]
    if switcher["active"] != active:
        raise ControlError("window switcher has the wrong held/released lifetime")
    if active and switcher["selected_window_id"] != target:
        raise ControlError("window switcher selected the wrong client")
    expected_focus = target if focused is None else focused
    actual = [item["id"] for item in status["windows"] if item["focused"]]
    if actual != [expected_focus]:
        raise ControlError("switcher changed actual client focus unexpectedly")


def previews(status, capture, targets):
    # Require each marker inside its own preview, not merely colored chrome.
    return presented(status, capture, targets)


def scene(status):
    """Only app poses matter here; desktop items share the presentation scene."""
    windows = {window["id"] for window in status["windows"]}
    return {
        item["window_id"]: item["display"]
        for item in status["window_switcher"]["scene"]
        if item["window_id"] in windows
    }


def same_rect(first, second):
    return all(
        abs(first[key] - second[key]) <= 1 for key in ("x", "y", "width", "height")
    )


def window_rects(status):
    return {
        window["id"]: {key: window[key] for key in ("x", "y", "width", "height")}
        for window in status["windows"]
    }


def intermediate_motion(start, samples, end):
    interior = []
    for sample in samples:
        if not sample["window_switcher"]["animating"]:
            continue
        moved = {
            identity: rect
            for identity, rect in scene(sample).items()
            if identity in start
            and identity in end
            and not same_rect(start[identity], rect)
            and not same_rect(end[identity], rect)
        }
        if moved:
            interior.append(moved)
    if not interior:
        raise ControlError("switcher lacks observed intermediate live preview geometry")
    return {"intermediate_previews": interior, "start": start, "end": end}


def held_motion(before, samples, after):
    old_focus = [item["id"] for item in before["windows"] if item["focused"]]
    for state in [*samples, after]:
        preserved(before, state)
        if old_focus != [item["id"] for item in state["windows"] if item["focused"]]:
            raise ControlError("held switcher focused a preview before commit")
    order = after["window_switcher"]["window_ids"]
    for state in samples:
        if (
            state["window_switcher"]["active"]
            and state["window_switcher"]["window_ids"] != order
        ):
            raise ControlError("switcher changed MRU order while held")
    if before["window_switcher"]["active"]:
        start = scene(before)
        for state in [*samples, after]:
            current = scene(state)
            if start.keys() != current.keys() or any(
                not same_rect(rect, current[identity])
                for identity, rect in start.items()
            ):
                raise ControlError("cycling rearranged the held preview grid")
        return {"stationary_grid": start, "window_ids": order}
    return intermediate_motion(window_rects(before), samples, scene(after))


def committed(before, after, target):
    preserved(before, after, viewport=False)
    if before["viewport"]["scale"] != after["viewport"]["scale"]:
        raise ControlError("window switching changed the canvas zoom")
    selection(after, target, active=False)
    output = next(
        item
        for item in after["outputs"]
        if item["name"] == before["window_switcher"]["output"]
    )
    window = next(item for item in after["windows"] if item["id"] == target)
    for axis, extent in (("x", "width"), ("y", "height")):
        center = output[f"logical_{axis}"] + output[f"logical_{extent}"] / 2
        if abs(window[axis] + window[extent] / 2 - center) > 2:
            raise ControlError("switcher release did not center the selected client")


def released_motion(before, samples, after, target):
    committed(before, after, target)
    return intermediate_motion(scene(before), samples, window_rects(after))


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
