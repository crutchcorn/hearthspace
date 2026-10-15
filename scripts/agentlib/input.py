"""Evdev input with guaranteed best-effort releases on errors and interruption."""

import math
import time

from .control import Control, ControlError, Deadline

KEYS = dict(zip("qwertyuiop", range(16, 26)))
KEYS.update(zip("asdfghjkl", range(30, 39)))
KEYS.update(zip("zxcvbnm", range(44, 51)))
KEYS.update(zip("1234567890", range(2, 12)))
KEYS.update(
    {
        "escape": 1,
        "esc": 1,
        "backspace": 14,
        "tab": 15,
        "enter": 28,
        "return": 28,
        "ctrl": 29,
        "control": 29,
        "shift": 42,
        "alt": 56,
        "space": 57,
        "super": 125,
        "meta": 125,
        "left": 105,
        "right": 106,
        "up": 103,
        "down": 108,
        "home": 102,
        "end": 107,
        "delete": 111,
        "pageup": 104,
        "pagedown": 109,
        "insert": 110,
    }
)
KEYS.update({f"f{i}": 58 + i for i in range(1, 11)})
KEYS.update({"f11": 87, "f12": 88})
UNSHIFTED = dict(zip("-=[];'`\\,./", [12, 13, 26, 27, 39, 40, 41, 43, 51, 52, 53]))
SHIFTED = dict(zip('_+{}:"~|<>?', UNSHIFTED.values()))
SHIFTED.update(zip("!@#$%^&*()", range(2, 12)))
BUTTONS = {"left": 272, "right": 273, "middle": 274}


def keycode(name):
    if name.lower() in KEYS:
        return KEYS[name.lower()]
    if name.startswith("evdev:") and name[6:].isdigit():
        code = int(name[6:])
        if 1 <= code <= 767:
            return code
    raise ControlError(f"unsupported key {name!r}; use a key name or evdev:NUMBER")


def text_keys(text):
    """Validate all text before sending anything. This is a US keyboard mapping."""
    result = []
    for char in text:
        if char in "\n\t ":
            result.append(({"\n": 28, "\t": 15, " ": 57}[char], False))
        elif char.isascii() and char.isalnum():
            result.append((KEYS[char.lower()], char.isupper()))
        elif char in UNSHIFTED:
            result.append((UNSHIFTED[char], False))
        elif char in SHIFTED:
            result.append((SHIFTED[char], True))
        else:
            raise ControlError(
                f"unsupported text character {char!r}; type supports US ASCII"
            )
    return result


def chord_keys(names):
    codes = [keycode(name) for name in names.split("+")]
    if len(set(codes)) != len(codes):
        raise ControlError("key chord cannot repeat a key")
    return codes


class Input:
    def __init__(self, control):
        self.control = control
        self.held = []

    def __enter__(self):
        return self

    def __exit__(self, *_):
        # Releases have their own aggregate deadline: the action's deadline may
        # already have expired. Record before sending, since a lost ack can still
        # mean that the compositor received the press.
        cleanup = Control(self.control.path, Deadline(2))
        errors = []
        for kind, code in reversed(self.held):
            try:
                cleanup.request(f"{kind}-up {code}")
            except ControlError as error:
                errors.append(str(error))
        self.held.clear()
        if errors:
            raise ControlError("input release cleanup failed: " + "; ".join(errors))

    def down(self, kind, code):
        self.held.append((kind, code))
        self.control.request(f"{kind}-down {code}")

    def up(self, kind, code):
        self.control.request(f"{kind}-up {code}")
        self.held.remove((kind, code))

    def motion(self, x, y):
        if not math.isfinite(x) or not math.isfinite(y):
            raise ControlError("pointer coordinates must be finite")
        self.control.request(f"pointer-motion-abs {x} {y}")

    def chord(self, names):
        codes = chord_keys(names)
        for code in codes:
            self.down("key", code)
        for code in reversed(codes):
            self.up("key", code)

    def type(self, text):
        for code, shift in text_keys(text):
            if shift:
                self.down("key", 42)
            self.down("key", code)
            self.up("key", code)
            if shift:
                self.up("key", 42)

    def click(self, x, y, button="left"):
        self.motion(x, y)
        self.down("pointer-button", BUTTONS[button])
        self.up("pointer-button", BUTTONS[button])

    def drag(self, x, y, end_x, end_y, button="left", duration=0.3):
        if not math.isfinite(duration) or not 0 <= duration <= 30:
            raise ControlError("drag duration must be between 0 and 30 seconds")
        self.motion(x, y)
        self.down("pointer-button", BUTTONS[button])
        steps = max(1, math.ceil(duration * 60))
        for step in range(1, steps + 1):
            self.motion(x + (end_x - x) * step / steps, y + (end_y - y) * step / steps)
            if duration:
                time.sleep(min(duration / steps, self.control.deadline.remaining()))
        self.up("pointer-button", BUTTONS[button])

    def scroll(self, horizontal, vertical, modifiers=()):
        if not all(math.isfinite(value) for value in (horizontal, vertical)):
            raise ControlError("scroll distances must be finite")
        codes = [keycode(name) for name in modifiers]
        for code in codes:
            self.down("key", code)
        self.control.request(f"axis {horizontal} {vertical}")
        for code in reversed(codes):
            self.up("key", code)
