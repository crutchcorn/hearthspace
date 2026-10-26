"""Checked paths and atomic managed-file writes shared by the installer."""

import os
import tempfile
from pathlib import Path


def no_symlinks(path, *, leaf=True):
    for candidate in [*reversed(path.parents), *([path] if leaf else [])]:
        if candidate.is_symlink():
            raise RuntimeError(f"refusing symlink in installation path: {candidate}")


def directories(path):
    """New system directories must remain traversable even under umask 077."""
    no_symlinks(path)
    for directory in [*reversed(path.parents), path]:
        if not directory.exists():
            directory.mkdir(mode=0o755)
            directory.chmod(0o755)


def atomic_write(path, content, mode, *, owner=None):
    no_symlinks(path)
    directories(path.parent)
    descriptor, temporary = tempfile.mkstemp(prefix=".hearthspace-", dir=path.parent)
    try:
        with os.fdopen(descriptor, "wb") as stream:
            stream.write(content)
            if owner is not None:
                os.fchown(stream.fileno(), *owner)
            os.fchmod(stream.fileno(), mode)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary, path)
    finally:
        Path(temporary).unlink(missing_ok=True)
