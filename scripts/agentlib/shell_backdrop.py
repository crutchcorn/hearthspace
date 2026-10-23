"""An owned opaque terminal makes transparent popup corners distinguishable."""

import os
import subprocess
from contextlib import contextmanager

from .clipboard import stop_client
from .control import ControlError
from .input import Input

APP_ID = "hearthspace-popup-backdrop"
COLOR = (16, 24, 32)
PADDING = 128
CELL_TOLERANCE = 32


def environment(manifest):
    env = {
        **os.environ,
        "XDG_RUNTIME_DIR": manifest["runtime_dir"],
        "WAYLAND_DISPLAY": manifest["wayland_display"],
        "DBUS_SESSION_BUS_ADDRESS": manifest["dbus_session_bus_address"],
    }
    for key in ("DISPLAY", "AT_SPI_BUS_ADDRESS", "DBUS_SESSION_BUS_PID"):
        env.pop(key, None)
    return env


def unfocus(smoke, target):
    # Popup focus removes the normal window's active frame. Capture its
    # reference in that same state. Keyboard shell focus avoids a background
    # press near the output edge starting a desktop marquee and edge panning.
    with Input(smoke.control) as inputs:
        inputs.chord("super+f6")

    def inactive():
        window = smoke.window(target)
        return window if window and window.get("focused") is False else None

    return smoke.until("popup backdrop is inactive before reference capture", inactive)


def configured(window, output):
    if not (
        window
        and window.get("mapped")
        and window.get("visible")
        and window.get("titlebar_height") == 0
        and window.get("maximized") is False
        and window.get("fullscreen") is False
        and window.get("focused_view") is False
        and window.get("output_coordinates") is False
    ):
        return False
    # Foot rounds ordinary resize requests to complete character cells. Its
    # actual committed dimensions must still cover every popup's blur halo.
    return all(
        0
        <= output["logical_" + axis] + 2 * PADDING - window["content_" + axis]
        <= CELL_TOLERANCE
        for axis in ("width", "height")
    )


def covers_output(window, output):
    return configured(window, output) and all(
        abs(window[axis] - (output["logical_" + axis] - PADDING)) <= 1
        and window[size] == window["content_" + size]
        for axis, size in (("x", "width"), ("y", "height"))
    )


def place(smoke, target, output):
    width, height = (
        output["logical_" + axis] + 2 * PADDING for axis in ("width", "height")
    )
    smoke.control.request(f"resize-window {target} {width} {height}")
    smoke.until(
        "ordinary popup backdrop committed its oversized native content",
        lambda: configured(smoke.window(target), output),
    )
    view = smoke.control.status()["viewport"]
    if view["scale"] != 1 or view.get("animating", False):
        raise ControlError("popup contrast fixture needs a settled unit-scale canvas")
    x, y = (view[axis] + output["logical_" + axis] - PADDING for axis in ("x", "y"))
    smoke.control.request(f"move-window {target} {x} {y}")
    smoke.until(
        "popup backdrop covers output and blur neighborhoods",
        lambda: covers_output(smoke.window(target), output),
    )


@contextmanager
def backdrop(smoke, output):
    before = {item["id"] for item in smoke.control.status()["windows"]}
    command = [
        "foot",
        "--config=/dev/null",
        f"--app-id={APP_ID}",
        "--title=Hearthspace popup backdrop",
        "--override=colors.background=101820",
        "--override=colors.alpha=1.0",
        "--override=csd.preferred=none",
        "sh",
        "-c",
        r"printf '\033[?25l'; exec cat",
    ]
    with (smoke.artifacts / "shell-popup-backdrop.log").open("w") as log:
        process = subprocess.Popen(
            command, env=environment(smoke.manifest), stdout=log, stderr=log
        )
        try:

            def mapped():
                if process.poll() is not None:
                    raise ControlError(
                        "popup backdrop exited before mapping; see its log"
                    )
                return next(
                    (
                        item
                        for item in smoke.control.status()["windows"]
                        if item["id"] not in before
                        and item.get("app_id") == APP_ID
                        and item.get("mapped")
                    ),
                    None,
                )

            window = smoke.until("owned popup backdrop mapped", mapped)
            target = window["id"]
            place(smoke, target, output)
            inactive = unfocus(smoke, target)
            if not covers_output(inactive, output):
                raise ControlError("shell focus changed popup backdrop coverage")
            smoke.record(
                "shell-popup-backdrop", window_id=target, argv=command, window=inactive
            )
            try:
                yield
            except BaseException:
                smoke.failure_evidence("shell-popup-presentation-failed")
                raise
            smoke.control.request(f"close-window {target}")
            smoke.until(
                "only popup backdrop closed", lambda: smoke.window(target) is None
            )
            if {item["id"] for item in smoke.control.status()["windows"]} != before:
                raise ControlError("popup backdrop cleanup changed unrelated windows")
            smoke.record("shell-popup-backdrop-closed", window_id=target)
        finally:
            stop_client(process)
