"""Opt-in login guard tests never claim a VT or create a real PAM session."""

import argparse
import json
import os
import sys
import tempfile
import unittest
from contextlib import ExitStack, nullcontext
from pathlib import Path
from unittest.mock import Mock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from nativelib import cli, common, recovery, service


def args(directory, login_session=False):
    return argparse.Namespace(
        session=Path(directory),
        binary_dir=Path(directory) / "bin",
        timeout=20,
        watchdog=60,
        no_build=True,
        compositor=None,
        log_level="info",
        login_session=login_session,
    )


def state(directory):
    return {
        "id": "aabbcc",
        "uid": 1000,
        "home": "/home/test",
        "unit": "hearthspace-native-aabbcc.service",
        "session_dir": str(directory),
        "runtime_dir": "/run/user/1000/hearthspace-native-aabbcc",
        "state_file": "/run/hearthspace-native/aabbcc.json",
        "original_vt": 2,
        "vt": 7,
        "watchdog": 60,
        "startup_timeout": 20,
        "binary_dir": "/tmp/installed/bin",
        "wayland_display": "wayland-99",
        "login_session": True,
    }


def installed_data(directory):
    for relative in (
        "applications/dev.hearthspace.Settings.desktop",
        "icons/hicolor/scalable/apps/hearthspace.svg",
    ):
        resource = Path(directory) / "share" / relative
        resource.parent.mkdir(parents=True, exist_ok=True)
        resource.write_text("installed resource\n")


