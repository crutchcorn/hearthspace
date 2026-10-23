"""Caller desktop data directories survive sudo and PAM without taking a VT."""

import os
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from nativelib import cli, service
from nativelib.environment import validated_data_dirs

DEFAULT_DATA_DIRS = "/usr/local/share:/usr/share"
CALLER_DATA_DIRS = (
    "/usr/share/gnome:/usr/local/share/:/usr/share/:/var/lib/snapd/desktop:"
    "/var/lib/flatpak/exports/share:/home/test/.local/share/flatpak/exports/share:"
    "/custom data/$literal/%n"
)


class NativeEnvironmentTests(unittest.TestCase):
    def assert_caller_handoff(self, caller_environment, expected):
        for flags in (
            [],
            ["--isolated-profile"],
            ["--login-session"],
            ["--isolated-profile", "--login-session"],
        ):
            with tempfile.TemporaryDirectory() as directory:
                args = cli.parser().parse_args(
                    ["start", "--session", directory, "--no-build", *flags]
                )
                with (
                    self.subTest(flags=flags),
                    patch.dict(os.environ, caller_environment, clear=True),
                    patch.object(cli, "preflight"),
                    patch.object(cli, "build"),
                    patch.object(cli, "privileged", return_value={}) as privileged,
                    patch.object(cli, "wait_ready", return_value={}),
                ):
                    cli.start(args)
                    command, forwarded = privileged.call_args.args
                    parsed = cli.parser().parse_args([command, *map(str, forwarded)])
                    self.assertEqual(parsed.xdg_data_dirs, expected)

    def test_public_start_preserves_caller_exported_directories_literally(self):
        self.assert_caller_handoff(
            {"XDG_DATA_DIRS": CALLER_DATA_DIRS}, CALLER_DATA_DIRS
        )

    def test_public_start_uses_xdg_defaults_when_unset(self):
        self.assert_caller_handoff({}, DEFAULT_DATA_DIRS)

    def test_public_start_uses_xdg_defaults_when_empty(self):
        self.assert_caller_handoff({"XDG_DATA_DIRS": ""}, DEFAULT_DATA_DIRS)

    def test_invalid_data_directories_are_rejected(self):
        for value in (
            7,
            False,
            [],
            {"XDG_DATA_DIRS": "/usr/share"},
            "relative/share",
            "/usr/share:relative/share",
            ":/usr/share",
            "/usr/share:",
            "/usr/share::/custom/share",
            "/usr/share\nOTHER=value",
            "/usr/share\r",
            "/usr/share\t",
            "/usr/share\x00",
            "/usr/share\x1b",
            "/usr/share\x7f",
            "/usr/share\x85",
        ):
            with (
                self.subTest(value=value),
                self.assertRaisesRegex(ValueError, "XDG_DATA_DIRS"),
            ):
                validated_data_dirs(value)

    def test_invalid_caller_value_fails_before_preflight_or_build(self):
        args = cli.parser().parse_args(["start", "--session", "/tmp/unused-native"])
        with (
            patch.dict(os.environ, {"XDG_DATA_DIRS": "/usr/share\nOTHER=value"}),
            patch.object(cli, "preflight") as preflight,
            patch.object(cli, "build") as build,
            patch.object(cli, "privileged") as privileged,
            self.assertRaisesRegex(ValueError, "XDG_DATA_DIRS"),
        ):
            cli.start(args)
        preflight.assert_not_called()
        build.assert_not_called()
        privileged.assert_not_called()

    def test_private_start_revalidates_before_allocating_a_session(self):
        args = cli.parser().parse_args(
            [
                "_start",
                "--session",
                "/tmp/unused-native",
                "--xdg-data-dirs",
                "/usr/share:relative",
            ]
        )
        with (
            patch.object(service.os, "geteuid", return_value=0),
            patch.dict(os.environ, {"SUDO_UID": "1000"}),
            patch.object(service, "allocation_lock") as allocate,
            patch.object(service, "run") as run,
            self.assertRaisesRegex(ValueError, "XDG_DATA_DIRS"),
        ):
            service.launch(args)
        allocate.assert_not_called()
        run.assert_not_called()

    def test_private_bus_restores_caller_dirs_in_each_session_mode(self):
        for isolated, login in (
            (False, False),
            (True, False),
            (False, True),
            (True, True),
        ):
            with tempfile.TemporaryDirectory() as directory:
                saved = {
                    "uid": os.getuid(),
                    "vt": 7,
                    "session_dir": directory,
                    "runtime_dir": directory + "/runtime",
                    "isolated_profile": isolated,
                    "login_session": login,
                    "wayland_display": "wayland-99",
                    "binary_dir": "/tmp/installed/bin",
                    "startup_timeout": 30,
                    "xdg_data_dirs": CALLER_DATA_DIRS,
                }
                with (
                    self.subTest(isolated=isolated, login=login),
                    patch.object(service, "load", return_value=saved),
                    patch.dict(
                        os.environ,
                        {
                            "XDG_SESSION_ID": "12",
                            "XDG_DATA_DIRS": "/pam/share",
                        },
                        clear=True,
                    ),
                    patch.object(service.os, "execve") as execute,
                ):
                    service.exec_session("ignored")
                command, env = execute.call_args.args[1:]
                self.assertIn("/usr/bin/dbus-run-session", command)
                prefix = "/tmp/installed/share:" if login else ""
                self.assertEqual(env["XDG_DATA_DIRS"], prefix + CALLER_DATA_DIRS)
                if isolated:
                    self.assertEqual(env["XDG_DATA_HOME"], directory + "/profile/data")

    def test_old_state_uses_xdg_defaults_instead_of_pam_directories(self):
        saved = {
            "uid": 1000,
            "vt": 7,
            "session_dir": "/tmp/native-test",
            "runtime_dir": "/run/private",
            "wayland_display": "wayland-99",
            "binary_dir": "/tmp/installed/bin",
            "startup_timeout": 30,
        }
        with (
            patch.object(service, "load", return_value=saved),
            patch.dict(
                os.environ,
                {"XDG_SESSION_ID": "12", "XDG_DATA_DIRS": "/pam/share"},
                clear=True,
            ),
            patch.object(service, "save"),
            patch.object(service.os, "execve") as execute,
        ):
            service.exec_session("ignored")
        self.assertEqual(execute.call_args.args[2]["XDG_DATA_DIRS"], DEFAULT_DATA_DIRS)


if __name__ == "__main__":
    unittest.main()
