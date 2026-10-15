"""Login cleanup must never select a foreign desktop or remove mounted data."""

import importlib.machinery
import importlib.util
import subprocess
import unittest
from pathlib import Path
from unittest.mock import patch

SOURCE = Path(__file__).resolve().parents[1] / "runtime/hearthspace-login-guard"
LOADER = importlib.machinery.SourceFileLoader("login_guard", str(SOURCE))
SPEC = importlib.util.spec_from_loader(LOADER.name, LOADER)
guard = importlib.util.module_from_spec(SPEC)
LOADER.exec_module(guard)


class LoginGuardTests(unittest.TestCase):
    def setUp(self):
        self.owner = {
            "pid": 42,
            "start": "123",
            "uid": 1000,
            "session": "108",
            "scope": "session-108.scope",
            "leader": "40",
            "leader_start": "122",
            "seat": "seat0",
            "cgroup": "/user.slice/user-1000.slice/session-108.scope",
        }
        self.properties = {
            "Id": "108",
            "User": "1000",
            "Scope": "session-108.scope",
            "Leader": "40",
            "Remote": "no",
            "Seat": "seat0",
            "Type": "wayland",
            "Class": "user",
            "Desktop": "Hearthspace",
        }

    def test_scope_resolution_cannot_fall_back_to_active_or_user_manager_session(self):
        self.assertEqual(
            guard.login_scope("0::" + self.owner["cgroup"] + "\n", 1000)[:2],
            ("108", "session-108.scope"),
        )
        for text in (
            "0::/user.slice/user-1001.slice/session-108.scope\n",
            "0::/user.slice/user-1000.slice/user@1000.service/app.slice/helper.service\n",
            "0::/user.slice/user-1000.slice/session-108.scope/child\n",
            "0::/userXslice/user-1000Xslice/session-108Xscope\n",
            "0::/system.slice/gdm.service\n",
        ):
            with self.subTest(text=text), self.assertRaises(RuntimeError):
                guard.login_scope(text, 1000)

    def test_changed_owner_seat_type_leader_and_scope_refuse_cleanup(self):
        guard.verify_session(self.properties, self.owner)
        for key, value in (
            ("Id", "2"),
            ("User", "1001"),
            ("Scope", "session-2.scope"),
            ("Leader", "41"),
            ("Seat", ""),
            ("Remote", "yes"),
            ("Type", "x11"),
            ("Class", "manager"),
        ):
            with self.subTest(key=key), self.assertRaises(RuntimeError):
                guard.verify_session(self.properties | {key: value}, self.owner)

    def test_parent_liveness_requires_uid_start_ticks_and_cgroup(self):
        valid = ("123", "0::" + self.owner["cgroup"] + "\n", 1000)
        for actual, expected in (
            (valid, True),
            (None, False),
            (("124", valid[1], 1000), False),
            (("123", "0::/foreign\n", 1000), False),
            (("123", valid[1], 1001), False),
        ):
            with patch.object(guard, "process_identity", return_value=actual):
                self.assertEqual(guard.alive(self.owner), expected)

    def test_another_graphical_desktop_cannot_be_adopted(self):
        for desktop in ("GNOME", "ubuntu", "", "Hearthspace:GNOME"):
            with self.subTest(desktop=desktop), self.assertRaises(RuntimeError):
                guard.verify_session(self.properties | {"Desktop": desktop}, self.owner)
        guard.verify_session(self.properties | {"Desktop": "hearthspace"}, self.owner)

    def test_missing_scope_is_a_completed_stop_even_with_systemctl_exit_one(self):
        output = subprocess.CompletedProcess(
            [], 1, "LoadState=not-found\nActiveState=inactive\n", ""
        )
        with patch.object(guard.subprocess, "run", return_value=output):
            self.assertTrue(guard.scope_stopped("session-108.scope"))

    def test_termination_uses_only_recorded_session_after_revalidation(self):
        with (
            patch.object(guard, "scope_stopped", side_effect=[False, True]),
            patch.object(guard, "properties", return_value=self.properties),
            patch.object(guard, "process_identity", return_value=None),
            patch.object(guard, "command") as command,
        ):
            guard.terminate(self.owner)
        command.assert_called_once_with(
            ["loginctl", "--no-ask-password", "terminate-session", "108"]
        )

    def test_reused_leader_pid_prevents_termination(self):
        with (
            patch.object(guard, "scope_stopped", return_value=False),
            patch.object(guard, "properties", return_value=self.properties),
            patch.object(
                guard, "process_identity", return_value=("999", "unused", 1000)
            ),
            patch.object(guard, "command") as command,
            self.assertRaisesRegex(RuntimeError, "reused"),
        ):
            guard.terminate(self.owner)
        command.assert_not_called()

    def test_logind_disappearance_is_accepted_only_after_scope_stop(self):
        with (
            patch.object(guard, "scope_stopped", side_effect=[False, True]),
            patch.object(guard, "properties", side_effect=RuntimeError("no session")),
            patch.object(guard, "command") as command,
        ):
            guard.terminate(self.owner)
        command.assert_not_called()

    def test_foreign_runtime_layouts_are_rejected_before_removal(self):
        for path in (
            "/run/user/1000",
            "/home/example",
            "/run/user/1000/hearthspace",
            "/run/user/1000/hearthspace/session-invalid",
            "/run/user/1001/hearthspace/session-1",
        ):
            with self.subTest(path=path), self.assertRaises(RuntimeError):
                guard.runtime_identity(Path(path), 1000)

    def test_session_ending_between_verification_and_termination_is_safe(self):
        with (
            patch.object(guard, "scope_stopped", side_effect=[False, True]),
            patch.object(guard, "properties", return_value=self.properties),
            patch.object(guard, "process_identity", return_value=None),
            patch.object(guard, "command", side_effect=RuntimeError("no session")),
        ):
            guard.terminate(self.owner)

    def test_unknown_mount_or_replaced_directory_is_never_deleted(self):
        runtime = Path("/run/user/1000/hearthspace/session-42")
        owner = self.owner | {"runtime_identity": [1, 2]}
        for inode, mounts in (((2, 3), []), ((1, 2), [(runtime / "other", "ext4")])):
            with (
                patch.object(Path, "exists", return_value=True),
                patch.object(guard, "runtime_identity", return_value=inode),
                patch.object(guard, "mounted_paths", return_value=mounts),
                patch.object(guard, "command") as command,
                patch.object(guard.shutil, "rmtree") as remove,
                self.assertRaises(RuntimeError),
            ):
                guard.cleanup_runtime(runtime, owner)
            command.assert_not_called()
            remove.assert_not_called()


if __name__ == "__main__":
    unittest.main()
