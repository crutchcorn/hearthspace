"""Activate complete releases atomically; remove only verified owned files."""

import fcntl
import hashlib
import json
import os
import re
import shutil
import stat
import tempfile
from contextlib import contextmanager
from pathlib import Path

from .layout import (
    BASE,
    BINARIES,
    CURRENT,
    LAUNCHER,
    MANIFEST,
    SESSION,
    digest,
    launcher,
    session_entry,
)


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


def relative_path(value):
    path = Path(value)
    if path.is_absolute() or ".." in path.parts or str(path) != value:
        raise RuntimeError(f"unsafe installation manifest path: {value}")
    if path not in (LAUNCHER, SESSION) and not re.fullmatch(
        r"lib/hearthspace/releases/[0-9a-f]{64}/(?:bin|share)/.+", value
    ):
        raise RuntimeError(f"unowned installation manifest path: {value}")
    return path


def atomic_write(path, content, mode):
    no_symlinks(path)
    directories(path.parent)
    descriptor, temporary = tempfile.mkstemp(prefix=".hearthspace-", dir=path.parent)
    try:
        with os.fdopen(descriptor, "wb") as stream:
            stream.write(content)
            os.fchmod(stream.fileno(), mode)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary, path)
    finally:
        Path(temporary).unlink(missing_ok=True)


def replace_current(root, target):
    no_symlinks(root / CURRENT, leaf=False)
    temporary = root / BASE / ".current-new"
    if temporary.exists() or temporary.is_symlink():
        raise RuntimeError(f"unexpected pending activation: {temporary}")
    temporary.symlink_to(target)
    try:
        os.replace(temporary, root / CURRENT)
    finally:
        temporary.unlink(missing_ok=True)


@contextmanager
def locked(root):
    base = root / BASE
    no_symlinks(base)
    if (
        base.exists()
        and not (root / MANIFEST).exists()
        and any(entry.name != ".install.lock" for entry in base.iterdir())
    ):
        raise RuntimeError(f"refusing unowned installation directory: {base}")
    directories(base)
    lock = base / ".install.lock"
    descriptor = os.open(lock, os.O_CREAT | os.O_RDWR | os.O_NOFOLLOW, 0o600)
    try:
        fcntl.flock(descriptor, fcntl.LOCK_EX)
        if not lock.exists() or os.fstat(descriptor).st_ino != lock.stat().st_ino:
            raise RuntimeError("installation changed while waiting; retry the command")
        yield
    finally:
        os.close(descriptor)


def installed(root, prefix):
    manifest = root / MANIFEST
    no_symlinks(manifest)
    if not manifest.exists():
        return {"version": 1, "prefix": str(prefix), "files": {}, "current": None}
    data = json.loads(manifest.read_text())
    if data.get("version") != 1 or data.get("prefix") != str(prefix):
        raise RuntimeError("unknown installation manifest or prefix mismatch")
    if not re.fullmatch(r"releases/[0-9a-f]{64}", data.get("current", "")):
        raise RuntimeError("unsafe release target in installation manifest")
    for name, expected in data["files"].items():
        path = root / relative_path(name)
        no_symlinks(path)
        if (
            not path.is_file()
            or digest(path) != expected["sha256"]
            or stat.S_IMODE(path.stat().st_mode) != expected["mode"]
        ):
            raise RuntimeError(
                f"owned file changed or missing; refusing to overwrite/remove: {path}"
            )
    current = root / CURRENT
    no_symlinks(current, leaf=False)
    if not current.is_symlink() or os.readlink(current) != data["current"]:
        raise RuntimeError(
            "current release link was changed; refusing to overwrite/remove"
        )
    return data


def verify_bundle(bundle, prefix):
    spec = json.loads((bundle / "bundle.json").read_text())
    if spec.get("version") != 1 or spec.get("prefix") != str(prefix):
        raise RuntimeError("bundle prefix/version mismatch")
    release_id = spec.get("release", "")
    if not re.fullmatch(r"[0-9a-f]{64}", release_id):
        raise RuntimeError("invalid release identifier")
    expected_names = {f"bin/{name}" for name in BINARIES} | {
        "bin/hearthspace-login-guard",
        "share/applications/dev.hearthspace.Settings.desktop",
        "share/icons/hicolor/scalable/apps/hearthspace.svg",
    }
    if set(spec["files"]) != expected_names:
        raise RuntimeError("bundle does not contain the complete runtime file set")
    if (
        hashlib.sha256(json.dumps(spec["files"], sort_keys=True).encode()).hexdigest()
        != release_id
    ):
        raise RuntimeError("bundle release identifier does not match its files")
    for name, expected in spec["files"].items():
        relative_path(f"{BASE}/releases/{release_id}/{name}")
        path = bundle / "release" / name
        no_symlinks(path)
        if not path.is_file() or digest(path) != expected["sha256"]:
            raise RuntimeError(f"bundle file changed or missing: {path}")
        if expected["mode"] not in (0o644, 0o755):
            raise RuntimeError(f"invalid file mode: {name}")
    return spec