class NativeLoginTests(unittest.TestCase):
    def test_native_preflight_requires_searchable_settings_binary(self):
        with tempfile.TemporaryDirectory() as directory:
            values = args(directory)
            values.binary_dir.mkdir()
            for name in common.BINARIES:
                if name != "hearthspace-settings":
                    executable = values.binary_dir / name
                    executable.write_text("#!/bin/sh\nexit 0\n")
                    executable.chmod(0o755)
            with self.assertRaisesRegex(
                RuntimeError, "missing executable .*hearthspace-settings"
            ):
                cli.build(values, Path(directory))

    def test_login_guard_is_opt_in_for_both_public_and_privileged_start(self):
        for command in ("start", "_start"):
            base = [command, "--session", "/tmp/native"]
            self.assertFalse(cli.parser().parse_args(base).login_session)
            self.assertTrue(
                cli.parser().parse_args([*base, "--login-session"]).login_session
            )

    def test_missing_or_nonexecutable_guard_fails_before_service_submission(self):
        with tempfile.TemporaryDirectory() as directory:
            values = args(directory, True)
            values.binary_dir.mkdir()
            helper = values.binary_dir / "hearthspace-login-guard"
            for present in (False, True):
                if present:
                    helper.write_text("#!/usr/bin/python3\n")
                    helper.chmod(0o644)
                with (
                    self.subTest(present=present),
                    patch.object(cli, "preflight"),
                    patch.object(cli, "privileged") as privileged,
                    patch.object(cli.subprocess, "run") as build,
                    self.assertRaisesRegex(RuntimeError, "installed cleanup helper"),
                ):
                    cli.start(values)
                privileged.assert_not_called()
                build.assert_not_called()

    def test_missing_shared_data_fails_before_service_submission(self):
        with tempfile.TemporaryDirectory() as directory:
            values = args(directory, True)
            values.binary_dir.mkdir()
            helper = values.binary_dir / "hearthspace-login-guard"
            helper.write_text("#!/usr/bin/python3\n")
            helper.chmod(0o755)
            installed_data(directory)
            for relative in (
                "applications/dev.hearthspace.Settings.desktop",
                "icons/hicolor/scalable/apps/hearthspace.svg",
            ):
                resource = Path(directory) / "share" / relative
                resource.unlink()
                with (
                    self.subTest(relative=relative),
                    patch.object(cli, "preflight"),
                    patch.object(cli, "privileged") as privileged,
                    self.assertRaisesRegex(RuntimeError, "installed shared data"),
                ):
                    cli.start(values)
                privileged.assert_not_called()
                installed_data(directory)

    def test_installed_guard_provenance_is_recorded_only_in_login_mode(self):
        with tempfile.TemporaryDirectory() as directory:
            values = args(directory, True)
            values.binary_dir.mkdir()
            for name in (*common.BINARIES, "hearthspace-login-guard"):
                executable = values.binary_dir / name
                executable.write_text("#!/bin/sh\nexit 1\n")
                executable.chmod(0o755)
            installed_data(directory)
            for enabled in (True, False):
                values.login_session = enabled
                cli.build(values, Path(directory))
                record = json.loads((Path(directory) / "binaries.json").read_text())
                self.assertEqual("hearthspace-login-guard" in record, enabled)
                if enabled:
                    self.assertEqual(
                        record["hearthspace-login-guard"]["path"],
                        str(values.binary_dir / "hearthspace-login-guard"),
                    )
                    self.assertEqual(
                        len(record["hearthspace-login-guard"]["sha256"]), 64
                    )

    def test_public_start_forwards_explicit_flag_only(self):
        with tempfile.TemporaryDirectory() as directory:
            for enabled in (False, True):
                values = args(directory, enabled)
                saved = state(directory)
                with (
                    patch.object(cli, "preflight"),
                    patch.object(cli, "build"),
                    patch.object(cli, "privileged", return_value=saved) as privileged,
                    patch.object(cli, "wait_ready", return_value=saved),
                    patch.object(cli, "save"),
                ):
                    cli.start(values)
                self.assertEqual(privileged.call_args.args[0], "_start")
                self.assertEqual(
                    "--login-session" in privileged.call_args.args[1], enabled
                )

    def test_launch_records_login_mode_in_both_owned_manifests(self):
        for enabled in (False, True):
            values = args("/tmp/native-test", enabled)
            with (
                patch.object(service, "allocation_lock", return_value=nullcontext()),
                patch.object(service.os, "geteuid", return_value=0),
                patch.dict(os.environ, {"SUDO_UID": "1000"}),
                patch.object(
                    service.pwd,
                    "getpwuid",
                    return_value=Mock(pw_gid=1000, pw_dir="/home/test"),
                ),
                patch.object(Path, "stat", return_value=Mock(st_uid=1000)),
                patch.object(Path, "is_symlink", return_value=False),
                patch.object(Path, "exists", return_value=False),
                patch.object(Path, "mkdir"),
                patch.object(Path, "chmod"),
                patch.object(Path, "touch"),
                patch.object(service.os, "chown"),
                patch.object(service, "active_vt", return_value=2),
                patch.object(service, "choose_vt", return_value=7),
                patch.object(service, "save") as save,
                patch.object(service, "run"),
            ):
                launched = service.launch(values)
            self.assertEqual(launched["login_session"], enabled)
            self.assertEqual(len(save.call_args_list), 3)
            for call in save.call_args_list:
                self.assertEqual(call.args[1]["login_session"], enabled)

    def test_supervisor_receives_login_flag_without_changing_private_bus(self):
        for enabled in (False, True):
            saved = state("/tmp/native-test")
            if not enabled:
                # Older manifests remain valid and retain the standard lifecycle.
                saved.pop("login_session")
            with (
                patch.object(service, "load", return_value=saved),
                patch.dict(os.environ, {"XDG_SESSION_ID": "108"}),
                patch.object(service, "save"),
                patch.object(service.os, "execve") as execute,
            ):
                service.exec_session("ignored")
            command = execute.call_args.args[1]
            self.assertEqual("--login-session" in command, enabled)
            self.assertIn("--private-session-bus", command)
            self.assertEqual(
                command[command.index("--binary-dir") + 1], saved["binary_dir"]
            )

    def test_login_environment_uses_release_data_and_disables_checkout_fallback(self):
        for enabled in (False, True):
            for inherited in (None, "", "/custom/share:/usr/share"):
                saved = state("/tmp/native-test") | {"login_session": enabled}
                original = {
                    "XDG_SESSION_ID": "108",
                    "HEARTHSPACE_DEV": "1",
                    "HEARTHSPACE_REPO_ROOT": "/checkout",
                }
                if inherited is not None:
                    original["XDG_DATA_DIRS"] = inherited
                with (
                    self.subTest(enabled=enabled, inherited=inherited),
                    patch.object(service, "load", return_value=saved),
                    patch.dict(os.environ, original, clear=True),
                    patch.object(service, "save"),
                    patch.object(service.os, "execve") as execute,
                ):
                    service.exec_session("ignored")
                environment = execute.call_args.args[2]
                if enabled:
                    self.assertEqual(
                        environment["XDG_DATA_DIRS"],
                        "/tmp/installed/share:"
                        + (inherited or "/usr/local/share:/usr/share"),
                    )
                    self.assertNotIn("HEARTHSPACE_DEV", environment)
                    self.assertNotIn("HEARTHSPACE_REPO_ROOT", environment)
                else:
                    self.assertEqual(environment.get("XDG_DATA_DIRS"), inherited)
                    self.assertEqual(environment["HEARTHSPACE_DEV"], "1")
                    self.assertEqual(environment["HEARTHSPACE_REPO_ROOT"], "/checkout")

    def test_selected_binaries_precede_post_pam_path_for_native_and_login_sessions(
        self,
    ):
        for login in (False, True):
            for inherited in (None, "", "/pam/bin:/usr/bin"):
                saved = state("/tmp/native-test") | {"login_session": login}
                original = {"XDG_SESSION_ID": "108"}
                if inherited is not None:
                    original["PATH"] = inherited
                with (
                    self.subTest(login=login, inherited=inherited),
                    patch.object(service, "load", return_value=saved),
                    patch.dict(os.environ, original, clear=True),
                    patch.object(service, "save"),
                    patch.object(service.os, "execve") as execute,
                ):
                    service.exec_session("ignored")
                self.assertEqual(
                    execute.call_args.args[2]["PATH"],
                    f"{saved['binary_dir']}:{inherited or '/usr/local/bin:/usr/bin:/bin'}",
                )

    def test_recovery_accepts_removed_owned_session_without_adopting_foreign_login(
        self,
    ):
        def result(command, **_kwargs):
            if "list-sessions" in command:
                return Mock(returncode=0, stdout="2 user\n")
            return Mock(returncode=0, stdout="User=1000\nTTY=tty2\nLeader=123\n")

        with (
            patch.object(
                recovery, "load", return_value={"session_id": "108", "leader": 456}
            ),
            patch.object(recovery, "run", side_effect=result),
        ):
            self.assertEqual(recovery.owned_sessions(state("/tmp/native-test")), [])

    def test_recovery_finishes_when_login_guard_removed_scope_and_runtime(self):
        saved = state("/tmp/native-test")
        with ExitStack() as stack:
            for target, name, value in (
                (recovery.os, "geteuid", 0),
                (recovery, "read_state", saved),
                (recovery, "active_vt", 2),
                (recovery, "owned_sessions", []),
                (recovery, "cleanup_app_scopes", True),
                (recovery, "load", {}),
                (Path, "exists", False),
            ):
                stack.enter_context(patch.object(target, name, return_value=value))
            for target, name in (
                (recovery, "detach_recovery_tty"),
                (recovery, "save"),
                (recovery, "cleanup_runtime_mounts"),
                (Path, "chmod"),
                (recovery.fcntl, "ioctl"),
            ):
                stack.enter_context(patch.object(target, name))
            stack.enter_context(patch("builtins.open"))
            command = stack.enter_context(patch.object(recovery, "run"))
            remove = stack.enter_context(patch.object(recovery.shutil, "rmtree"))
            result = recovery.recover("ignored", lock=False)
        self.assertTrue(result["cleanup"]["complete"])
        self.assertTrue(result["cleanup"]["session_scope_stopped"])
        self.assertTrue(result["cleanup"]["runtime_removed"])
        command.assert_called_once_with(["chvt", 2], timeout=unittest.mock.ANY)
        remove.assert_not_called()


if __name__ == "__main__":
    unittest.main()
