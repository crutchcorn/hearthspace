"""Owned-process fixtures and independent emergency cleanup for lifecycle tests."""

import contextlib
import os
import select
import signal
import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from headless_process import owned_process

TREE = """
import ctypes, os, signal, sys, threading, time
from pathlib import Path
ready = Path(sys.argv[1])
mode, early_exit = sys.argv[2:]

def publish():
    fields = Path('/proc/self/stat').read_text().rsplit(')', 1)[1].split()
    ready.write_text(f'{os.getpid()} {fields[19]}')

def delayed_exit(signum, frame):
    time.sleep(0.15)
    ready.with_suffix('.done').write_text('shutdown finished')
    os._exit(0)

if os.fork() == 0:
    signal.signal(signal.SIGTERM, delayed_exit if mode == 'delay' else signal.SIG_IGN)
    if mode == 'threads':
        def worker():
            publish()
            while True:
                time.sleep(0.02)
        threading.Thread(target=worker).start()
        libc = ctypes.CDLL(None)
        libc.pthread_exit.argtypes = [ctypes.c_void_p]
        libc.pthread_exit(None)
        os._exit(99)
    publish()
    while True:
        time.sleep(1)
while not ready.exists():
    time.sleep(0.01)
if early_exit == 'yes':
    os._exit(0)
while True:
    time.sleep(1)
"""


class HeadlessProcessFixture(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="hs-owned-process-")
        self.addCleanup(temporary.cleanup)
        self.directory = Path(temporary.name)
        self.sequence = 0
        self.leaders = {}

    def until(self, predicate):
        deadline = time.monotonic() + 5
        while not predicate():
            if time.monotonic() >= deadline:
                self.fail("process fixture did not become ready")
            time.sleep(0.01)

    @staticmethod
    def delayed_census(seconds):
        iterdir = Path.iterdir

        def slow(directory):
            if directory == Path("/proc"):
                time.sleep(seconds)
            return iterdir(directory)

        return patch.object(Path, "iterdir", slow)

    def start(self, stack, code, *args, term_timeout=0.05):
        process = stack.enter_context(
            owned_process(
                [sys.executable, "-c", code, *args],
                stdin=subprocess.DEVNULL,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                term_timeout=term_timeout,
                kill_timeout=2,
            )
        )
        self.addCleanup(process._close_pidfd)
        self.track_leader(process._child)
        return process

    def track_leader(self, child):
        fd = os.pidfd_open(child.pid)
        self.leaders[child.pid] = fd
        self.addCleanup(os.close, fd)
        self.addCleanup(self.force_stop, fd, child)

    def assert_reaped(self, leader):
        flags = os.WEXITED | os.WNOHANG | os.WNOWAIT
        with self.assertRaises(ChildProcessError):
            os.waitid(os.P_PIDFD, self.leaders[leader.pid], flags)

    @staticmethod
    def exited(fd, seconds=0):
        poll = select.poll()
        poll.register(fd, select.POLLIN)
        return bool(poll.poll(int(seconds * 1000)))

    def force_stop(self, fd, child=None):
        with contextlib.suppress(ProcessLookupError):
            signal.pidfd_send_signal(fd, signal.SIGKILL)
        self.assertTrue(self.exited(fd, 2), "fixture child survived emergency cleanup")
        if child is not None:
            # Inspect production reaping first; this backstop never retries stop.
            with contextlib.suppress(ChildProcessError):
                status = os.waitid(os.P_PIDFD, fd, os.WEXITED | os.WNOHANG)
                self.assertIsNotNone(status)
                child.returncode = (
                    status.si_status
                    if status.si_code == os.CLD_EXITED
                    else -status.si_status
                )

    def tree(self, stack, mode="ignore", *, early=False):
        self.sequence += 1
        ready = self.directory / f"child-{self.sequence}"
        leader = self.start(
            stack,
            TREE,
            str(ready),
            mode,
            "yes" if early else "no",
            term_timeout=1 if mode == "delay" else 0.05,
        )
        self.until(lambda: ready.exists() and len(ready.read_text().split()) == 2)
        pid, start = ready.read_text().split()
        pid = int(pid)
        fd = os.pidfd_open(pid)
        self.addCleanup(os.close, fd)
        self.assertFalse(self.exited(fd), "fixture child exited before registration")
        fields = Path(f"/proc/{pid}/stat").read_bytes().rsplit(b")", 1)[1].split()
        self.assertEqual(fields[19].decode(), start)
        self.assertFalse(self.exited(fd), "fixture child exited during registration")
        # This identity-checked pidfd also cleans up a descendant if the owner
        # under test falsely reports success and releases its group handle.
        self.addCleanup(self.force_stop, fd)
        return leader, pid, fd, ready
