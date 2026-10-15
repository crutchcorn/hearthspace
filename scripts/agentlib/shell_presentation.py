"""Popup acceptance from observed surface geometry, content, and composited pixels."""

import math

from . import shell_checks as checks
from .control import ControlError
from .input import Input
from .png import pixels
from .shell_backdrop import COLOR, backdrop
from .visual import pixel_rect


class Frame:
    def __init__(self, capture):
        self.metadata, png = capture
        self.width, self.height, self.channels, self.rows = pixels(png)
        if (self.width, self.height) != (
            self.metadata["width"],
            self.metadata["height"],
        ):
            raise ControlError("captured PNG dimensions disagree with metadata")

    def rgb(self, x, y):
        if not (0 <= x < self.width and 0 <= y < self.height):
            raise ControlError("observed shell pixels lie outside captured output")
        offset = x * self.channels
        return self.rows[y][offset : offset + 3]

    def rect(self, surface, bounds=None):
        scale = self.metadata["scale"]
        left, top, right, bottom = pixel_rect(
            self.metadata,
            (
                surface["x"],
                surface["y"],
                surface["x"] + surface["width"],
                surface["y"] + surface["height"],
            ),
        )
        if bounds is not None:
            # AccessKit bounds are physical and local to this surface.
            left, top = left + bounds["x"], top + bounds["y"]
            right, bottom = left + bounds["width"], top + bounds["height"]
        rect = math.floor(left), math.floor(top), math.ceil(right), math.ceil(bottom)
        if not (
            scale > 0
            and 0 <= rect[0] < rect[2] <= self.width
            and 0 <= rect[1] < rect[3] <= self.height
        ):
            raise ControlError("observed shell rectangle lies outside captured output")
        return rect


def difference(first, second):
    return max(abs(a - b) for a, b in zip(first, second))


def compatible(first, second):
    keys = ("width", "height", "scale", "logical_x", "logical_y")
    if any(first.metadata[key] != second.metadata[key] for key in keys):
        raise ControlError("shell screenshot output geometry changed during comparison")


def rounded(background, visible, surface):
    """Opaque rounded panels reveal the actual background at all four corners."""
    compatible(background, visible)
    left, top, right, bottom = visible.rect(surface)
    corners = []
    for x, dx in ((left, 1), (right - 1, -1)):
        for y, dy in ((top, 1), (bottom - 1, -1)):
            patch = [(x + dx * i, y + dy * j) for i in range(2) for j in range(2)]
            if any(difference(background.rgb(*p), visible.rgb(*p)) > 2 for p in patch):
                raise ControlError(
                    f"{surface['kind']} popup corner is opaque at {(x, y)}"
                )
            corners.append([x, y])
    # A missing or entirely transparent popup must not satisfy the corner test.
    inset = max(3, round(4 * visible.metadata["scale"]))
    inside = ((left + right) // 2, top + inset)
    if difference(background.rgb(*inside), visible.rgb(*inside)) <= 3:
        raise ControlError(f"{surface['kind']} popup has no visible interior")
    return corners


def highlight_change(before, after, surface, rows):
    """Both the old and new selected rows must visibly change, not just the caret."""
    compatible(before, after)
    fractions = []
    for row in rows:
        left, top, right, bottom = before.rect(surface, row["bounds"])
        changed = sum(
            difference(before.rgb(x, y), after.rgb(x, y)) > 3
            for y in range(top, bottom)
            for x in range(left, right)
        )
        fraction = changed / ((right - left) * (bottom - top))
        if fraction < 0.1:
            raise ControlError(
                f"search selection did not visibly change row {row['name']!r}"
            )
        fractions.append(fraction)
    return fractions


def bottom_gap(surface, element, scale):
    bounds = element.get("bounds")
    if not bounds or not all(
        math.isfinite(bounds[key]) for key in ("x", "y", "width", "height")
    ):
        raise ControlError("shell content has no finite accessible bounds")
    if scale <= 0 or bounds["width"] <= 0 or bounds["height"] <= 0:
        raise ControlError("shell content has empty bounds or invalid output scale")
    return surface["height"] - (bounds["y"] + bounds["height"]) / scale


def search_width(surface, output):
    width = output["logical_width"]
    expected = min(max(math.floor(width * 0.6 + 0.5), 360), max(width - 24, 1))
    if abs(surface["width"] - expected) > 1:
        raise ControlError(
            f"search width {surface['width']} does not match responsive width {expected}"
        )
    return expected


def fitted(smoke, kind, last_name, output):
    previous = None

    def settled():
        nonlocal previous
        surface = checks.panel(smoke, kind)
        elements = checks.elements(smoke, last_name, role=None)
        if not surface or not elements:
            return None
        gap = bottom_gap(surface, elements[0], output["scale"])
        current = surface, elements[0]["bounds"]
        stable, previous = current == previous, current
        return (surface, gap) if stable and 0 <= gap <= 24 else None

    return smoke.until(f"{kind} content fits settled popup height", settled)


def park_pointer(smoke, output):
    with Input(smoke.control) as inputs:
        inputs.motion(output["logical_x"] + 4, output["logical_y"] + 4)


def dark_background(frame, output):
    # Verify the actual committed backdrop, not merely its maximize state.
    for fx in (0.1, 0.5, 0.9):
        for fy in (0.1, 0.5, 0.9):
            x = round(fx * frame.width)
            y = round((48 + (output["logical_height"] - 48) * fy) * output["scale"])
            if difference(frame.rgb(x, y), COLOR) > 2:
                return False
    return True


def run(smoke):
    checks.bar(smoke)
    status = smoke.control.status()
    if status["windows"] or len(status["outputs"]) != 1:
        raise ControlError(
            "popup presentation acceptance needs an empty owned single-output session"
        )
    output = status["outputs"][0]
    with backdrop(smoke, output):
        panels(smoke, output)


def panels(smoke, output):
    park_pointer(smoke, output)

    def presented():
        frame = Frame(smoke.control.capture(output["name"]))
        return dark_background(frame, output)

    smoke.until("opaque dark popup backdrop presented", presented)
    background = Frame(smoke.capture("shell-popup-background", output["name"]))
    for kind, button, last_name in (
        ("search", "Search", "Type to search apps and open windows"),
        ("notifications", "Notifications", "No notifications yet"),
        ("power", "Power", "Log out"),
        ("map", "Canvas map", "Zoom in"),
    ):
        checks.click(smoke, button)
        checks.opened(smoke, kind)
        surface, gap = fitted(smoke, kind, last_name, output)
        if kind == "search":
            search_width(surface, output)
        park_pointer(smoke, output)
        visible = Frame(smoke.capture(f"shell-popup-{kind}", output["name"]))
        if checks.panel(smoke, kind) != surface:
            raise ControlError(f"{kind} popup changed geometry during capture")
        corners = rounded(background, visible, surface)
        smoke.record(
            "shell-popup-presentation",
            kind=kind,
            surface=surface,
            content_bottom_gap=gap,
            transparent_corners=corners,
        )
        with Input(smoke.control) as inputs:
            inputs.chord("esc")
        checks.closed(smoke)
