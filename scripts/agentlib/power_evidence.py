"""Assertions distinguish real sleep/resume and preserved apps from command ACKs."""

import time
from pathlib import Path

from .control import ControlError


def process_identity(pid, root=Path("/proc")):
    stat = (root / str(pid) / "stat").read_text().rsplit(")", 1)[1].split()
    return {"pid": pid, "start_ticks": int(stat[19])}


def owned_swaylocks(compositor_pid, root=Path("/proc")):
    compositor = root / str(compositor_pid)
    cgroup = (compositor / "cgroup").read_text()
    owner = compositor.stat().st_uid
    found = []
    for path in root.iterdir():
        if not path.name.isdecimal():
            continue
        try:
            if (path / "comm").read_text().strip() != "swaylock":
                continue
            status = dict(
                line.split(":", 1)
                for line in (path / "status").read_text().splitlines()
            )
            if (
                int(status["PPid"]) != compositor_pid
                or int(status["Uid"].split()[0]) != owner
                or (path / "cgroup").read_text() != cgroup
            ):
                continue
            found.append(
                {
                    **process_identity(int(path.name), root),
                    "parent_pid": compositor_pid,
                    "uid": owner,
                    "comm": "swaylock",
                    "cgroup": cgroup,
                }
            )
        except (FileNotFoundError, PermissionError):
            # Other host processes may exit or have unreadable /proc entries.
            continue
    return found


def sleep_sample(root=Path("/sys/power/suspend_stats")):
    return {
        "success": int((root / "success").read_text()),
        "fail": int((root / "fail").read_text()),
        "suspended_seconds": time.clock_gettime(time.CLOCK_BOOTTIME) - time.monotonic(),
    }


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
