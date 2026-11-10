"""Bounded private source extraction and serial Meson/Ninja preparation."""

import os
import shutil
import signal
import subprocess
import tarfile
import time
from pathlib import PurePosixPath

TOOLS = ("patch", "meson", "ninja", "pkg-config", "cc", "c++")


def available_tools():
    found = {name: shutil.which(name) for name in TOOLS}
    missing = [name for name, path in found.items() if path is None]
    if missing:
        raise RuntimeError(
            "missing Xwayland preparation tools: "
            + ", ".join(missing)
            + "; install the development dependencies before running prepare-xwayland"
        )
    return found


def extract(archive, destination, version):
    """Only directories and regular files inside the pinned archive's root."""
    expected = f"xwayland-{version}"
    with tarfile.open(archive, "r:xz") as source:
        members = source.getmembers()
        seen = set()
        for member in members:
            path = PurePosixPath(member.name)
            if (
                path.is_absolute()
                or ".." in path.parts
                or not path.parts
                or path.parts[0] != expected
                or not (member.isdir() or member.isfile())
                or path in seen
            ):
                raise RuntimeError(
                    f"unsafe or duplicate source archive member: {member.name}"
                )
            seen.add(path)
        # Validate the entire archive before writing any member. Strip ownership
        # and dangerous permission bits rather than trusting archive metadata.
        for member in members:
            path = destination / member.name
            if member.isdir():
                path.mkdir(parents=True, exist_ok=True, mode=0o700)
                continue
            path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
            with source.extractfile(member) as incoming, path.open("xb") as outgoing:
                shutil.copyfileobj(incoming, outgoing)
            path.chmod(0o700 if member.mode & 0o111 else 0o600)
    return destination / expected


def environment(root, tools):
    """No inherited compiler flags, preload hooks, or user Meson configuration."""
    for name in ("home", "tmp", "cache", "config"):
        (root / name).mkdir(mode=0o700)
    return {
        "PATH": os.environ.get("PATH", os.defpath),
        "HOME": str(root / "home"),
        "TMPDIR": str(root / "tmp"),
        "XDG_CACHE_HOME": str(root / "cache"),
        "XDG_CONFIG_HOME": str(root / "config"),
        "LANG": "C.UTF-8",
        "LC_ALL": "C.UTF-8",
        "PYTHONDONTWRITEBYTECODE": "1",
        "CC": tools["cc"],
        "CXX": tools["c++"],
        "NINJA": tools["ninja"],
    }


def run(command, *, cwd, env, log, deadline):
    """A timeout/interrupt kills only this owned process group, including workers."""
    remaining = deadline - time.monotonic()
    if remaining <= 0:
        raise RuntimeError("Xwayland preparation exceeded its 1800-second budget")
    with subprocess.Popen(
        command,
        cwd=cwd,
        env=env,
        stdin=subprocess.DEVNULL,
        stdout=log,
        stderr=subprocess.STDOUT,
        start_new_session=True,
    ) as child:
        try:
            code = child.wait(timeout=remaining)
        except BaseException:
            try:
                os.killpg(child.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            child.wait()
            raise
    if code:
        raise RuntimeError(
            f"Xwayland preparation command failed ({code}); see {log.name}"
        )


def compile_source(root, source, patch, tools):
    build = root / "build"
    env = environment(root, tools)
    deadline = time.monotonic() + 1800
    commands = [
        [tools["patch"], "--batch", "--forward", "--fuzz=0", "-p1", "-i", str(patch)],
        [
            tools["meson"],
            "setup",
            str(build),
            str(source),
            "--buildtype=debug",
            "--wrap-mode=nofallback",
            f"--prefix={root / 'install'}",
            "-Dxvfb=false",
            "-Ddocs=false",
            "-Ddevel-docs=false",
            "-Dxkb_dir=/usr/share/X11/xkb",
            "-Dxkb_bin_dir=/usr/bin",
            "-Dfontrootdir=/usr/share/fonts/X11",
            "-Dsha1=libmd",
        ],
        [tools["ninja"], "-C", str(build), "-j1", "-l1", "hw/xwayland/Xwayland"],
    ]
    with (root / "build.log").open("xb") as log:
        for command in commands:
            run(command, cwd=source, env=env, log=log, deadline=deadline)
    with (root / "compiler-inputs.log").open("xb") as log:
        run(
            [tools["ninja"], "-C", str(build), "-t", "deps"],
            cwd=build,
            env=env,
            log=log,
            deadline=deadline,
        )
    return build / "hw/xwayland/Xwayland"
