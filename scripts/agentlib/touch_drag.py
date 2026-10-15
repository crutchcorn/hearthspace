"""Owned native touchscreen regression for Firefox CSD and terminal decorations."""

import argparse
import json
import shutil
import subprocess
from pathlib import Path

from . import firefox_checks, touch_drag_checks
from .control import ControlError
from .daily import DailySmoke
from .firefox import FirefoxSmoke
from .pinch import terminal


class TouchDragSmoke(FirefoxSmoke):
    def workflow(self):
        self.manifest = self.connect()
        outputs = self.control.status()["outputs"]
        if len(outputs) != 1:
            raise ControlError("touch drag acceptance requires one active output")
        self.output = outputs[0]
        self.step("ready", DailySmoke.ready)

        def decorated_terminal(smoke):
            smoke.target = terminal(smoke)["id"]
            touch_drag_checks.run(smoke, "ssd-unit", zoom=False, firefox=False)
            smoke.control.request(f"close-window {smoke.target}")
            smoke.until("test terminal closed", lambda: not smoke.window(smoke.target))

        self.step("ssd-touch-drag", decorated_terminal)
        self.step("firefox-launch", firefox_checks.launch)
        for zoom in (False, True):
            name = "csd-zoomed" if zoom else "csd-unit"
            self.step(
                name,
                lambda smoke, name=name, zoom=zoom: touch_drag_checks.run(
                    smoke, name, zoom=zoom, firefox=True
                ),
            )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--session", type=Path, required=True)
    parser.add_argument("--firefox", type=Path, default=Path("/usr/bin/firefox"))
    parser.add_argument("--timeout", type=float, default=90, help="per-step seconds")
    parser.add_argument("--total-timeout", type=float, default=420)
    parser.add_argument("--log-level", default="info")
    args = parser.parse_args()
    try:
        if shutil.which("libinput") is None:
            raise ControlError(
                "touch drag smoke requires libinput-tools and python3-evdev"
            )
        subprocess.run(
            ["/usr/bin/python3", "-c", "import evdev"], check=True, timeout=5
        )
        TouchDragSmoke(args).run()
        return 0
    except (
        ControlError,
        OSError,
        ValueError,
        KeyError,
        subprocess.SubprocessError,
        KeyboardInterrupt,
    ) as error:
        print(json.dumps({"ok": False, "error": str(error)}), flush=True)
        return 1
