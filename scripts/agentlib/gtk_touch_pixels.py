"""Rendered GTK content must follow a held touchscreen drag, not just geometry."""

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
    matched = total = 0
    stride = max(2, round(scale * 2))
    for y in range(top, bottom - 3, stride):
        for x in range(left, right - 3, stride):
            value = old.rgb(x, y)
            # Only distinctive edges count; a blank background cannot prove a
            # translation. A one-pixel neighborhood tolerates canvas rounding.
            if (
                max(
                    difference(value, old.rgb(x + 3, y)),
                    difference(value, old.rgb(x, y + 3)),
                )
                < 25
            ):
                continue
            total += 1
            matched += any(
                difference(value, new.rgb(x + dx + ox, y + dy + oy)) <= 20
                for ox in (-1, 0, 1)
                for oy in (-1, 0, 1)
            )
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
        "vacated_changed_fraction": changed / vacated,
    }
