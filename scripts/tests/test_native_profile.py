"""Fresh native profiles stay opt-in, private, and owned by the session user."""

import os
import stat
import sys
import tempfile
import unittest
from contextlib import nullcontext
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import Mock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib import cli as agent_cli
from agentlib import native as agent_native
from nativelib import cli, profile, service


def state(directory, enabled=True):
    return {
        "uid": os.getuid(),
        "vt": 7,
        "session_dir": str(directory),
        "runtime_dir": str(directory),
        "isolated_profile": enabled,
        "wayland_display": "wayland-99",
        "binary_dir": "/tmp/owned-bin",
        "startup_timeout": 30,
    }


class NativeProfileTests(unittest.TestCase):
    def test_public_and_privileged_parsers_require_explicit_opt_in(self):
        for command in ("start", "_start"):
            base = [command, "--session", "/tmp/native-profile"]
            self.assertFalse(cli.parser().parse_args(base).isolated_profile)
            self.assertTrue(
                cli.parser().parse_args([*base, "--isolated-profile"]).isolated_profile
            )

    def test_public_start_forwards_flag_to_privileged_start(self):
        with tempfile.TemporaryDirectory() as directory:
            for enabled in (False, True):
                flags = ["--isolated-profile"] if enabled else []
                args = cli.parser().parse_args(
                    ["start", "--session", directory, *flags]
                )
                with (
                    self.subTest(enabled=enabled),
                    patch.object(cli, "preflight"),
                    patch.object(cli, "build"),
                    patch.object(cli, "privileged", return_value={}) as privileged,
                    patch.object(cli, "wait_ready", return_value={}),
                    patch.object(cli, "save"),
                ):
                    cli.start(args)
                self.assertEqual(privileged.call_args.args[0], "_start")
                self.assertEqual(
                    "--isolated-profile" in privileged.call_args.args[1], enabled
                )

    def test_agent_wrapper_forwards_flag_without_changing_build_budget(self):
        for enabled in (False, True):
            flags = ["--isolated-profile"] if enabled else []
            args = agent_cli.parser().parse_args(
                ["--session", "/tmp/native-profile", "start", "--no-build", *flags]
            )
            with patch.object(agent_native.subprocess, "run") as run:
                run.return_value = SimpleNamespace(returncode=0, stdout='{"ok":true}')
                self.assertTrue(agent_cli.run_launcher(args)["ok"])
            self.assertEqual("--isolated-profile" in run.call_args.args[0], enabled)
            self.assertEqual(run.call_args.kwargs["timeout"], args.timeout + 30)

    def test_launcher_records_profile_choice_without_root_creating_profile(self):
        for enabled in (False, True):
            flags = ["--isolated-profile"] if enabled else []
            args = cli.parser().parse_args(
                ["_start", "--session", "/tmp/native-profile", *flags]
            )
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
                patch.object(Path, "mkdir") as mkdir,
                patch.object(Path, "chmod"),
                patch.object(Path, "touch"),
                patch.object(service.os, "chown"),
                patch.object(service, "active_vt", return_value=2),
                patch.object(service, "choose_vt", return_value=7),
                patch.object(service, "save") as save,
                patch.object(service, "run"),
                patch.object(service, "isolated_environment") as prepare,
            ):
                launched = service.launch(args)
            self.assertEqual(launched["isolated_profile"], enabled)
            self.assertEqual(len(save.call_args_list), 3)
            for call in save.call_args_list:
                self.assertEqual(call.args[1]["isolated_profile"], enabled)
            mkdir.assert_called_once_with(mode=0o700)
            prepare.assert_not_called()

    def test_default_profile_neither_reads_nor_creates_runtime_paths(self):
        with patch.object(profile, "Path") as path:
            self.assertEqual(profile.isolated_environment({}), {})
            self.assertEqual(
                profile.isolated_environment({"isolated_profile": False}), {}
            )
        path.assert_not_called()

    def test_private_profile_is_fresh_and_contains_all_xdg_homes(self):
        with tempfile.TemporaryDirectory() as directory:
            env = profile.isolated_environment(state(directory))
            self.assertEqual(env["GSETTINGS_BACKEND"], "keyfile")
            self.assertNotIn("HOME", env)
            for variable, leaf in (
                ("XDG_CONFIG_HOME", "config"),
                ("XDG_CACHE_HOME", "cache"),
                ("XDG_DATA_HOME", "data"),
                ("XDG_STATE_HOME", "state"),
            ):
                target = Path(directory) / "profile" / leaf
                self.assertEqual(env[variable], str(target))
                self.assertEqual(stat.S_IMODE(target.stat().st_mode), 0o700)
                self.assertEqual(target.stat().st_uid, os.getuid())
                self.assertEqual(list(target.iterdir()), [])
            with self.assertRaises(FileExistsError):
                profile.isolated_environment(state(directory))

    def test_root_and_wrong_user_cannot_create_a_profile(self):
        with tempfile.TemporaryDirectory() as directory:
            saved = state(directory)
            for uid in (0, saved["uid"] + 1):
                with (
                    self.subTest(uid=uid),
                    patch.object(profile.os, "geteuid", return_value=uid),
                    self.assertRaisesRegex(RuntimeError, "session user"),
                ):
                    profile.isolated_environment(saved)
            self.assertEqual(list(Path(directory).iterdir()), [])

    def test_artifact_symlink_cannot_redirect_profile_creation(self):
        with tempfile.TemporaryDirectory() as directory:
            parent = Path(directory)
            destination = parent / "destination"
            destination.mkdir()
            link = parent / "linked-artifacts"
            link.symlink_to(destination, target_is_directory=True)
            with self.assertRaisesRegex(RuntimeError, "owned artifact"):
                profile.isolated_environment(state(link))
            self.assertEqual(list(destination.iterdir()), [])

    def test_profile_symlink_cannot_reuse_or_modify_another_directory(self):
        with tempfile.TemporaryDirectory() as directory:
            parent = Path(directory)
            destination = parent / "destination"
            destination.mkdir()
            (parent / "profile").symlink_to(destination, target_is_directory=True)
            with self.assertRaises(FileExistsError):
                profile.isolated_environment(state(directory))
            self.assertEqual(list(destination.iterdir()), [])

    def test_private_bus_and_supervisor_inherit_profile_with_real_home(self):
        with tempfile.TemporaryDirectory() as directory:
            saved = state(directory)
            runtime = Path(directory) / "runtime"
            runtime.mkdir()
            saved["runtime_dir"] = str(runtime)
            with (
                patch.object(service, "load", return_value=saved),
                patch.dict(
                    os.environ,
                    {
                        "XDG_SESSION_ID": "12",
                        "HOME": "/home/real-user",
                        "XDG_CONFIG_HOME": "/host/config",
                        "XDG_STATE_HOME": "/host/state",
                        "GSETTINGS_BACKEND": "dconf",
                    },
                ),
                patch.object(service.os, "execve") as execute,
            ):
                service.exec_session("ignored")
            env = execute.call_args.args[2]
            self.assertEqual(env["HOME"], "/home/real-user")
            self.assertEqual(env["XDG_RUNTIME_DIR"], str(runtime))
            self.assertEqual(list(runtime.iterdir()), [])
            self.assertEqual(env["GSETTINGS_BACKEND"], "keyfile")
            for key in ("CONFIG", "CACHE", "DATA", "STATE"):
                self.assertEqual(
                    env[f"XDG_{key}_HOME"],
                    str(Path(directory) / "profile" / key.lower()),
                )
                self.assertTrue(Path(env[f"XDG_{key}_HOME"]).is_dir())
            self.assertIn("/usr/bin/dbus-run-session", execute.call_args.args[1])


if __name__ == "__main__":
    unittest.main()
