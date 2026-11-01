"""Cancellation gives the owned supervisor time to close its private runtime."""

import json
import os
import signal
import sys
import tempfile
import threading
import time
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from xwayland_checklib import process, runner

SUPERVISOR = """
import os, signal, socket, sys, time
from pathlib import Path

out = Path(sys.argv[1])
runtime = out / 'private-owned'
listener = socket.socket(socket.AF_UNIX)

def cancel(signum, _frame):
    (out / 'handling').write_text('cancelling')
    time.sleep(0.15)
    listener.close()
    (runtime / 'x11').unlink()
    runtime.rmdir()
    (out / 'cleaned').write_text('closed')
    raise SystemExit(128 + signum)

signal.signal(signal.SIGTERM, cancel)
runtime.mkdir()
listener.bind(str(runtime / 'x11'))
(out / 'ready').write_text(str(os.getpid()))
signal.pause()
"""


class XwaylandCancellationTests(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.out = Path(directory.name)

    def test_repeated_runner_cancellation_allows_runtime_cleanup_and_reaps(self):
        stopped = threading.Event()
        delivery_lock = threading.Lock()
        errors = []

        def send_after(marker):
            deadline = time.monotonic() + 5
            while not (self.out / marker).is_file():
                if stopped.wait(0.01):
                    return
                if time.monotonic() >= deadline:
                    raise RuntimeError(f"missing cancellation milestone: {marker}")
            with delivery_lock:
                if not stopped.is_set():
                    os.kill(os.getpid(), signal.SIGTERM)

        def cancel():
            try:
                send_after("ready")
                send_after("handling")
            except (OSError, RuntimeError) as error:
                errors.append(str(error))

        previous = signal.signal(signal.SIGTERM, runner.interrupted)
        sender = threading.Thread(target=cancel, daemon=True)
        sender.start()
        try:
            with self.assertRaisesRegex(RuntimeError, "interrupted by signal"):
                process.invoke(
                    [sys.executable, "-B", "-c", SUPERVISOR, self.out],
                    self.out,
                    "cancelled",
                    timeout=8,
                )
        finally:
            with delivery_lock:
                stopped.set()
                signal.signal(signal.SIGTERM, previous)
            sender.join(timeout=1)
        self.assertFalse(sender.is_alive(), "cancellation sender did not stop")
        self.assertEqual(errors, [])
        self.assertEqual((self.out / "cleaned").read_text(), "closed")
        self.assertFalse((self.out / "private-owned").exists())
        pid = int((self.out / "ready").read_text())
        with self.assertRaises(ChildProcessError):
            os.waitpid(pid, os.WNOHANG)

    def cancellation_at_transition(self, boundary):
        out = self.out / boundary
        out.mkdir()
        children = []
        real_popen = process.subprocess.Popen
        real_exited = process.exited
        real_finish = process.finish
        first_sent = False

        def spawn(*args, **kwargs):
            child = real_popen(*args, **kwargs)
            children.append(child)
            deadline = time.monotonic() + 5
            while not (out / "ready").is_file():
                if real_exited(child) or time.monotonic() >= deadline:
                    raise RuntimeError("transition supervisor did not become ready")
                time.sleep(0.01)
            if boundary == "spawn-return":
                # The real child exists, but invoke has not received its Popen
                # object yet. An exception-raising handler leaks it here.
                signal.raise_signal(signal.SIGTERM)
            return child

        def poll(child):
            nonlocal first_sent
            if boundary == "cleanup-entry" and not first_sent:
                first_sent = True
                signal.raise_signal(signal.SIGTERM)
            return real_exited(child)

        def cleanup(child):
            if boundary == "cleanup-entry":
                # A second signal lands before finish can establish any local
                # protection. The old cleanup-only handlers leave this gap.
                signal.raise_signal(signal.SIGINT)
            return real_finish(child)

        handlers = {
            signal.SIGTERM: signal.signal(signal.SIGTERM, runner.interrupted),
            signal.SIGINT: signal.signal(signal.SIGINT, signal.default_int_handler),
        }
        try:
            with (
                patch.object(process.subprocess, "Popen", side_effect=spawn),
                patch.object(process, "exited", side_effect=poll),
                patch.object(process, "finish", side_effect=cleanup),
                self.assertRaises((RuntimeError, KeyboardInterrupt)) as cancelled,
            ):
                process.invoke(
                    [sys.executable, "-B", "-c", SUPERVISOR, out],
                    out,
                    "transition",
                    timeout=8,
                )
            self.assertIsInstance(cancelled.exception, RuntimeError)
            self.assertIn(
                f"interrupted by signal {signal.SIGTERM}", str(cancelled.exception)
            )
            self.assertEqual(len(children), 1)
            self.assertEqual(
                children[0].returncode,
                128 + signal.SIGTERM,
                "owned supervisor must handle cancellation and be reaped",
            )
            self.assertEqual((out / "cleaned").read_text(), "closed")
            self.assertFalse((out / "private-owned").exists())
            with self.assertRaises(ChildProcessError):
                os.waitpid(children[0].pid, os.WNOHANG)
        finally:
            try:
                # Keep the negative run against the old implementation safe:
                # reclaim only our still-unreaped child, even when it leaked.
                for child in children:
                    if child.returncode is None:
                        os.waitid(
                            os.P_PID, child.pid, os.WEXITED | os.WNOHANG | os.WNOWAIT
                        )
                        os.killpg(child.pid, signal.SIGKILL)
                        child.wait(timeout=2)
            finally:
                for signum, handler in handlers.items():
                    signal.signal(signum, handler)

    def test_cancellation_during_spawn_return_keeps_child_owned(self):
        self.cancellation_at_transition("spawn-return")

    def test_second_cancellation_at_cleanup_entry_keeps_child_owned(self):
        self.cancellation_at_transition("cleanup-entry")

    def test_unhandled_cancellation_escalates_within_bound_and_reaps(self):
        child = (
            "import json, os, signal, sys; from pathlib import Path; "
            "signal.signal(signal.SIGTERM, signal.SIG_IGN); "
            "Path(sys.argv[1]).write_text(json.dumps({'pid': os.getpid()})); "
            "signal.pause()"
        )
        start = time.monotonic()
        with (
            patch.object(process, "SHUTDOWN_GRACE_SECONDS", 0.2),
            self.assertRaisesRegex(RuntimeError, "exceeded resource deadline"),
        ):
            process.invoke(
                [sys.executable, "-B", "-c", child, self.out / "ready"],
                self.out,
                "unhandled",
                timeout=2,
            )
        self.assertLess(time.monotonic() - start, 5)
        pid = json.loads((self.out / "ready").read_text())["pid"]
        with self.assertRaises(ChildProcessError):
            os.waitpid(pid, os.WNOHANG)


if __name__ == "__main__":
    unittest.main()
