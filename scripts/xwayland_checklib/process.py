"""Bounded commands with private environments and owned process-group teardown."""

import os
import signal
import subprocess
import threading
import time
from contextlib import contextmanager
from pathlib import Path

SHUTDOWN_GRACE_SECONDS = 3


def exited(process):
    # Never poll()/wait() before the last group signal: the unreaped leader
    # reserves its pid/pgid even if it exits during cancellation.
    return (
        os.waitid(os.P_PID, process.pid, os.WEXITED | os.WNOHANG | os.WNOWAIT)
        is not None
    )


@contextmanager
def cancellation():
    # Python installs handlers only on its main thread. Reject unsupported use
    # before spawning rather than own a child without cancellation protection.
    if threading.current_thread() is not threading.main_thread():
        raise RuntimeError("owned commands require main-thread cancellation handling")
    requested = []
    signals = (signal.SIGTERM, signal.SIGINT)
    handlers = {signum: signal.getsignal(signum) for signum in signals}

    def defer(signum, _frame):
        if not requested:
            requested.append(signum)

    try:
        # Installation can still be interrupted by an old handler, but no
        # child exists yet. Save every old disposition before changing any.
        for signum in signals:
            signal.signal(signum, defer)
        yield requested
    finally:
        # The caller's child cleanup completes before this context unwinds.
        for signum, handler in handlers.items():
            signal.signal(signum, handler)
        if requested:
            raise RuntimeError(f"interrupted by signal {requested[0]}")


def finish(process):
    # The caller retains flag-only handlers through this entire operation;
    # neither first nor repeated cancellation can bypass group cleanup.
    try:
        if not exited(process):
            os.killpg(process.pid, signal.SIGTERM)
            deadline = time.monotonic() + SHUTDOWN_GRACE_SECONDS
            while not exited(process) and time.monotonic() < deadline:
                time.sleep(0.02)
    finally:
        # Reconfirm ownership before signaling; fail without sending a
        # signal if an unexpected reaper has taken our child (ECHILD).
        exited(process)
        os.killpg(process.pid, signal.SIGKILL)
        code = process.wait()
    return code


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def private_environment(runtime):
    return {
        "PATH": "/usr/bin:/bin",
        "HOME": str(runtime),
        "XDG_RUNTIME_DIR": str(runtime),
        "XDG_CONFIG_HOME": str(runtime / "config"),
        "XDG_CACHE_HOME": str(runtime / "cache"),
        "TMPDIR": str(runtime),
        "LC_ALL": "C",
        "CCACHE_DISABLE": "1",
    }


def invoke(
    command,
    out,
    name,
    *,
    timeout=45,
    cpu=30,
    address_space=2048 * 1024**2,
    file_size=64 * 1024**2,
    env=None,
    cwd=None,
):
    """Retain logs; allow bounded supervisor cleanup before owned-group teardown."""
    runtime = out / "tmp"
    runtime.mkdir(mode=0o700, exist_ok=True)
    environment = private_environment(runtime) if env is None else env
    with (
        cancellation() as requested,
        (out / f"{name}.stdout").open("xb") as stdout,
        (out / f"{name}.stderr").open("xb") as stderr,
    ):
        process = None
        timed_out = False
        try:
            if requested:
                raise RuntimeError(f"interrupted by signal {requested[0]}")
            process = subprocess.Popen(
                [
                    "/usr/bin/prlimit",
                    "--core=0:0",
                    f"--cpu={cpu}:{cpu + 1}",
                    f"--as={address_space}:{address_space}",
                    f"--fsize={file_size}:{file_size}",
                    "--nofile=128:128",
                    "--",
                    *(str(arg) for arg in command),
                ],
                cwd=cwd or out,
                env=environment,
                stdin=subprocess.DEVNULL,
                stdout=stdout,
                stderr=stderr,
                start_new_session=True,
            )
            deadline = time.monotonic() + timeout
            while not requested:
                if exited(process):
                    break
                if time.monotonic() >= deadline:
                    timed_out = True
                    break
                time.sleep(0.02)
        finally:
            if process is not None:
                code = finish(process)
    require(not timed_out, f"{name} exceeded resource deadline; see {out}")
    require(code == 0, f"{name} exited {code}; see {out / (name + '.stderr')}")
    return code


def fresh_output(value):
    path = Path(value)
    require(path.is_absolute(), "--output must be an absolute fresh directory")
    path.mkdir(mode=0o700)  # Existing directories/symlinks are always rejected.
    return path.resolve(strict=True)
