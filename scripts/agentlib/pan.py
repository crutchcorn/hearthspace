"""Bounded real-input acceptance for three-finger canvas pan and combined zoom."""

import argparse
import json
import shutil
import subprocess
from pathlib import Path

from . import pan_checks
from .control import ControlError
from .daily import DailySmoke
from .pinch import terminal


class PanSmoke(DailySmoke):
    def __init__(self, args):
        args.captures, args.notifications = 1, False
        super().__init__(args)

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
            raise ControlError("pan acceptance requires exactly one active output")
        self.output = outputs[0]
        self.step("ready", DailySmoke.ready)

        def fixture(smoke):
            smoke.floating = terminal(smoke)
            smoke.target = smoke.floating["id"]

        self.step("terminal", fixture)
        for kind in ("touchpad", "touchscreen"):
            if self.args.device not in (kind, "both"):
                continue
            for case in pan_checks.CASES:
                self.step(
                    f"{kind}-{case['name']}",
                    lambda smoke, kind=kind, case=case: pan_checks.run(
                        smoke, kind, case
                    ),
                )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--session", type=Path, required=True)
    parser.add_argument("--timeout", type=float, default=60, help="per-step seconds")
    parser.add_argument("--total-timeout", type=float, default=600)
    parser.add_argument(
        "--device", choices=("touchpad", "touchscreen", "both"), default="both"
    )
    parser.add_argument("--log-level", default="info")
    args = parser.parse_args()
    try:
        if shutil.which("libinput") is None:
            raise ControlError("pan smoke requires libinput-tools and python3-evdev")
        subprocess.run(
            ["/usr/bin/python3", "-c", "import evdev"], check=True, timeout=5
        )
        PanSmoke(args).run()
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
