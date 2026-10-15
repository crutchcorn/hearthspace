"""Bounded RTC alarm ownership; never overwrite an existing wake schedule."""

import re
import subprocess
import time
from pathlib import Path

from .control import ControlError


def command(argv, timeout=10):
    result = subprocess.run(
        argv, capture_output=True, text=True, check=False, timeout=timeout
    )
    if result.returncode:
        raise ControlError(f"{argv[0]} failed: {result.stderr.strip()}")
    return {"argv": argv, "stdout": result.stdout, "stderr": result.stderr}


class WakeAlarm:
    def __init__(self, rtc, record, root=Path("/sys/class/rtc")):
        if re.fullmatch(r"rtc[0-9]+", rtc) is None:
            raise ControlError("RTC must be a kernel device name such as rtc0")
        self.rtc = rtc
        self.root = root / rtc
        self.record = record
        self.owned = None

    def current(self):
        value = (self.root / "wakealarm").read_text().strip()
        return int(value) if value else None

    def available(self):
        if (self.root / "device/power/wakeup").read_text().strip() != "enabled":
            raise ControlError("RTC wakeup must already be enabled")
        if self.current() is not None:
            raise ControlError("an existing RTC alarm belongs to another task")

    def rtcwake(self, mode, seconds=None):
        argv = ["sudo", "-n", "rtcwake", "--utc", "--device", self.rtc, "--mode", mode]
        if seconds is not None:
            argv.extend(["--seconds", str(seconds)])
        return command(argv, timeout=(seconds or 0) + 10)

    def prove_interrupt(self):
        self.available()
        # The on mode only waits for an RTC IRQ; it never changes system power.
        # This tests the alarm interrupt, not the firmware's asleep wake behavior.
        started = time.monotonic()
        expected = int((self.root / "since_epoch").read_text()) + 3
        try:
            result = self.rtcwake("on", 3)
        finally:
            self.remember(expected)
            self.clear()
        elapsed = time.monotonic() - started
        if not 1 <= elapsed <= 10:
            raise ControlError("RTC interrupt arrived outside the expected interval")
        self.record("rtc-interrupt-proven", elapsed=elapsed, **result)

    def arm(self, seconds):
        self.available()
        expected = int((self.root / "since_epoch").read_text()) + seconds
        try:
            result = self.rtcwake("no", seconds)
        finally:
            self.remember(expected)
        epoch = int((self.root / "since_epoch").read_text())
        if self.owned is None or not seconds - 3 <= self.owned - epoch <= seconds + 3:
            raise ControlError(
                "RTC wake alarm readback differs from requested deadline"
            )
        self.record("rtc-wake-armed", rtc=self.rtc, wake_epoch=self.owned, **result)

    def remember(self, expected):
        current = self.current()
        # rtcwake adds a second for scheduling; never adopt an unrelated alarm
        # after a failed command, even when none existed before the command.
        if current is not None and not expected - 1 <= current <= expected + 2:
            raise ControlError("RTC alarm changed while arming; refusing to claim it")
        self.owned = current

    def clear(self):
        if self.owned is None:
            return
        current = self.current()
        if current is not None and current != self.owned:
            raise ControlError("RTC alarm changed ownership; refusing to clear it")
        if current is not None:
            self.rtcwake("disable")
        self.owned = None
