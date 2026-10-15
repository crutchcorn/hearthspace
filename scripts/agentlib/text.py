"""Explicit UTF-8 clipboard input; never changes the global keyboard mapping."""

from .control import ControlError
from .input import chord_keys

MAX_TEXT_BYTES = 16 * 1024


def clipboard_command(text, path=None):
    if (text is None) == (path is None):
        raise ControlError("provide either text or --file")
    if path is not None:
        with path.open("rb") as stream:
            encoded = stream.read(MAX_TEXT_BYTES + 1)
        if len(encoded) > MAX_TEXT_BYTES:
            raise ControlError("clipboard text exceeds 16 KiB UTF-8 limit")
        text = encoded.decode("utf-8")
    if "\0" in text:
        raise ControlError("clipboard text cannot contain NUL")
    encoded = text.encode("utf-8")
    if len(encoded) > MAX_TEXT_BYTES:
        raise ControlError("clipboard text exceeds 16 KiB UTF-8 limit")
    return f"clipboard-set {encoded.hex()}"


def paste(inputs, command, shortcut):
    # Validate the entire chord before replacing any existing clipboard content.
    chord_keys(shortcut)
    inputs.control.request(command)
    inputs.chord(shortcut)
