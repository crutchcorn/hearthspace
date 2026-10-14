"""Give the exact native supervisor a bounded exit before systemd force cleanup."""

import os
import select
import signal
import time
from pathlib import Path

from .app_scopes import owner_identity
from .common import unit_info

GRACE_SECONDS = 6


def exited(pidfd, timeout=0):
    poll = select.poll()
    poll.register(pidfd, select.POLLIN)
    return bool(poll.poll(max(0, int(timeout * 1000))))


def process_matches(pid, uid, executable, arguments, parent=None):
    directory = Path(f"/proc/{pid}")
    try:
        if directory.stat().st_uid != uid:
            return False
        if (directory / "exe").resolve(strict=True) != executable.resolve(strict=True):
            return False
        command = (directory / "cmdline").read_bytes().split(b"\0")
        encoded = [os.fsencode(arg) for arg in arguments]
        if not any(
            command[index : index + len(encoded)] == encoded
            for index in range(len(command))
        ):
            return False
        if parent is not None:
            fields = (directory / "stat").read_text().rsplit(")", 1)[1].split()
            if int(fields[1]) != parent:
                return False
        return True
    except (FileNotFoundError, ProcessLookupError):
        return False


def signal_and_wait(pidfd, timeout):
    if exited(pidfd):
        return True
    try:
        signal.pidfd_send_signal(pidfd, signal.SIGTERM)
    except ProcessLookupError:
        return True
    return exited(pidfd, timeout)


def request_stop(state_file):
    from .recovery import read_state

    state = read_state(state_file)
    runtime, _ = owner_identity(state)
    deadline = time.monotonic() + GRACE_SECONDS
    info = unit_info(state["unit"], timeout=1)
    main = int(info.get("MainPID", "0"))
    if main <= 0:
        return {"graceful_requested": False, "reason": "unit main process has exited"}
    supervisor = Path(state["binary_dir"]) / "hearthspace-session"
    try:
        mainfd = os.pidfd_open(main)
    except ProcessLookupError:
        return {"graceful_requested": False, "reason": "unit main process has exited"}
    try:
        # The root-owned unit names this wrapper. Keep its pidfd open while
        # validating a direct child so PID reuse cannot redirect the signal.
        if exited(mainfd) or not process_matches(
            main, state["uid"], Path("/usr/bin/dbus-run-session"), [supervisor]
        ):
            return {"graceful_requested": False, "reason": "unit main identity changed"}
        try:
            children = Path(f"/proc/{main}/task/{main}/children").read_text().split()
        except FileNotFoundError:
            children = []
        for child in children[:1024]:
            try:
                childfd = os.pidfd_open(int(child))
            except ProcessLookupError:
                continue
            try:
                if (
                    exited(mainfd)
                    or exited(childfd)
                    or not process_matches(
                        int(child),
                        state["uid"],
                        supervisor,
                        ["--session-dir", str(runtime)],
                        parent=main,
                    )
                ):
                    continue
                remaining = max(0, deadline - time.monotonic())
                completed = signal_and_wait(childfd, remaining)
                return {
                    "graceful_requested": True,
                    "supervisor": int(child),
                    "graceful_completed": completed,
                }
            finally:
                os.close(childfd)
        return {
            "graceful_requested": False,
            "reason": "owned supervisor is not running",
        }
    finally:
        os.close(mainfd)
