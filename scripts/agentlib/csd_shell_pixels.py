"""Require widget ink to disappear independently of its surrounding material."""

from statistics import median

from .control import ControlError
from .shell_presentation import difference


def _color(values):
    return tuple(median(channel) for channel in zip(*values))


def foreground_disappeared(before, after, surface, name):
    """Derive ink from the revealed widget alone, never from the claimed hide.

    Shell symbols/text occupy a minority of their live widget rectangle. Its
    median color estimates the backing; high-contrast enclosed components are
    ink. Exclude components touching the rectangle edge (rounded material and
    underlying client content), then use their strongest cores, not antialiasing.
    An unidentifiable foreground is insufficient evidence, not a passing hide.
    """
    left, top, right, bottom = before.rect(surface)
    colors = {
        (x, y): before.rgb(x, y) for y in range(top, bottom) for x in range(left, right)
    }
    backing = _color(colors.values())
    contrast = {point: difference(value, backing) for point, value in colors.items()}
    pending = {point for point, value in contrast.items() if value > 32}
    enclosed = set()
    while pending:
        seed = pending.pop()
        component, queue, edge = {seed}, [seed], False
        while queue:
            x, y = queue.pop()
            edge |= x in (left, right - 1) or y in (top, bottom - 1)
            for dx, dy in ((-1, 0), (1, 0), (0, -1), (0, 1)):
                neighbor = x + dx, y + dy
                if neighbor in pending:
                    pending.remove(neighbor)
                    component.add(neighbor)
                    queue.append(neighbor)
        if not edge:
            enclosed.update(component)
    strongest = max((contrast[point] for point in enclosed), default=0)
    ink = [point for point in enclosed if contrast[point] >= strongest * 0.75]
    background = [point for point, value in contrast.items() if value <= 12]
    if len(ink) < max(4, round(4 * before.metadata["scale"] ** 2)) or not background:
        raise ControlError(f"{name} shell region has no identifiable foreground")
    # Reuse the revealed backing mask on each row. A client header/content
    # boundary can cross the glyph; a whole-widget median mistakes that border
    # for retained ink. Identified ink never contributes to its backing estimate.
    backing_rows = {}
    for x, y in background:
        backing_rows.setdefault(y, []).append(after.rgb(x, y))
    if any(y not in backing_rows for _x, y in ink):
        raise ControlError(f"{name} shell foreground row has no backing samples")
    hidden_backing = {y: _color(values) for y, values in backing_rows.items()}
    disappeared = sum(
        difference(colors[point], after.rgb(*point)) > 8
        and difference(after.rgb(*point), hidden_backing[point[1]])
        <= max(16, contrast[point] * 0.25)
        for point in ink
    )
    fraction = disappeared / len(ink)
    # Both ink color and contrast must go away. The contrast condition also
    # rejects translucent glyphs that recompose over a disappearing backdrop.
    if fraction < 0.8:
        raise ControlError(
            f"focused view still presents the {name} shell region foreground: "
            f"only {disappeared}/{len(ink)} ink pixels disappeared"
        )
    return fraction
