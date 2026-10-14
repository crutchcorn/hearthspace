"""Detach only known desktop FUSE mounts in a verified owned native runtime."""

import re
import time
from pathlib import Path

from .common import run

OWNED_FILESYSTEMS = {"doc": "fuse.portal", "gvfs": "fuse.gvfsd-fuse"}


def runtime_mounts(mountinfo, runtime):
    mounts = []
    for line in mountinfo.splitlines():
        fields, separator, filesystem = line.partition(" - ")
        fields, filesystem = fields.split(), filesystem.split()
        if not separator or len(fields) < 5 or not filesystem:
            continue
        target = Path(
            re.sub(r"\\([0-7]{3})", lambda match: chr(int(match[1], 8)), fields[4])
        )
        if target == runtime or runtime in target.parents:
            mounts.append((target, filesystem[0]))
    return mounts


def cleanup_runtime_mounts(runtime, timeout):
    if runtime.is_symlink():
        raise RuntimeError("refusing symlink desktop runtime")
    deadline = time.monotonic() + timeout
    mountinfo = Path("/proc/self/mountinfo")
    mounts = runtime_mounts(mountinfo.read_text(), runtime)
    for target, filesystem in mounts:
        if target.parent != runtime or OWNED_FILESYSTEMS.get(target.name) != filesystem:
            raise RuntimeError(
                f"unexpected mounted filesystem in owned runtime: {target}"
            )
    for target, _ in mounts:
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise TimeoutError("desktop FUSE cleanup deadline exceeded")
        run(["fusermount3", "-u", "-z", "--", target], timeout=remaining)
    if runtime_mounts(mountinfo.read_text(), runtime):
        raise RuntimeError(
            f"owned desktop runtime still contains mounted filesystems: {runtime}"
        )
