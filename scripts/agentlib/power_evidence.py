"""Assertions distinguish real sleep/resume and preserved apps from command ACKs."""

import os
import stat
import time
from pathlib import Path

from .control import ControlError

LOCKER = "hearthspace-lock"
LOCKER_COMM = LOCKER[:15]  # Linux TASK_COMM_LEN includes the trailing NUL.


def locker_ready(binary, policy=Path("/etc/pam.d/hearthspace-lock")):
    if not binary.is_file() or not os.access(binary, os.X_OK):
        raise ControlError(f"build the native locker before acceptance: {binary}")
    try:
        info = policy.lstat()
    except FileNotFoundError as error:
        raise ControlError(
            "install the fixed /etc/pam.d/hearthspace-lock policy first; "
            "see CONTRIBUTING.md"
        ) from error
    if (
        not stat.S_ISREG(info.st_mode)
        or info.st_uid != 0
        or info.st_gid != 0
        or stat.S_IMODE(info.st_mode) != 0o644
    ):
        raise ControlError("locker PAM policy must be root:root, regular, mode 0644")


def process_identity(pid, root=Path("/proc")):
    stat = (root / str(pid) / "stat").read_text().rsplit(")", 1)[1].split()
    return {"pid": pid, "start_ticks": int(stat[19])}


def owned_lockers(compositor_pid, root=Path("/proc")):
    compositor = root / str(compositor_pid)
    cgroup = (compositor / "cgroup").read_text()
    owner = compositor.stat().st_uid
    compositor_exe = (compositor / "exe").readlink()
    if (
        not compositor_exe.is_absolute()
        or compositor_exe.name != "hearthspace-compositor"
    ):
        raise ControlError("owned compositor executable is unavailable or changed")
    executable = compositor_exe.with_name(LOCKER)
    expected = executable.stat()
    found = []
    for path in root.iterdir():
        if not path.name.isdecimal():
            continue
        try:
            if (path / "comm").read_text().strip() != LOCKER_COMM:
                continue
            binary = (path / "exe").stat()
            if (binary.st_dev, binary.st_ino) != (expected.st_dev, expected.st_ino):
                continue
            status = dict(
                line.split(":", 1)
                for line in (path / "status").read_text().splitlines()
            )
            if (
                int(status["PPid"]) != compositor_pid
                or any(int(uid) != owner for uid in status["Uid"].split())
                or (path / "cgroup").read_text() != cgroup
            ):
                continue
            found.append(
                {
                    **process_identity(int(path.name), root),
                    "parent_pid": compositor_pid,
                    "uid": owner,
                    "comm": LOCKER_COMM,
                    "exe": str(executable),
                    "exe_device": binary.st_dev,
                    "exe_inode": binary.st_ino,
                    "cgroup": cgroup,
                }
            )
        except (FileNotFoundError, PermissionError, ProcessLookupError):
            # Other host processes may exit or have unreadable /proc entries.
            continue
    return found


def sleep_sample(root=Path("/sys/power/suspend_stats")):
    return {
        "success": int((root / "success").read_text()),
        "fail": int((root / "fail").read_text()),
        "suspended_seconds": time.clock_gettime(time.CLOCK_BOOTTIME) - time.monotonic(),
    }


def safe_wake_delay(logind_property, wake_after):
    parts = logind_property.split()
    if len(parts) != 2 or parts[0] != "t" or not parts[1].isdecimal():
        raise ControlError("invalid logind InhibitDelayMaxUSec property")
    delay_usec = int(parts[1])
    minimum = (delay_usec + 999_999) // 1_000_000 + 10
    if wake_after < minimum:
        raise ControlError(
            f"RTC wake delay must be at least {minimum}s to outlast logind's "
            "sleep inhibitors with a ten-second safety margin"
        )
    return {"logind_inhibit_delay_usec": delay_usec, "minimum_wake_seconds": minimum}


def resumed(before, after):
    if after["success"] != before["success"] + 1:
        raise ControlError("expected exactly one completed kernel suspend cycle")
    if after["fail"] != before["fail"]:
        raise ControlError("kernel reported a failed suspend/resume attempt")
    elapsed = after["suspended_seconds"] - before["suspended_seconds"]
    if elapsed < 0.25:
        raise ControlError("kernel did not spend measurable time suspended")
    return elapsed


def locked(status):
    if status.get("locked") is not True or status.get("lock_confirmed") is not True:
        raise ControlError("lock shield has not been presented on every output")
    if status.get("windows"):
        raise ControlError("locked status exposed application metadata")


def recovery_ready(status):
    if status.get("recovery_ready") is not True:
        raise ControlError("physical input and output recovery is not ready")


def recovered(before, after):
    expected = {
        "phase": "awake",
        "connected": True,
        "sleep_inhibited": True,
        "lock_confirmed": True,
        "recovery_ready": True,
        "active": True,
        "sleep_count": before["sleep_count"] + 1,
        "resume_count": before["resume_count"] + 1,
        "last_error": None,
    }
    for field, value in expected.items():
        if after.get(field) != value:
            raise ControlError(f"power recovery {field}: expected {value!r}")


def preserved(before, after):
    if after is None or any(
        after.get(field) != before.get(field)
        for field in (
            "id",
            "app_id",
            "title",
            "canvas_x",
            "canvas_y",
            "width",
            "height",
        )
    ):
        raise ControlError("suspend did not preserve the same application and geometry")
    if not after.get("mapped") or not after.get("visible"):
        raise ControlError("preserved application is no longer visible")


def fresh_frame(before, after):
    if (
        after.get("presentation") != "page-flip"
        or after["output"] != before["output"]
        or after["frame_id"] <= before["frame_id"]
    ):
        raise ControlError("resume requires a fresh page flip on the same output")