def install(bundle, root, prefix):
    spec = verify_bundle(bundle, prefix)
    with locked(root):
        previous = installed(root, prefix)
        stable = {
            LAUNCHER: (launcher(prefix), 0o755),
            SESSION: (session_entry(prefix), 0o644),
        }
        for name in stable:
            path = root / name
            no_symlinks(path)
            if path.exists() and str(name) not in previous["files"]:
                raise RuntimeError(f"refusing to replace unowned file: {path}")
        release = BASE / "releases" / spec["release"]
        destination = root / release
        no_symlinks(destination)
        directories(destination.parent)
        new_release = not destination.exists()
        stage = None
        completed = False
        backups = {
            name: ((root / name).read_bytes() if (root / name).exists() else None)
            for name in stable
        }
        try:
            if new_release:
                stage = Path(
                    tempfile.mkdtemp(prefix=".staging-", dir=destination.parent)
                )
                for name, expected in spec["files"].items():
                    path = stage / name
                    path.parent.mkdir(parents=True, exist_ok=True)
                    shutil.copyfile(bundle / "release" / name, path)
                    path.chmod(expected["mode"])
                    if digest(path) != expected["sha256"]:
                        raise RuntimeError(f"bundle changed during copy: {name}")
                for directory in [
                    stage,
                    *(path for path in stage.rglob("*") if path.is_dir()),
                ]:
                    directory.chmod(0o755)
                os.rename(stage, destination)
                stage = None
            elif not any(
                name.startswith(str(release) + "/") for name in previous["files"]
            ):
                raise RuntimeError(f"refusing unowned release directory: {destination}")
            files = dict(previous["files"])
            files.update(
                {str(release / name): info for name, info in spec["files"].items()}
            )
            for name, (content, mode) in stable.items():
                if name == SESSION:
                    replace_current(root, "releases/" + spec["release"])
                atomic_write(root / name, content, mode)
                files[str(name)] = {"sha256": digest(root / name), "mode": mode}
            current = "releases/" + spec["release"]
            manifest = {
                "version": 1,
                "prefix": str(prefix),
                "files": files,
                "current": current,
            }
            atomic_write(
                root / MANIFEST,
                json.dumps(manifest, sort_keys=True, indent=2).encode(),
                0o644,
            )
            completed = True
        except BaseException:
            if previous["current"]:
                replace_current(root, previous["current"])
            else:
                (root / CURRENT).unlink(missing_ok=True)
            for name, content in backups.items():
                if content is None:
                    (root / name).unlink(missing_ok=True)
                else:
                    atomic_write(root / name, content, stable[name][1])
            if new_release and destination.exists():
                shutil.rmtree(destination)
            raise
        finally:
            if stage is not None:
                shutil.rmtree(stage)
            if not completed and previous["current"] is None:
                # A failed first install has no ownership manifest. Remove the
                # empty container only after staging cleanup so retry sees just
                # our lock, rather than mistaking it for an unowned installation.
                try:
                    destination.parent.rmdir()
                except OSError:
                    pass  # Preserve any unexpected files for inspection.
    return destination


def running_processes(root):
    prefix = str(root / BASE / "releases") + "/"
    found = []
    for entry in Path("/proc").iterdir():
        if not entry.name.isdigit():
            continue
        try:
            if os.readlink(entry / "exe").startswith(prefix):
                found.append(entry.name)
        except (OSError, PermissionError):
            pass
    return found


def uninstall(root, prefix):
    if not (root / MANIFEST).exists():
        raise RuntimeError("no owned Hearthspace installation was found")
    with locked(root):
        previous = installed(root, prefix)
        running = running_processes(root)
        if running:
            raise RuntimeError(
                "log out of installed Hearthspace before uninstalling; running PIDs: "
                + ", ".join(running)
            )
        directories = {root / BASE / "releases", root / BASE}
        for name in previous["files"]:
            path = root / relative_path(name)
            path.unlink()
            directories.update(
                parent
                for parent in path.parents
                if parent != root and root in parent.parents
            )
        (root / CURRENT).unlink()
        (root / MANIFEST).unlink()
        (root / BASE / ".install.lock").unlink()
        for directory in sorted(
            directories, key=lambda path: len(path.parts), reverse=True
        ):
            try:
                directory.rmdir()
            except OSError:
                pass  # Shared directories and unowned files are never removed.
