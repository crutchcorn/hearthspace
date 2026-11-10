"""Publish verified build outputs beside the actual compositor, receipt last."""

import json
import os
import shutil
import tempfile
from pathlib import Path

from installlib import xwayland
from installlib.filesystem import no_symlinks


def compositor_path(value):
    path = Path(value)
    if not path.is_absolute() or ".." in path.parts:
        raise RuntimeError(
            "--compositor must be an absolute executable path without '..'"
        )
    no_symlinks(path)
    xwayland.regular_file(path, executable=True)
    return path


def destinations(compositor):
    compositor = compositor_path(compositor)
    paths = [compositor.with_name(name) for name in (xwayland.BINARY, xwayland.RECEIPT)]
    for path in paths:
        no_symlinks(path)
        if path.exists():
            xwayland.regular_file(path)
    return paths


def stage(binary, compositor, receipt):
    """Only the preparation pipeline calls this after its pinned build succeeds.

    Each replacement is atomic. The receipt is the commit marker: interruption
    between the two replacements fails closed on SHA mismatch. Normal failures
    restore the previous pair, including its modes.
    """
    targets = destinations(compositor)
    if xwayland.regular_file(binary, executable=True)["sha256"] != receipt["sha256"]:
        raise RuntimeError("built Xwayland changed before companion staging")
    with tempfile.TemporaryDirectory(
        prefix=".hearthspace-xwayland-", dir=compositor.parent
    ) as name:
        temporary = Path(name)
        pending = [temporary / path.name for path in targets]
        shutil.copyfile(binary, pending[0])
        pending[0].chmod(0o755)
        pending[1].write_text(json.dumps(receipt, sort_keys=True, indent=2) + "\n")
        pending[1].chmod(0o644)
        xwayland.verify_binary(pending[0])
        backups = []
        for index, path in enumerate(targets):
            backup = temporary / f"previous-{index}"
            if path.exists():
                shutil.copy2(path, backup, follow_symlinks=False)
                backups.append(backup)
            else:
                backups.append(None)
        changed = []
        try:
            for path, target in zip(pending, targets, strict=True):
                with path.open("rb") as stream:
                    os.fsync(stream.fileno())
                no_symlinks(target)
                os.replace(path, target)
                changed.append(target)
            xwayland.verify_companions(compositor)
        except BaseException:
            for target, backup in zip(targets, backups, strict=True):
                if target not in changed:
                    continue
                if backup is None:
                    target.unlink()
                else:
                    os.replace(backup, target)
            raise
    return targets[0]
