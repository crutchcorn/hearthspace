"""Unmount only the document portal inside a verified owned native runtime."""

import re
from pathlib import Path

from .common import run


def document_mounted(mountinfo, target):
    for line in mountinfo.splitlines():
        fields, separator, filesystem = line.partition(" - ")
        fields = fields.split()
        if not separator or len(fields) < 5:
            continue
        if filesystem.split()[0] != "fuse.portal":
            continue
        mount = re.sub(r"\\([0-7]{3})", lambda match: chr(int(match[1], 8)), fields[4])
        if mount == str(target):
            return True
    return False


def cleanup_documents(runtime, timeout):
    target = runtime / "doc"
    mountinfo = Path("/proc/self/mountinfo")
    if not document_mounted(mountinfo.read_text(), target):
        return
    run(["fusermount3", "-u", "-z", "--", target], timeout=timeout)
    if document_mounted(mountinfo.read_text(), target):
        raise RuntimeError(f"owned document portal is still mounted: {target}")
