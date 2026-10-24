"""Login cleanup must never select a foreign desktop or remove mounted data."""

import importlib.machinery
import importlib.util
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

SOURCE = Path(__file__).resolve().parents[1] / "runtime/hearthspace-login-guard"
LOADER = importlib.machinery.SourceFileLoader("login_guard", str(SOURCE))
SPEC = importlib.util.spec_from_loader(LOADER.name, LOADER)
guard = importlib.util.module_from_spec(SPEC)
LOADER.exec_module(guard)


class LoginGuardTests(unittest.TestCase):
    def test_loginctl_properties_use_separate_options(self):
        # loginctl accepts repeated -p options but silently ignores a comma list.
        with patch.object(
            guard, "command", return_value="Id=108\nUser=1000\n"
        ) as command:
            self.assertEqual(
                guard.properties("loginctl", "108", ("Id", "User")),
                {"Id": "108", "User": "1000"},
            )
        command.assert_called_once_with(
            [
                "loginctl",
                "--no-ask-password",
                "show-session",
                "108",
                "--property=Id",
                "--property=User",
            ],
            missing_ok=False,
        )

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
            "desktop": "Hearthspace",
            "service": "login",
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
            "Service": "login",
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
            ("Desktop", ""),
            ("Service", "gdm-password"),
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
        guard.verify_session(
            self.properties | {"Desktop": "hearthspace"},
            self.owner | {"desktop": "hearthspace"},
        )

    def test_gdm_without_logind_desktop_requires_its_original_session_selection(self):
        for service in ("gdm-password", "gdm-autologin"):
            value = self.properties | {"Desktop": "", "Service": service}
            owner = self.owner | {
                "desktop": "",
                "service": service,
                "gdm_session": "hearthspace",
            }
            with self.subTest(service=service):
                guard.verify_session(value, owner)
            for selected in (None, "", "ubuntu", "Hearthspace", "hearthspace:GNOME"):
                with self.subTest(selected=selected), self.assertRaises(RuntimeError):
                    guard.verify_session(value, owner | {"gdm_session": selected})
        for service in ("login", "gdm-launch-environment", "", "other"):
            with self.subTest(service=service), self.assertRaises(RuntimeError):
                guard.verify_session(
                    self.properties | {"Desktop": "", "Service": service},
                    self.owner
                    | {"desktop": "", "service": service, "gdm_session": "hearthspace"},
                )

    def test_gdm_selection_never_overrides_a_foreign_logind_desktop(self):
        for desktop in ("GNOME", "ubuntu", "Hearthspace:GNOME"):
            with self.subTest(desktop=desktop), self.assertRaises(RuntimeError):
                guard.verify_session(
                    self.properties | {"Desktop": desktop, "Service": "gdm-password"},
                    self.owner
                    | {
                        "desktop": desktop,
                        "service": "gdm-password",
                        "gdm_session": "hearthspace",
                    },
                )

    def test_gdm_selection_is_read_from_the_requested_process_without_ambiguity(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            process = root / "42"
            process.mkdir()
            for contents, expected in (
                (b"UNRELATED=other\0GDMSESSION=hearthspace\0", "hearthspace"),
                (b"GDMSESSION=ubuntu\0XDG_SESSION_DESKTOP=Hearthspace\0", "ubuntu"),
                (b"XDG_SESSION_DESKTOP=Hearthspace\0", ""),
                (b"GDMSESSION=hearthspace\0GDMSESSION=ubuntu\0", ""),
            ):
                (process / "environ").write_bytes(contents)
                with self.subTest(contents=contents):
                    self.assertEqual(guard.selected_gdm_session(42, root), expected)
            with self.assertRaises(FileNotFoundError):
                guard.selected_gdm_session(43, root)

    def test_establish_pins_gdm_metadata_and_rechecks_the_supervisor_identity(self):
        actual = ("123", "0::" + self.owner["cgroup"] + "\n", 1000)
        value = self.properties | {"Desktop": "", "Service": "gdm-password"}
        runtime = Path("/run/user/1000/hearthspace/session-42")
        for final, accepted in (
            (actual, True),
            (None, False),
            (("124", actual[1], 1000), False),
        ):
            with (
                patch.object(guard.os, "getuid", return_value=1000),
                patch.object(
                    guard, "process_identity", side_effect=[actual, None, final]
                ),
                patch.object(Path, "read_text", return_value="0::/user-manager\n"),
                patch.object(guard, "properties", return_value=value),
                patch.object(
                    guard, "selected_gdm_session", return_value="hearthspace"
                ) as selection,
                patch.object(guard, "runtime_identity", return_value=(1, 2)),
                self.subTest(final=final),
            ):
                if accepted:
                    identity = guard.establish(42, "123", runtime)
                    self.assertEqual(identity["desktop"], "")
                    self.assertEqual(identity["service"], "gdm-password")
                    self.assertEqual(identity["gdm_session"], "hearthspace")
                    selection.assert_called_once_with(42)
                else:
                    with self.assertRaisesRegex(RuntimeError, "supervisor.*changed"):
                        guard.establish(42, "123", runtime)

    def test_gdm_cleanup_revalidates_recorded_metadata_without_a_live_supervisor(self):
        value = self.properties | {"Desktop": "", "Service": "gdm-password"}
        owner = self.owner | {
            "desktop": "",
            "service": "gdm-password",
            "gdm_session": "hearthspace",
        }
        with (
            patch.object(guard, "scope_stopped", side_effect=[False, True]),
            patch.object(guard, "properties", return_value=value),
            patch.object(guard, "process_identity", return_value=None),
            patch.object(guard, "selected_gdm_session") as selection,
            patch.object(guard, "command") as command,
        ):
            guard.terminate(owner)
        selection.assert_not_called()
        command.assert_called_once_with(
            ["loginctl", "--no-ask-password", "terminate-session", "108"]
        )
        for changed in ({"Desktop": "Hearthspace"}, {"Service": "gdm-autologin"}):
            with (
                patch.object(guard, "scope_stopped", return_value=False),
                patch.object(guard, "properties", return_value=value | changed),
                patch.object(guard, "command") as command,
                self.subTest(changed=changed),
                self.assertRaises(RuntimeError),
            ):
                guard.terminate(owner)
            command.assert_not_called()

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

    def test_activation_retirement_precedes_runtime_deletion(self):
        with tempfile.TemporaryDirectory() as temporary:
            runtime = Path(temporary)
            (runtime / "activation-environment.json").write_text("{}")
            calls = []
            with (
                patch.object(guard, "runtime_identity", return_value=(1, 2)),
                patch.object(guard, "mounted_paths", return_value=[]),
                patch.object(
                    guard, "command", side_effect=lambda argv: calls.append(argv)
                ),
                patch.object(
                    guard.shutil, "rmtree", side_effect=lambda path: calls.append(path)
                ),
            ):
                guard.cleanup_runtime(
                    runtime, self.owner | {"runtime_identity": [1, 2]}
                )
            self.assertEqual(
                calls[0][1:], ["--stop-app-scopes", str(runtime), "/run/user/1000"]
            )
            self.assertTrue(calls[0][0].endswith("/hearthspace-app-scope-guard"))
            self.assertEqual(calls[1][1:], ["--retire-activation", str(runtime)])
            self.assertEqual(calls[2], runtime)

    def test_failed_activation_retirement_preserves_recovery_record(self):
        with tempfile.TemporaryDirectory() as temporary:
            runtime = Path(temporary)
            record = runtime / "activation-environment.json"
            record.write_text("{}")
            with (
                patch.object(guard, "runtime_identity", return_value=(1, 2)),
                patch.object(guard, "mounted_paths", return_value=[]),
                patch.object(
                    guard,
                    "command",
                    side_effect=["", RuntimeError("manager unavailable")],
                ),
                patch.object(guard.shutil, "rmtree") as remove,
                self.assertRaisesRegex(RuntimeError, "manager unavailable"),
            ):
                guard.cleanup_runtime(
                    runtime, self.owner | {"runtime_identity": [1, 2]}
                )
            remove.assert_not_called()
            self.assertTrue(record.exists())


if __name__ == "__main__":
    unittest.main()
