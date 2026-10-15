"""Repeatable everyday native workflow, with deadlines and owned-session cleanup."""

import argparse
import json
import math
import os
import shutil
import subprocess
import time
import uuid
from pathlib import Path

from . import daily_checks as checks
from .control import Control, ControlError, Deadline
from .smoke import LAUNCHER, Smoke
from .viewport_fixture import set_viewport


class DailySmoke(Smoke):
    def __init__(self, args):
        directory = args.session.resolve()
        if (directory / "session.json").exists() or (
            directory / "artifacts/results.json"
        ).exists():
            raise ControlError("daily smoke requires a fresh session directory")
        Deadline(args.timeout)
        self.total = Deadline(args.total_timeout)
        if not 1 <= args.captures <= 20:
            raise ControlError("captures must be between 1 and 20 per output")
        super().__init__(args)
        self.marker = "HEARTHSPACE_DAILY_" + uuid.uuid4().hex[:12]
        self.manifest = None
        self.target = None

    def launcher(self, operation, *extra, expected=0):
        # Cleanup has a separate, reserved budget even if interaction time expired.
        budget = (
            45.0
            if operation == "stop"
            else min(self.args.timeout, self.total.remaining())
        )
        command = [
            str(LAUNCHER),
            operation,
            "--session",
            str(self.directory),
            "--timeout",
            str(budget),
            *extra,
        ]
        if operation == "start":
            command.extend(["--log-level", self.args.log_level])
        result = subprocess.run(
            command, capture_output=True, text=True, check=False, timeout=budget
        )
        self.record(
            operation,
            argv=command,
            returncode=result.returncode,
            stdout=result.stdout,
            stderr=result.stderr,
        )
        if expected is not None and result.returncode != expected:
            raise ControlError(f"launcher {operation} exited {result.returncode}")
        return result

    def step(self, name, operation):
        budget = min(self.args.timeout, self.total.remaining())
        self.control = Control(self.manifest["command_socket"], Deadline(budget))
        self.record("step-start", name=name, budget=budget)
        started = time.monotonic()
        operation(self)
        self.control.deadline.remaining()
        self.record("step-passed", name=name, elapsed=time.monotonic() - started)

    def ready(self):
        status = self.control.status()
        if status["backend"] != "native" or not status.get("shell_ready"):
            raise ControlError("daily smoke requires a ready native shell")
        set_viewport(self, x=0, y=0, scale=1)
        self.record("ready", manifest=self.manifest, status=status)
        self.capture("initial")

    def failure_evidence(self, name):
        """Preserve post-action evidence without replacing the original failure."""
        original = self.control
        try:
            budget = min(8.0, self.total.remaining())
            self.control = Control(original.path, Deadline(budget))
            self.record("failure-status", name=name, status=self.control.status())
            self.capture(name)
            snapshot = checks.a11y(self, "snapshot")
            destination = self.artifacts / f"{name}.xml"
            destination.write_text(snapshot["snapshot"])
            self.record("failure-accessibility", name=name, artifact=str(destination))
        except (ControlError, OSError, ValueError, KeyError) as error:
            self.record("failure-evidence-unavailable", name=name, error=str(error))
        finally:
            self.control = original

    def workflow(self):
        self.manifest = self.connect()
        for name, operation in [
            ("ready", DailySmoke.ready),
            ("keyboard-launch-and-focus", checks.terminal),
            ("unicode-paste", checks.unicode_paste),
            ("running-window-selector", checks.selector),
            ("off-canvas-recovery", checks.off_canvas),
            ("maximize-fullscreen-restore", checks.window_states),
            ("repeated-page-flips", checks.captures),
            ("stalled-settings-client", checks.stalled_settings),
            ("semantic-role-and-action", checks.accessibility),
        ]:
            self.step(name, operation)
        if self.args.notifications:
            self.step("notification-dismiss", checks.notifications)

    def run(self):
        if shutil.which("foot") is None or shutil.which("bash") is None:
            raise ControlError("daily smoke requires foot and bash")
        if self.args.notifications and shutil.which("notify-send") is None:
            raise ControlError("--notifications requires notify-send")
        self.record(
            "host",
            uname=list(os.uname()),
            marker=self.marker,
            total_budget=self.args.total_timeout,
            step_budget=self.args.timeout,
        )
        error = None
        try:
            self.launcher(
                "start",
                "--no-build",
                "--watchdog",
                str(math.ceil(self.args.total_timeout + 60)),
            )
            self.workflow()
        except BaseException as failure:
            error = failure
            self.record("operation-failed", error=str(failure))
            raise
        finally:
            if (self.directory / "session.json").exists():
                try:
                    self.launcher("stop")
                    self.verify_restored()
                except BaseException as failure:
                    self.record("recovery-failed", error=str(failure))
                    if error is None:
                        raise
        self.record("complete", ok=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--session", type=Path, required=True, help="fresh absolute evidence directory"
    )
    parser.add_argument(
        "--timeout",
        type=float,
        default=45,
        help="maximum seconds per step, including start",
    )
    parser.add_argument(
        "--total-timeout",
        type=float,
        default=240,
        help="aggregate start/interaction budget; cleanup reserves 45 more seconds",
    )
    parser.add_argument(
        "--captures",
        type=int,
        default=5,
        help="consecutive page-flip captures per output",
    )
    parser.add_argument(
        "--notifications",
        action="store_true",
        help="also post and semantically dismiss a private notification",
    )
    parser.add_argument("--log-level", default="info")
    args = parser.parse_args()
    try:
        DailySmoke(args).run()
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
