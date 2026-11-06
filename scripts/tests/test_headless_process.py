"""Real inherited-group teardown regressions; no desktop or services required."""

import contextlib
import os
import signal
import subprocess
import sys
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from headless_process import owned_process
from headless_process_fixture import HeadlessProcessFixture


class HeadlessProcessTests(HeadlessProcessFixture):
    def test_context_waits_for_delayed_child_after_leader_exits(self):
        with contextlib.ExitStack() as stack:
            leader, _, fd, ready = self.tree(stack, "delay")
        self.assertEqual(ready.with_suffix(".done").read_text(), "shutdown finished")
        self.assertTrue(self.exited(fd), "context returned before descendant exit")
        self.assertEqual(leader.poll(), -signal.SIGTERM)

    def test_term_resistant_child_is_killed_after_leader_exits(self):
        with contextlib.ExitStack() as stack:
            leader, _, fd, _ = self.tree(stack)
        self.assertTrue(self.exited(fd), "TERM-resistant descendant is still live")
        self.assertEqual(leader.poll(), -signal.SIGTERM)

    def test_polling_early_exit_preserves_ownership_and_unrelated_sentinel(self):
        with contextlib.ExitStack() as outside:
            sentinel = self.start(outside, "import time; time.sleep(60)")
            with contextlib.ExitStack() as stack:
                leader, _, fd, _ = self.tree(stack, early=True)
                self.until(lambda: leader.poll() == 0)
                self.assertEqual(leader.poll(), 0)
                status = os.waitid(
                    os.P_PID, leader.pid, os.WEXITED | os.WNOHANG | os.WNOWAIT
                )
                self.assertEqual(status.si_status, 0, "poll reaped the owned leader")
            self.assertTrue(self.exited(fd))
            self.assertIsNone(sentinel.poll(), "unrelated group was signalled")
            with patch("portal_checklib.os.killpg", wraps=os.killpg) as send:
                leader.stop()
                send.assert_not_called()

    def test_zombie_main_thread_with_surviving_worker_is_drained(self):
        with contextlib.ExitStack() as stack:
            leader, pid, fd, _ = self.tree(stack, "threads", early=True)
            self.until(lambda: leader.poll() == 0)

            def zombie_main_with_worker():
                fields = (
                    Path(f"/proc/{pid}/stat").read_bytes().rsplit(b")", 1)[1].split()
                )
                return fields[0] == b"Z" and int(fields[17]) > 1

            self.until(zombie_main_with_worker)
            self.assertFalse(self.exited(fd), "worker exited before teardown")
        self.assertTrue(self.exited(fd), "zombie main thread hid a surviving worker")

    def test_unreadable_census_still_kills_and_unwinds_other_owners(self):
        stack = self.enterContext(contextlib.ExitStack())
        other = self.start(stack, "import time; time.sleep(60)")
        leader, _, fd, _ = self.tree(stack)
        iterdir = Path.iterdir

        def denied(directory):
            if directory == Path("/proc"):
                raise PermissionError("fixture cannot census /proc")
            return iterdir(directory)

        with (
            patch.object(Path, "iterdir", denied),
            patch("portal_checklib.os.killpg", wraps=os.killpg) as send,
            self.assertRaisesRegex(PermissionError, "cannot census"),
        ):
            stack.close()
        self.assertTrue(self.exited(fd, 2), "census error skipped descendant KILL")
        self.assertIn(
            (leader.pid, signal.SIGKILL), [call.args for call in send.call_args_list]
        )
        self.until(lambda: other.poll() is not None)
        self.assert_reaped(leader)
        self.assert_reaped(other)
        with (
            patch("portal_checklib.os.killpg", wraps=os.killpg) as send,
            self.assertRaisesRegex(PermissionError, "cannot census"),
        ):
            leader.stop()
        send.assert_not_called()

    def test_stat_read_failure_remains_failure_after_successful_kill(self):
        stack = self.enterContext(contextlib.ExitStack())
        leader, pid, fd, _ = self.tree(stack)
        read_bytes = Path.read_bytes
        failed = False

        def denied_once(path):
            nonlocal failed
            if path == Path(f"/proc/{pid}/stat") and not failed:
                failed = True
                raise PermissionError("fixture cannot read member stat")
            return read_bytes(path)

        with (
            patch.object(Path, "read_bytes", denied_once),
            self.assertRaisesRegex(PermissionError, "member stat"),
        ):
            stack.close()
        self.assertTrue(self.exited(fd))
        with patch("portal_checklib.os.killpg", wraps=os.killpg) as send:
            leader.stop()
            send.assert_not_called()  # Cleanup completed, but the original call failed.

    def test_term_census_deadline_escalates_and_can_succeed(self):
        stack = self.enterContext(contextlib.ExitStack())
        leader, _, fd, _ = self.tree(stack)
        with self.delayed_census(0.1):  # Exceeds TERM only; KILL can finish its census.
            stack.close()
        self.assertTrue(self.exited(fd), "census timeout skipped descendant KILL")
        self.assert_reaped(leader)

    def test_kill_census_deadline_still_fails_but_reaps_the_leader(self):
        stack = self.enterContext(contextlib.ExitStack())
        leader, _, fd, _ = self.tree(stack)
        with (
            self.delayed_census(2.1),  # Exceeds both phase budgets, including KILL.
            self.assertRaisesRegex(TimeoutError, "quiescence not verified"),
        ):
            stack.close()
        self.assertTrue(self.exited(fd))
        self.assert_reaped(leader)

    def test_lost_leader_ownership_prevents_any_group_signal(self):
        with patch("portal_checklib.os.killpg", wraps=os.killpg) as send:
            with (
                self.assertRaises(ChildProcessError),
                owned_process([sys.executable, "-c", "pass"]) as leader,
            ):
                self.until(lambda: leader.poll() == 0)
                os.waitpid(leader.pid, 0)  # Deliberately violate ownership.
            send.assert_not_called()

    def test_ownership_is_revalidated_before_escalation(self):
        stack = self.enterContext(contextlib.ExitStack())
        leader, pid, fd, _ = self.tree(stack, early=True)
        self.until(lambda: leader.poll() == 0)
        read_bytes = Path.read_bytes

        def reap_during_census(path):
            if path == Path(f"/proc/{pid}/stat"):
                os.waitpid(leader.pid, 0)  # Leader was still ours before TERM.
                raise PermissionError("fixture census lost ownership")
            return read_bytes(path)

        with (
            patch.object(Path, "read_bytes", reap_during_census),
            patch("portal_checklib.os.killpg", wraps=os.killpg) as send,
            self.assertRaisesRegex(PermissionError, "lost ownership") as raised,
        ):
            stack.close()
        self.assertEqual(
            [call.args for call in send.call_args_list], [(leader.pid, signal.SIGTERM)]
        )
        self.assertFalse(self.exited(fd), "KILL was sent without leader ownership")
        self.assertTrue(
            any("ChildProcessError" in note for note in raised.exception.__notes__)
        )

    def test_invalid_leader_stat_reaps_original_without_group_signals(self):
        for expected in (ChildProcessError, PermissionError, IndexError):
            with self.subTest(error=expected), contextlib.ExitStack() as outside:
                sentinel = self.start(outside, "import time; time.sleep(60)")
                stack = self.enterContext(contextlib.ExitStack())
                leader = self.start(stack, "pass")
                self.until(lambda leader=leader: leader.poll() == 0)
                stat = Path(f"/proc/{leader.pid}/stat").read_bytes()
                prefix, tail = stat.rsplit(b")", 1)
                fields = tail.split()
                fields[19] = str(int(fields[19]) + 1).encode()
                response = {
                    ChildProcessError: prefix + b") " + b" ".join(fields),
                    PermissionError: PermissionError("leader stat unreadable"),
                    IndexError: b"",
                }[expected]
                with patch("portal_checklib.os.killpg", wraps=os.killpg) as send:
                    with (
                        patch.object(Path, "read_bytes", side_effect=[response]),
                        self.assertRaises(expected) as raised,
                    ):
                        stack.close()
                    self.assert_reaped(leader)
                    self.assertIsNone(leader._pidfd)
                    with self.assertRaises(expected) as repeated:
                        leader.stop()
                    self.assertIs(repeated.exception, raised.exception)
                    send.assert_not_called()
                self.assertIsNone(sentinel.poll())

    def test_running_unreadable_leader_reap_timeout_preserves_safe_retry(self):
        stack = self.enterContext(contextlib.ExitStack())
        leader = self.start(stack, "import time; time.sleep(60)")
        handle = leader._pidfd
        error = PermissionError("running leader stat unreadable")
        with (
            patch.object(Path, "read_bytes", side_effect=error),
            patch("portal_checklib.os.killpg", wraps=os.killpg) as send,
            self.assertRaises(PermissionError) as raised,
        ):
            stack.close()  # The live, unsignalled child makes the real reap expire.
        self.assertIs(raised.exception, error)
        self.assertIn("TimeoutError", " ".join(error.__notes__))
        self.assertIn("recovery required", " ".join(error.__notes__))
        send.assert_not_called()
        self.assertEqual(leader._pidfd, handle)
        self.assertIsNone(leader.poll())  # Popen's placeholder is never a live status.
        with patch("subprocess._del_safe.waitpid", wraps=os.waitpid) as wait:
            leader._child.__del__()
        wait.assert_not_called()
        # Kill through the independent pidfd only after the failure assertions.
        self.force_stop(self.leaders[leader.pid])
        leader.stop(term_timeout=0.05, kill_timeout=2)
        self.assert_reaped(leader)
        self.assertEqual(leader.poll(), -signal.SIGKILL)

    def assert_failed_birth_setup(self, *, missing_handle=False, failed_reap=False):
        created, handles = [], []
        popen, pidfd_open = subprocess.Popen, os.pidfd_open
        waitid = os.waitid
        error = PermissionError("birth setup unavailable")

        def spawn(*args, **kwargs):
            child = popen(*args, **kwargs)
            self.track_leader(child)
            created.append(child)
            return child

        def acquire(pid):
            if missing_handle and created and pid == created[0].pid:
                raise error
            handle = pidfd_open(pid)
            if created and pid == created[0].pid:
                handles.append(handle)
            return handle

        def reap(kind, handle, flags):
            if failed_reap and not flags & os.WNOWAIT:
                raise TimeoutError("fixture waitid reap unavailable")
            return waitid(kind, handle, flags)

        with (
            patch("portal_checklib.subprocess.Popen", side_effect=spawn),
            patch("portal_checklib.os.pidfd_open", side_effect=acquire),
            patch("portal_checklib.os.waitid", side_effect=reap),
            patch.object(Path, "read_bytes", side_effect=error),
            self.assertRaises(PermissionError) as raised,
            owned_process([sys.executable, "-c", "import time; time.sleep(60)"]),
        ):
            self.fail("unrecorded identity was accepted")
        self.assertIs(raised.exception, error)
        child = created[0]
        self.assertIsNotNone(child.returncode)
        with patch("subprocess._del_safe.waitpid", wraps=os.waitpid) as wait:
            child.__del__()
        wait.assert_not_called()
        if missing_handle:
            self.assertFalse(self.exited(self.leaders[child.pid]))
            self.assertIn("recovery required", " ".join(error.__notes__))
        elif failed_reap:
            self.assertIn(
                "waitid reap unavailable; recovery required", " ".join(error.__notes__)
            )
            with self.assertRaises(OSError):
                os.fstat(handles[0])
            self.force_stop(self.leaders[child.pid], child)
            self.assert_reaped(child)
        else:
            self.assertEqual(child.returncode, -signal.SIGKILL)
            self.assert_reaped(child)

    def test_failed_birth_identity_capture_kills_and_reaps_original_child(self):
        self.assert_failed_birth_setup(missing_handle=False)

    def test_missing_birth_pidfd_disables_destructor_numeric_wait(self):
        self.assert_failed_birth_setup(missing_handle=True)

    def test_failed_constructor_reap_disables_destructor_numeric_wait(self):
        self.assert_failed_birth_setup(failed_reap=True)

    def test_assertion_unwinding_drains_the_owned_group(self):
        with (
            self.assertRaisesRegex(AssertionError, "workflow failed"),
            contextlib.ExitStack() as stack,
        ):
            _, _, fd, _ = self.tree(stack)
            raise AssertionError("workflow failed")
        self.assertTrue(self.exited(fd))

    def test_failed_later_spawn_cleans_up_the_earlier_owner(self):
        with self.assertRaises(FileNotFoundError), contextlib.ExitStack() as stack:
            first = self.start(stack, "import time; time.sleep(60)")
            stack.enter_context(owned_process([str(self.directory / "missing")]))
        self.assertIsNotNone(first.poll())


if __name__ == "__main__":
    unittest.main()
