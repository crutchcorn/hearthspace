"""Native MRU previews, deferred focus, reveal motion and owned input cleanup."""

import argparse
import json
import shutil
import subprocess
from pathlib import Path

from . import window_switcher_smoke_checks as checks
from .control import ControlError
from .daily import DailySmoke
from .window_switcher_focused import focused
from .window_switcher_keyboard import unplug
from .window_switcher_pinch import stages as pinch_stages


class WindowSwitcherSmoke(DailySmoke):
    def __init__(self, args):
        args.captures, args.notifications = 1, False
        super().__init__(args)

    def launcher(self, operation, *extra, **kwargs):
        if operation == "start":
            extra = (*extra, "--isolated-profile")
        return super().launcher(operation, *extra, **kwargs)

    def step(self, name, operation):
        try:
            super().step(name, operation)
        except BaseException:
            self.failure_evidence(f"{name}-failed")
            raise

    def workflow(self):
        self.manifest = self.connect()
        outputs = self.control.status()["outputs"]
        if len(outputs) != 1:
            raise ControlError("window switcher acceptance requires one active output")
        self.output = outputs[0]
        self.step("ready", DailySmoke.ready)
        self.step("switcher-fixtures", checks.fixtures)
        self.step("held-alt-tab", checks.keyboard)
        self.step("keyboard-unplug", unplug)
        for kind in ("touchpad", "touchscreen"):
            if self.args.device not in (kind, "both"):
                continue
            if kind == "touchpad":
                self.record(
                    "backend-limit",
                    device=kind,
                    fingers=5,
                    reason="libinput does not emit five-contact motion gestures",
                )
            for fingers in (4,) if kind == "touchpad" else (4, 5):
                pinch_stages(self, kind, fingers)
                for direction in (-1, 1):
                    self.step(
                        f"{kind}-{fingers}-{'left' if direction < 0 else 'right'}",
                        lambda smoke, kind=kind, fingers=fingers, direction=direction: (
                            checks.swipe(smoke, kind, fingers, direction)
                        ),
                    )
                self.step(
                    f"{kind}-{fingers}-focused-view",
                    lambda smoke, kind=kind, fingers=fingers: focused(
                        smoke, kind, fingers
                    ),
                )
            self.step(
                f"{kind}-ownership-and-cancellation",
                lambda smoke, kind=kind: checks.ownership(smoke, kind),
            )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--session", type=Path, required=True)
    parser.add_argument("--timeout", type=float, default=90, help="per-step seconds")
    parser.add_argument("--total-timeout", type=float, default=900)
    parser.add_argument(
        "--device", choices=("touchpad", "touchscreen", "both"), default="both"
    )
    parser.add_argument("--log-level", default="info")
    args = parser.parse_args()
    try:
        if shutil.which("libinput") is None:
            raise ControlError(
                "switcher smoke requires libinput-tools and python3-evdev"
            )
        subprocess.run(
            ["/usr/bin/python3", "-c", "import evdev"], check=True, timeout=5
        )
        WindowSwitcherSmoke(args).run()
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
