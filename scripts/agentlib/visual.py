"""Targeted native-smoke assertions in captured pixel coordinates."""

from .control import ControlError, save_capture
from .input import Input
from .png import crop, pixels


def pixel_rect(metadata, bounds):
    x1, y1, x2, y2 = bounds
    scale = metadata["scale"]
    return (
        (x1 - metadata["logical_x"]) * scale,
        (y1 - metadata["logical_y"]) * scale,
        (x2 - metadata["logical_x"]) * scale,
        (y2 - metadata["logical_y"]) * scale,
    )


def marker_bounds(capture, window):
    metadata, png = capture
    width, height, channels, rows = pixels(png)
    content = pixel_rect(
        metadata,
        (
            window["x"],
            window["y"] + window.get("titlebar_height", 0),
            window["x"] + window["width"],
            window["y"] + window["height"],
        ),
    )
    left, top, right, bottom = width, height, -1, -1
    # Ignore wallpaper and other windows; the terminal is opaque.
    for y in range(max(0, int(content[1])), min(height, int(content[3]))):
        row = rows[y]
        for x in range(max(0, int(content[0])), min(width, int(content[2]))):
            r, g, b = row[x * channels : x * channels + 3]
            if r < 40 and g > 220 and b < 40:
                left, top = min(left, x), min(top, y)
                right, bottom = max(right, x), max(bottom, y)
    bounds = (left, top, right + 1, bottom + 1)
    if (
        right < left
        or bottom < top
        or left < content[0]
        or top < content[1]
        or right >= content[2]
        or bottom >= content[3]
    ):
        raise ControlError(
            "visible marker is missing or outside the rendered client bounds"
        )
    return bounds


def verify_move(before_capture, after_capture, before_window, after_window):
    before = marker_bounds(before_capture, before_window)
    after = marker_bounds(after_capture, after_window)
    before_metadata, after_metadata = before_capture[0], after_capture[0]
    # Convert both landmarks independently so output origins/scales are explicit.
    for axis, index in (("x", 0), ("y", 1)):
        old = (
            before[index] / before_metadata["scale"]
            + before_metadata[f"logical_{axis}"]
        )
        new = after[index] / after_metadata["scale"] + after_metadata[f"logical_{axis}"]
        expected = after_window[axis] - before_window[axis]
        if abs((new - old) - expected) > 1:
            raise ControlError("captured marker did not follow the window move")
    return after


def verify_resize(
    background, before_capture, after_capture, before_window, after_window
):
    marker = marker_bounds(after_capture, after_window)
    # The smoke shrinks the right edge. Its vacated strip must reveal the exact
    # background captured before launch; a stale image cannot pass this check.
    old_right = before_window["x"] + before_window["width"]
    new_right = after_window["x"] + after_window["width"]
    left, right = max(new_right + 8, old_right - 24), old_right - 8
    y = after_window["y"] + after_window["height"] / 2
    if left >= right:
        raise ControlError("resize did not expose a testable background strip")
    bounds = (left, y, right, y + 16)
    regions = [
        crop(image, pixel_rect(metadata, bounds))
        for metadata, image in (background, before_capture, after_capture)
    ]
    if not regions[0] or regions[0] == regions[1] or regions[2] != regions[0]:
        raise ControlError("resized window did not reveal the expected background")
    return {"marker_bounds": marker, "vacated_logical_region": bounds}


def verify_cursor_motion(before, after, x, y, distance=70):
    for metadata, _ in (before, after):
        if (
            metadata.get("cursor") != "embedded"
            or metadata.get("cursor_source") != "framebuffer"
        ):
            raise ControlError("terminal cursor must be included in the framebuffer")
    regions = []
    for position in (x, x + distance):
        bounds = (position - 12, y - 20, position + 20, y + 24)
        old = crop(before[1], pixel_rect(before[0], bounds))
        new = crop(after[1], pixel_rect(after[0], bounds))
        if not old or old == new:
            raise ControlError(
                "software cursor motion did not change both cursor regions"
            )
        regions.append(bounds)
    return regions


def software_cursor(smoke, window, output):
    x = window["x"] + window["width"] / 2
    y = window["y"] + window["height"] - 80
    with Input(smoke.control) as inputs:
        inputs.motion(x, y)

    def client_cursor():
        capture = smoke.control.capture(output)
        return capture if capture[0].get("cursor_source") == "framebuffer" else None

    first = smoke.until("terminal software cursor", client_cursor)
    smoke.record(
        "software-cursor-first",
        **save_capture(first, smoke.artifacts / "software-cursor-first.png"),
    )
    with Input(smoke.control) as inputs:
        inputs.motion(x + 70, y)
    second = smoke.capture("software-cursor-second", output)
    regions = verify_cursor_motion(first, second, x, y)
    smoke.record(
        "software-cursor-inclusion",
        checked_logical_regions=regions,
        policy="embedded",
        cursor_source="framebuffer",
    )


def move_resize(smoke, window, visible, initial):
    titlebar = window.get("titlebar_height", 28)
    if titlebar <= 0:
        raise ControlError("foot needs compositor decorations for native drag test")
    x = window["x"] + window["width"] / 2
    y = window["y"] + titlebar / 2
    with Input(smoke.control) as inputs:
        inputs.drag(x, y, x - 48, y + 36)
    moved = smoke.until("window move", lambda: smoke.changed(window, ("x", "y")))
    smoke.record("move", before=window, after=moved)
    moved_capture = smoke.capture("moved", initial[0]["output"])
    landmark = verify_move(visible, moved_capture, window, moved)
    smoke.record("move-visible", marker_bounds=landmark)
    x, y = moved["x"] + moved["width"] - 2, moved["y"] + moved["height"] - 2
    with Input(smoke.control) as inputs:
        inputs.drag(x, y, x - 80, y - 48)
    resized = smoke.until(
        "window resize", lambda: smoke.changed(moved, ("width", "height"))
    )
    smoke.record("resize", before=moved, after=resized)
    resized_capture = smoke.capture("resized", initial[0]["output"])
    evidence = verify_resize(initial, moved_capture, resized_capture, moved, resized)
    smoke.record("resize-visible", **evidence)
    return resized
