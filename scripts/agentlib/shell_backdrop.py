"""An owned opaque terminal makes transparent popup corners distinguishable."""

import os
import subprocess
from contextlib import contextmanager

from .clipboard import stop_client
from .control import ControlError
from .firefox_assertions import maximized_at_output
from .input import Input

APP_ID = "hearthspace-popup-backdrop"
COLOR = (16, 24, 32)


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


def unfocus(smoke, target, output):
    # Popup focus removes the normal window's active frame. Capture its
    # reference in that same state; the bottom popup corners can overlap the
    # maximized window's inset highlight. The reserved outer margin is canvas.
    with Input(smoke.control) as inputs:
        inputs.click(output["logical_x"] + 4, output["logical_y"] + 4)

    def inactive():
        window = smoke.window(target)
        return window if window and window.get("focused") is False else None

    return smoke.until("popup backdrop is inactive before reference capture", inactive)


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
            smoke.control.request(f"maximize-window {target} true")
            smoke.until(
                "popup backdrop fills workarea between corners without decorations",
                lambda: (
                    maximized_at_output(current := smoke.window(target), [output])
                    and current.get("titlebar_height") == 0
                ),
            )
            unfocus(smoke, target, output)
            smoke.record("shell-popup-backdrop", window_id=target, argv=command)
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
