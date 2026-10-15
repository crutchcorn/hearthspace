"""Rendered GTK content must follow a held touchscreen drag, not just geometry."""

import math

from .control import ControlError
from .shell_presentation import Frame, compatible, difference
from .visual import pixel_rect


def translated(before, after, image, captured):
    old, new = Frame(image), Frame(captured)
    compatible(old, new)
    scale = old.metadata["scale"]
    dx = round((after["x"] - before["x"]) * scale)
    dy = round((after["y"] - before["y"]) * scale)
    if max(abs(dx), abs(dy)) < 12:
        raise ControlError("GTK pixel proof needs a visible window displacement")
    left, top, right, bottom = map(
        round,
        pixel_rect(
            old.metadata,
            (
                before["x"] + 12,
                before["y"] + 65,
                before["x"] + before["width"] - 12,
                before["y"] + before["height"] - 12,
            ),
        ),
    )
    features = []
    stride = max(2, round(scale * 2))
    for y in range(top, bottom - 3, stride):
        for x in range(left, right - 3, stride):
            value = old.rgb(x, y)
            # Only distinctive edges count; a blank background cannot prove a
            # translation.
            if (
                max(
                    difference(value, old.rgb(x + 3, y)),
                    difference(value, old.rgb(x, y + 3)),
                )
                < 25
            ):
                continue
            features.append((x, y, value))
    # Status rounds coordinates to logical pixels, while rendering rounds the
    # transformed surface origin separately. On a scale-2 output these can
    # differ by two physical pixels. Find one coherent translation for all
    # content, bounded to one output-logical pixel around the reported delta;
    # independently matching each feature to a neighbor could hide distortion.
    radius = math.ceil(scale)
    candidates = [
        (ox, oy)
        for ox in range(-radius, radius + 1)
        for oy in range(-radius, radius + 1)
    ]
    candidates.sort(key=lambda offset: abs(offset[0]) + abs(offset[1]))
    matched, adjustment = -1, (0, 0)
    for ox, oy in candidates:
        count = sum(
            difference(value, new.rgb(x + dx + ox, y + dy + oy)) <= 20
            for x, y, value in features
        )
        if count > matched:
            matched, adjustment = count, (ox, oy)
        if count == len(features):
            break
    dx, dy = dx + adjustment[0], dy + adjustment[1]
    total = len(features)
    fraction = matched / total if total else 0
    if total < 30 or fraction < 0.78:
        raise ControlError(
            f"GTK content did not follow its window: {matched}/{total} edge samples"
        )
    # The portion vacated by the drag must also be repainted. This rules out a
    # second stale copy remaining at the old position.
    x1, y1, x2, y2 = map(round, old.rect(before))
    vacated = changed = 0
    for y in range(y1 + 10, y2 - 10, stride):
        for x in range(x1 + 10, x2 - 10, stride):
            if x1 + dx <= x < x2 + dx and y1 + dy <= y < y2 + dy:
                continue
            vacated += 1
            changed += difference(old.rgb(x, y), new.rgb(x, y)) > 20
    if not vacated or changed / vacated < 0.5:
        raise ControlError("GTK drag left stale pixels in its vacated region")
    return {
        "content_edge_samples": total,
        "translated_fraction": fraction,
        "rendered_pixel_delta": [dx, dy],
        "geometry_rounding_adjustment": list(adjustment),
        "vacated_changed_fraction": changed / vacated,
    }
