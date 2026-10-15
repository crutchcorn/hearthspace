"""Owned native Firefox regression for CSD resize and maximized canvas zoom."""

import argparse
import json
import os
import subprocess
from pathlib import Path

from . import firefox_checks as checks
from . import firefox_profile, firefox_shell
from .control import ControlError
from .daily import DailySmoke


class FirefoxSmoke(DailySmoke):
    def __init__(self, args):
        args.captures, args.notifications = 1, False
        super().__init__(args)
        self.profile = None

    def step(self, name, operation):
        try:
            super().step(name, operation)
        except BaseException:
            self.failure_evidence(f"{name}-failed")
            raise

    def workflow(self):
        self.manifest = self.connect()
        if len(self.control.status()["outputs"]) != 1:
            raise ControlError(
                "Firefox gesture acceptance currently requires one active output"
            )
        for name, operation in [
            ("ready", DailySmoke.ready),
            ("firefox-launch", checks.launch),
            ("csd-restore-and-edge-resize", checks.controls_and_resize),
            ("shell-zoom", firefox_shell.run),
            ("touchpad-zoom", lambda smoke: checks.touch_zoom(smoke, "touchpad")),
            ("touchscreen-zoom", lambda smoke: checks.touch_zoom(smoke, "touchscreen")),
            ("explicit-fullscreen", checks.fullscreen),
        ]:
            self.step(name, operation)

    def run(self):
        binary = self.args.firefox.resolve(strict=True)
        if not os.access(binary, os.X_OK) or str(binary).startswith("/snap/"):
            raise ControlError(
                "--firefox must name an executable native Firefox, not Snap"
            )
        self.args.firefox = binary
        self.profile = firefox_profile.prepare(self.directory, self.marker)
        error = None
        try:
            super().run()
        except BaseException as failure:
            error = failure
            raise
        finally:
            try:
                self.cleanup_profile(error)
            except BaseException as cleanup_error:
                self.record("profile-cleanup-failed", error=str(cleanup_error))
                if error is None:
                    raise

    def cleanup_profile(self, error):
        manifest = self.directory / "session.json"
        if manifest.exists() and json.loads(manifest.read_text()).get(
            "cleanup", {}
        ).get("complete"):
            firefox_profile.verify_stopped(self.directory, self.marker)
            if error is None:
                firefox_profile.cleanup(self.directory, self.marker)
                self.record("firefox-profile-removed", profile=str(self.profile))
            else:
                self.record(
                    "failed-profile-retained",
                    profile=str(self.profile),
                    owned_processes_gone=True,
                )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--session", type=Path, required=True, help="fresh evidence directory"
    )
    parser.add_argument("--firefox", type=Path, default=Path("/usr/bin/firefox"))
    parser.add_argument(
        "--timeout", type=float, default=90, help="maximum seconds per step"
    )
    parser.add_argument(
        "--total-timeout",
        type=float,
        default=420,
        help="aggregate start/action budget; cleanup reserves 45 seconds",
    )
    parser.add_argument("--log-level", default="info")
    args = parser.parse_args()
    try:
        FirefoxSmoke(args).run()
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
