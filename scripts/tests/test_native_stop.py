"""Submission, cancellation, and ExecStopPost share one native seat owner."""

import os
import subprocess
import sys
import tempfile
import threading
import unittest
from contextlib import ExitStack, contextmanager
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from nativelib import cli, common, recovery, service


class NativeStopTests(unittest.TestCase):
    def setUp(self):
        self.stack = ExitStack()
        self.addCleanup(self.stack.close)
        root = Path(self.stack.enter_context(tempfile.TemporaryDirectory()))
        self.states, self.session, self.runtimes = (
            root / name for name in ("states", "session", "runtimes")
        )
        for path in (self.states, self.session, self.runtimes):
            path.mkdir()
        self.args = cli.parser().parse_args(["_start", "--session", str(self.session)])
        self.lock = threading.Lock()
        self.manifest = threading.Event()
        self.submit_gate = threading.Event()
        self.submit_gate.set()
        self.stop_waiting = threading.Event()
        self.hook_acquired = threading.Event()
        self.events, self.errors, self.workers = [], [], []
        self.accepted = False
        self.lost_reply = False
        self.stuck_stop = False
        self.unit = self.terminal()
        uid = os.getuid()

        def local_path(value):
            path = Path(value)
            if path.parent == Path(f"/run/user/{uid}"):
                return self.runtimes / path.name
            return path

        for target, name, replacement in (
            (service, "STATE_ROOT", self.states),
            (recovery, "STATE_ROOT", self.states),
            (service, "allocation_lock", self.allocation),
            (recovery, "allocation_lock", self.allocation),
        ):
            self.stack.enter_context(patch.object(target, name, replacement))
        for target, name, options in (
            (service, "Path", {"side_effect": local_path}),
            (service.os, "geteuid", {"return_value": 0}),
            (service.os, "chown", {}),
            (service, "active_vt", {"return_value": 2}),
            (service, "choose_vt", {"return_value": 7}),
            (service, "run", {"side_effect": self.submit}),
            (recovery, "run", {"side_effect": self.systemctl}),
            # Temp files are user-owned; privileged state validation is separate.
            (recovery, "read_state", {"side_effect": common.load}),
            (recovery, "recover", {"side_effect": self.cleanup}),
        ):
            self.stack.enter_context(patch.object(target, name, **options))
        self.stack.enter_context(patch.dict(os.environ, {"SUDO_UID": str(uid)}))
        self.addCleanup(self.join_workers)

    @staticmethod
    def terminal():
        return {
            "LoadState": "loaded",
            "ActiveState": "inactive",
            "SubState": "dead",
            "Job": "",
            "MainPID": "0",
            "ControlPID": "0",
        }

    @contextmanager
    def allocation(self):
        if threading.current_thread().name == "public-start":
            self.stop_waiting.set()
        if not self.lock.acquire(timeout=3):
            raise TimeoutError("allocation lock deadlocked with ExecStopPost")
        try:
            yield
        finally:
            self.lock.release()

    def worker(self, name, action):
        def run():
            try:
                action()
            except BaseException as error:  # noqa: BLE001 - assert worker failures in the parent thread
                self.errors.append((name, error))

        thread = threading.Thread(name=name, target=run, daemon=True)
        self.workers.append(thread)
        thread.start()
        return thread

    def join_workers(self):
        self.submit_gate.set()
        for thread in self.workers:
            thread.join(timeout=4)
            self.assertFalse(thread.is_alive(), f"{thread.name} did not settle")

    def saved(self):
        return common.load(self.session / "session.json")

    def submit(self, command, **_kwargs):
        self.assertEqual(command[0], "systemd-run")
        self.assertTrue(self.lock.locked())
        saved = self.saved()
        self.assertEqual(saved, common.load(saved["state_file"]))
        self.manifest.set()
        self.assertTrue(self.submit_gate.wait(timeout=3), "submission gate timed out")
        self.accepted = True
        # A queued start can still report inactive before ExecStartPre switches VT.
        self.unit = {**self.terminal(), "Job": "17"}
        self.events.append("submitted")
        if self.lost_reply:
            raise subprocess.TimeoutExpired(command, 15)
        return subprocess.CompletedProcess(command, 0, "", "")

    def systemctl(self, command, **_kwargs):
        if command[:2] == ["systemctl", "show"]:
            self.events.append("observed")
            return subprocess.CompletedProcess(
                command, 0, "\n".join(f"{k}={v}" for k, v in self.unit.items()), ""
            )
        saved = self.saved()
        self.assertEqual(command, ["systemctl", "stop", "--no-block", saved["unit"]])
        self.assertTrue(self.lock.locked(), "cancellation escaped submission barrier")
        self.events.append("cancelled")
        if self.unit != self.terminal():
            self.assertFalse(saved.get("cleanup", {}).get("complete"))
            self.assertEqual(saved["status"], "stopping")
            self.unit.update(ActiveState="deactivating", SubState="stop-post", Job="18")
            if not self.stuck_stop:
                self.worker("stop-hook", self.hook)
        return subprocess.CompletedProcess(command, 0, "", "")

    def hook(self):
        with self.allocation():
            self.hook_acquired.set()
            self.cleanup(self.saved()["state_file"], lock=False)
        self.unit = self.terminal()

    def cleanup(self, state_file, lock=True):
        if lock:
            with self.allocation():
                return self.cleanup(state_file, lock=False)
        saved = common.load(state_file)
        if saved.get("cleanup", {}).get("complete"):
            return saved
        self.assertIn(
            "cancelled", self.events, "recovered before exact unit cancellation"
        )
        if threading.current_thread().name != "stop-hook":
            self.assertEqual(
                self.unit, self.terminal(), "recovered before stop settled"
            )
        self.events.append("recovered")
        saved.update(status="stopped", cleanup={"complete": True})
        return recovery.record_state(state_file, saved)

    def test_public_wrapper_timeout_waits_for_delayed_submission_and_stop_hook(self):
        self.submit_gate.clear()

        def privileged(command, args, **_kwargs):
            if command == "_start":
                self.worker("private-start", lambda: service.launch(self.args))
                self.assertTrue(self.manifest.wait(timeout=2))
                raise subprocess.TimeoutExpired(["sudo", "_start"], 40)
            self.assertEqual(command, "_stop")
            return recovery.stop(args[1])

        public_args = cli.parser().parse_args(["start", "--session", str(self.session)])
        with (
            patch.object(cli, "preflight"),
            patch.object(cli, "build"),
            patch.object(cli, "privileged", side_effect=privileged),
        ):
            public = self.worker("public-start", lambda: cli.start(public_args))
            try:
                self.assertTrue(self.stop_waiting.wait(timeout=2))
                self.assertFalse(self.accepted)
                self.assertEqual(self.events, [], "stop observed pre-submission state")
                self.assertFalse(self.saved().get("cleanup", {}).get("complete"))
            finally:
                self.submit_gate.set()
            public.join(timeout=4)
            self.assertFalse(public.is_alive())
        self.join_workers()
        self.assertEqual(len(self.errors), 1, self.errors)
        name, error = self.errors[0]
        self.assertEqual(name, "public-start")
        self.assertIsInstance(error, subprocess.TimeoutExpired)
        self.assertLess(self.events.index("submitted"), self.events.index("cancelled"))
        self.assertLess(self.events.index("cancelled"), self.events.index("recovered"))
        self.assertTrue(
            self.hook_acquired.is_set(), "ExecStopPost could not acquire lock"
        )
        self.assertTrue(self.saved()["cleanup"]["complete"])

    def test_accepted_submission_with_lost_reply_stops_outside_allocation_lock(self):
        self.lost_reply = True
        with self.assertRaises(subprocess.TimeoutExpired):
            service.launch(self.args)
        self.join_workers()
        self.assertEqual(self.errors, [])
        self.assertTrue(self.hook_acquired.is_set())
        self.assertEqual(self.events.count("cancelled"), 1)
        self.assertTrue(self.saved()["cleanup"]["complete"])

    def test_live_unit_invalidates_old_complete_certificate_before_stop_hook(self):
        saved = service.launch(self.args)
        saved.update(status="stopped", cleanup={"complete": True})
        recovery.record_state(saved["state_file"], saved)
        self.unit.update(ActiveState="active", SubState="running", Job="", MainPID="44")
        result = recovery.stop(saved["state_file"])
        self.join_workers()
        self.assertEqual(self.errors, [])
        self.assertIn("recovered", self.events)
        self.assertTrue(result["cleanup"]["complete"])

    def test_unsettled_stop_remains_incomplete_and_blocks_new_allocation(self):
        saved = service.launch(self.args)
        self.stuck_stop = True
        with (
            patch.object(
                recovery, "unit_stopped", side_effect=TimeoutError("still stopping")
            ),
            self.assertRaisesRegex(TimeoutError, "still stopping"),
        ):
            recovery.stop(saved["state_file"])
        pending = self.saved()
        self.assertEqual(pending["status"], "stopping")
        self.assertFalse(pending["cleanup"]["complete"])
        self.assertNotIn("recovered", self.events)
        with (
            patch.object(service, "choose_vt") as choose,
            self.assertRaisesRegex(RuntimeError, "refusing to claim another VT"),
        ):
            service.launch(self.args)
        choose.assert_not_called()

    def test_old_start_without_service_still_blocks_new_allocation(self):
        saved = service.launch(self.args)
        saved["started_at"] = 0
        recovery.record_state(saved["state_file"], saved)
        self.unit = self.terminal()
        with self.assertRaisesRegex(RuntimeError, "refusing to claim another VT"):
            service.launch(self.args)
        self.assertNotIn("recovered", self.events)

    def test_old_session_stop_does_not_replace_new_current_owner(self):
        saved = service.launch(self.args)
        self.unit = self.terminal()
        newer = {"id": "another-owned-session", "status": "starting"}
        common.save(self.states / "current.json", newer)
        result = recovery.stop(saved["state_file"])
        self.assertTrue(result["cleanup"]["complete"])
        self.assertEqual(common.load(self.states / "current.json"), newer)

    def test_idempotent_stop_still_cancels_and_verifies_exact_unit(self):
        saved = service.launch(self.args)
        recovery.stop(saved["state_file"])
        self.join_workers()
        self.events.clear()
        result = recovery.stop(saved["state_file"])
        self.assertTrue(result["cleanup"]["complete"])
        self.assertEqual(self.events, ["observed", "cancelled", "observed"])

    def test_terminal_verification_rejects_pending_jobs_and_remaining_processes(self):
        for changes in (
            {"Job": "17"},
            {"MainPID": "44"},
            {"ControlPID": "45"},
            {"ActiveState": "deactivating", "SubState": "stop-post"},
        ):
            with self.subTest(changes=changes):
                self.unit = {**self.terminal(), **changes}
                self.assertFalse(recovery.unit_stopped("exact.service"))
        self.unit = self.terminal()
        self.assertTrue(recovery.unit_stopped("exact.service"))
        self.unit = {
            "LoadState": "not-found",
            "ActiveState": "inactive",
            "SubState": "dead",
            "Job": "",
        }
        self.assertTrue(recovery.unit_stopped("exact.service"))
        self.unit = {}
        self.assertFalse(recovery.unit_stopped("exact.service"))


if __name__ == "__main__":
    unittest.main()
