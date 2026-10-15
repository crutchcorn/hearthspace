"""Assert tray app delivery, menu semantics, and actual icon presentation."""

from . import shell_checks as checks
from .control import ControlError
from .daily_checks import a11y
from .input import Input
from .map_checks import SHELL, button_center
from .shell_presentation import Frame, park_pointer

TITLE = "Hearthspace Tray Fixture"


def visible(smoke, name=TITLE, *, suffix=""):
    target = f"{SHELL}//*[@showing='true' and @name='{name}']"
    return a11y(smoke, "find", target + suffix)["elements"]


def icon(smoke):
    icons = smoke.until("visible fixture tray icon", lambda: visible(smoke))
    if len(icons) != 1:
        raise ControlError("fixture tray icon is ambiguous")
    return icons[0]


def disappeared(smoke):
    smoke.until("tray fixture removed", lambda: not visible(smoke))


def point(smoke, name=TITLE, *, menu=False):
    observed = smoke.until(f"visible tray {name}", lambda: visible(smoke, name))[0]
    outputs = smoke.control.status()["outputs"]
    if len(outputs) != 1:
        raise ControlError("tray acceptance requires one active output")
    surface = checks.opened(smoke, "tray") if menu else None
    return button_center(observed.get("bounds"), outputs[0], surface)


def click(smoke, name=TITLE, *, menu=False, button="left"):
    with Input(smoke.control) as inputs:
        inputs.click(*point(smoke, name, menu=menu), button=button)


def open_menu(smoke):
    click(smoke, button="right")
    checks.opened(smoke, "tray")
    smoke.until("accessible exported menu", lambda: visible(smoke, "More"))


def dismiss(smoke):
    with Input(smoke.control) as inputs:
        inputs.chord("esc")
    checks.closed(smoke)


def color_pixels(frame, bounds, color):
    surface = {
        "x": frame.metadata["logical_x"],
        "y": frame.metadata["logical_y"],
        "width": frame.width / frame.metadata["scale"],
        "height": 48,
    }
    left, top, right, bottom = frame.rect(surface, bounds)
    # The button's focus ring and attention dot use the same blue hue as this
    # fixture. Only app pixels in its center can prove an icon-content update.
    inset = min(right - left, bottom - top) // 4
    left, top, right, bottom = left + inset, top + inset, right - inset, bottom - inset
    count = 0
    for y in range(top, bottom):
        for x in range(left, right):
            red, green, blue = frame.rgb(x, y)
            count += (
                red > 150 and green < 110 and blue < 100
                if color == "red"
                else blue > 150 and red < 110 and green < 140
            )
    return count


def presented(smoke, color):
    output = smoke.control.status()["outputs"][0]
    park_pointer(smoke, output)

    def matches():
        observed = icon(smoke)
        frame = Frame(smoke.control.capture(output["name"]))
        count = color_pixels(frame, observed["bounds"], color)
        return (observed, count) if count > 20 else None

    observed, count = smoke.until(f"{color} tray icon pixels presented", matches)
    index = 1 + sum(entry["step"] == "tray-icon-presented" for entry in smoke.results)
    capture = f"tray-icon-{index}-{color}"
    smoke.capture(capture, output["name"])
    smoke.record(
        "tray-icon-presented",
        color=color,
        matching_pixels=count,
        icon=observed,
        capture=capture,
    )


def snapshot(smoke, label):
    value = a11y(smoke, "snapshot")["snapshot"]
    (smoke.artifacts / f"{label}.xml").write_text(value)
    smoke.capture(label)
    return value


def checked(smoke, value):
    condition = "@checked='true'" if value else "not(@checked='true')"
    return visible(
        smoke, "Enabled toggle", suffix=f"[@role='check menu item' and {condition}]"
    )


def separator(smoke):
    return a11y(smoke, "find", f"{SHELL}//*[@role='separator' and @showing='true']")[
        "elements"
    ]
