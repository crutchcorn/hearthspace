"""Forward fresh external-monitor heartbeats to the service manager watchdog."""

import json
import os
import select
import socket
import subprocess
import sys
import time
from pathlib import Path

MAX_AGE_NS = 3_000_000_000


def start(state_path, entry):
    notify = os.environ.get("NOTIFY_SOCKET", "")
    if (
        not notify.startswith(("/", "@"))
        or os.environ.get("WATCHDOG_USEC") != "10000000"
    ):
        raise RuntimeError(
            "the service manager watchdog notification channel is unavailable"
        )
    return subprocess.Popen(
        [
            sys.executable,
            "-I",
            str(entry),
            "--watchdog",
            str(state_path),
            str(os.getpid()),
        ],
        stdin=subprocess.DEVNULL,
        close_fds=True,  # In particular, never inherit the advisory Cargo lock.
        env={"NOTIFY_SOCKET": notify, "WATCHDOG_USEC": "10000000"},
    )


def fresh_timestamp(path, owner, last, now):
    try:
        record = json.loads(path.read_text())
        stamp = record["sample_monotonic_ns"]
        if (
            record["owner"] == owner
            and type(stamp) is int
            and stamp > last
            and 0 <= now - stamp <= MAX_AGE_NS
        ):
            return stamp
    except (OSError, ValueError, KeyError, TypeError):
        pass
    return None


def relay(state_path, parent_pid):
    state = json.loads(state_path.read_text())
    if os.getppid() != parent_pid:
        return 1
    parent_fd = os.pidfd_open(parent_pid)
    try:
        if os.getppid() != parent_pid:
            return 1
        parent_exit = select.poll()
        parent_exit.register(parent_fd, select.POLLIN)
        notify = os.environ["NOTIFY_SOCKET"]
        address = "\0" + notify[1:] if notify.startswith("@") else notify
        with socket.socket(socket.AF_UNIX, socket.SOCK_DGRAM) as channel:
            channel.setblocking(False)
            channel.connect(address)
            last = 0
            while not parent_exit.poll(250):
                stamp = fresh_timestamp(
                    Path(state["evidence"]) / "monitor-heartbeat.json",
                    state["owner"],
                    last,
                    time.monotonic_ns(),
                )
                # Recheck after file I/O; a stalled read must not renew liveness.
                if (
                    stamp is not None
                    and 0 <= time.monotonic_ns() - stamp <= MAX_AGE_NS
                    and not parent_exit.poll(0)
                ):
                    channel.send(b"WATCHDOG=1")
                    last = stamp
        return 0
    finally:
        os.close(parent_fd)
