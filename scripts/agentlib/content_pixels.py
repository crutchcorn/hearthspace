"""Rendered client text assertions shared across native acceptance workflows."""

from .control import ControlError
from .png import pixels
from .visual import pixel_rect


def content_changed(image, captured, bounds):
    """New editor text must repaint, beyond the area of a blinking caret."""
    before, after = image[0], captured[0]
    keys = ("width", "height", "scale", "logical_x", "logical_y")
    if any(before[key] != after[key] for key in keys):
        raise ControlError(
            "client screenshot output geometry changed during comparison"
        )
    width, height, channels, rows = pixels(image[1])
    new_width, new_height, new_channels, new_rows = pixels(captured[1])
    if (width, height) != (before["width"], before["height"]) or (
        new_width,
        new_height,
    ) != (after["width"], after["height"]):
        raise ControlError("captured PNG dimensions disagree with metadata")
    left, top, right, bottom = map(round, pixel_rect(before, bounds))
    minimum = 100 * before["scale"] ** 2
    changed = 0
    for y in range(max(0, top), min(height, bottom)):
        for x in range(max(0, left), min(width, right)):
            old = rows[y][x * channels : x * channels + 3]
            new = new_rows[y][x * new_channels : x * new_channels + 3]
            changed += max(abs(a - b) for a, b in zip(old, new)) > 25
            if changed > minimum:
                return changed
    raise ControlError("client text input did not repaint the editor")
