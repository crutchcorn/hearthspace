"""Nonexecuted private Xwayland fixtures with the real pinned receipt contract."""

import hashlib
import json

from installlib import xwayland
from runtime_artifacts import BINARIES


def companions(directory, content=b"private Xwayland fixture; never execute\n"):
    binary = directory / xwayland.BINARY
    binary.write_bytes(content)
    binary.chmod(0o755)
    receipt = {**xwayland.requirements(), "sha256": hashlib.sha256(content).hexdigest()}
    path = directory / xwayland.RECEIPT
    path.write_text(json.dumps(receipt))
    path.chmod(0o644)
    return binary


def runtime(directory):
    directory.mkdir(parents=True, exist_ok=True)
    for name in (*BINARIES, "hearthspace-share-picker"):
        path = directory / name
        path.write_bytes(b"runtime fixture; never execute\n")
        path.chmod(0o755)
    return companions(directory)
