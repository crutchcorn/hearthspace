"""Bounded native acceptance for overview gestures, live thumbnails, and focus."""

import argparse
import json
import shutil
import subprocess
from pathlib import Path

from . import overview_checks as checks
from .control import ControlError
from .daily import DailySmoke


class OverviewSmoke(DailySmoke):
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
            raise ControlError("overview acceptance requires one active output")
        self.output = outputs[0]
        self.step("ready", DailySmoke.ready)
        self.step("overview-fixtures", checks.fixtures)
        for kind in ("touchpad", "touchscreen"):
            if self.args.device not in (kind, "both"):
                continue
            for fingers in (4, 5):
                for outward, small, cancel in (
                    (False, True, False),
                    *([(False, False, True)] if fingers == 4 else []),
                    (False, False, False),
                    (True, True, False),
                    (True, False, False),
                ):
                    self.step(
                        f"{kind}-{fingers}-{'out' if outward else 'in'}-{small}-{cancel}",
                        lambda smoke, kind=kind, fingers=fingers, outward=outward, small=small, cancel=cancel: (
                            checks.gesture(
                                smoke,
                                kind,
                                fingers,
                                outward=outward,
                                small=small,
                                cancel=cancel,
                            )
                        ),
                    )
            self.step(
                f"{kind}-selection",
                lambda smoke, kind=kind: checks.selection(smoke, kind),
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
                "overview smoke requires libinput-tools and python3-evdev"
            )
        subprocess.run(
            ["/usr/bin/python3", "-c", "import evdev"], check=True, timeout=5
        )
        OverviewSmoke(args).run()
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
