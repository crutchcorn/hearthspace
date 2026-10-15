"""Require overview thumbnails, presented client pixels, and preserved layout."""

from .control import ControlError
from .pinch_assertions import same_viewport
from .png import pixels
from .visual import pixel_rect

COLORS = ((0, 255, 0), (255, 0, 0), (0, 0, 255))
LAYOUT_KEYS = (
    "canvas_x",
    "canvas_y",
    "content_width",
    "content_height",
    "maximized",
    "fullscreen",
    "presentation_output",
)


def preserved(before, after, *, viewport=True):
    if viewport and not same_viewport(before["viewport"], after["viewport"]):
        raise ControlError("overview changed the saved canvas viewport")
    old = {window["id"]: window for window in before["windows"]}
    new = {window["id"]: window for window in after["windows"]}
    if old.keys() != new.keys():
        raise ControlError("overview changed the set of live windows")
    for window_id, window in old.items():
        if any(window[key] != new[window_id][key] for key in LAYOUT_KEYS):
            raise ControlError("overview mutated window geometry or presentation state")


def marker_count(capture, bounds, color):
    metadata, png = capture
    width, height, channels, rows = pixels(png)
    left, top, right, bottom = pixel_rect(metadata, bounds)
    count = 0
    for y in range(max(0, int(top)), min(height, int(bottom))):
        row = rows[y]
        for x in range(max(0, int(left)), min(width, int(right))):
            value = row[x * channels : x * channels + 3]
            count += all(
                abs(actual - expected) < 40 for actual, expected in zip(value, color)
            )
    return count


def presented(status, capture, targets):
    overview = status["overview"]
    if not overview["active"]:
        raise ControlError("canvas overview is not active")
    items = {item["window_id"]: item for item in overview["items"]}
    if set(items) != {window["id"] for window in status["windows"] if window["mapped"]}:
        raise ControlError("overview omitted or duplicated a mapped window")
    if len(items) != len(overview["items"]):
        raise ControlError("overview duplicated a thumbnail")
    metadata = capture[0]
    if overview["output"] != metadata["output"]:
        raise ControlError("overview was captured on the wrong output")
    output = next(
        item for item in status["outputs"] if item["name"] == overview["output"]
    )
    rectangles = []
    proof = {}
    for window_id, color in targets.items():
        item = items[window_id]
        bounds = (
            item["x"],
            item["y"],
            item["x"] + item["width"],
            item["y"] + item["height"],
        )
        if (
            item["width"] <= 0
            or item["height"] <= 0
            or bounds[0] < output["logical_x"]
            or bounds[1] < output["logical_y"]
            or bounds[2] > output["logical_x"] + output["logical_width"]
            or bounds[3] > output["logical_y"] + output["logical_height"]
        ):
            raise ControlError("overview thumbnail lies outside its output")
        for other in rectangles:
            if (
                bounds[0] < other[2]
                and bounds[2] > other[0]
                and bounds[1] < other[3]
                and bounds[3] > other[1]
            ):
                raise ControlError("overview thumbnails overlap")
        rectangles.append(bounds)
        preview = (
            item["preview_x"],
            item["preview_y"],
            item["preview_x"] + item["preview_width"],
            item["preview_y"] + item["preview_height"],
        )
        if (
            preview[0] < bounds[0]
            or preview[1] < bounds[1]
            or preview[2] > bounds[2]
            or preview[3] > bounds[3]
            or preview[2] <= preview[0]
            or preview[3] <= preview[1]
        ):
            raise ControlError("overview client preview lies outside its card")
        count = marker_count(capture, preview, color)
        if count < 20:
            raise ControlError(
                f"overview thumbnail {window_id} lacks its presented client marker"
            )
        proof[str(window_id)] = {
            "bounds": bounds,
            "preview": preview,
            "marker_pixels": count,
            "color": color,
        }
    return proof
