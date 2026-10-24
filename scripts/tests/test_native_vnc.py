"""Development sharing stays opt-in, local, and inside the native lifecycle."""

import json
import os
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import Mock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib import cli as agent_cli
from agentlib import native as agent_native
from nativelib import build, cli, common, service


def arguments(directory, *extra):
    return cli.parser().parse_args(
        ["start", "--session", str(directory), "--no-build", *extra]
    )


class NativeVncTests(unittest.TestCase):
    def test_sharing_rejects_implicit_dev_and_installed_overrides_before_build(self):
        for flags in (
            ["--vnc"],
            ["--dev", "--vnc-port", "5901"],
            ["--dev", "--vnc", "--vnc-port", "0"],
            ["--dev", "--vnc", "--vnc-port", "65536"],
            ["--dev", "--login-session"],
            ["--dev", "--compositor", "/bin/false"],
            ["--dev", "--binary-dir", "/opt/installed/bin"],
        ):
            with (
                self.subTest(flags=flags),
                patch.object(cli, "preflight") as preflight,
                patch.object(cli, "build") as compile,
                self.assertRaises(ValueError),
            ):
                cli.start(arguments("/tmp/unused-native", *flags))
            preflight.assert_not_called()
            compile.assert_not_called()

    def test_missing_wayvnc_is_reported_before_build_and_vt_acquisition(self):
        with (
            patch.object(
                cli.shutil,
                "which",
                side_effect=lambda name: None if name == "wayvnc" else "/usr/bin/cargo",
            ),
            patch.object(cli, "build") as compile,
            patch.object(cli, "privileged") as privileged,
            self.assertRaisesRegex(RuntimeError, "requires WayVNC"),
        ):
            cli.start(arguments("/tmp/unused-native", "--dev", "--vnc"))
        compile.assert_not_called()
        privileged.assert_not_called()

    def test_public_start_forwards_launching_path_across_sudo(self):
        with tempfile.TemporaryDirectory() as directory:
            args = arguments(directory, "--dev", "--vnc", "--vnc-port", "5909")
            saved = {"unit": "test", "session_dir": directory}
            with (
                patch.dict(os.environ, {"PATH": "/custom/rust/bin:/usr/bin"}),
                patch.object(cli.shutil, "which", return_value="/usr/bin/tool"),
                patch.object(cli, "preflight"),
                patch.object(cli, "build"),
                patch.object(cli, "privileged", return_value=saved) as privileged,
                patch.object(cli, "wait_ready", return_value=saved),
            ):
                cli.start(args)
            command = privileged.call_args.args[1]
            self.assertIn("--dev", command)
            self.assertIn("--vnc", command)
            self.assertEqual(command[command.index("--vnc-port") + 1], "5909")
            self.assertEqual(
                command[command.index("--dev-path") + 1], "/custom/rust/bin:/usr/bin"
            )

    def test_agent_forwards_dev_and_vnc_options_without_dropping_port(self):
        args = agent_cli.parser().parse_args(
            [
                "--session",
                "/tmp/native",
                "start",
                "--no-build",
                "--dev",
                "--vnc",
                "--vnc-port",
                "5909",
            ]
        )
        with patch.object(
            agent_native.subprocess,
            "run",
            return_value=Mock(returncode=0, stdout='{"ok":true}'),
        ) as run:
            agent_cli.run_launcher(args)
        command = run.call_args.args[0]
        self.assertIn("--dev", command)
        self.assertIn("--vnc", command)
        self.assertEqual(command[command.index("--vnc-port") + 1], "5909")

    def test_post_pam_execution_uses_real_dev_mode_and_original_cargo_path(self):
        saved = {
            "uid": 1000,
            "vt": 7,
            "session_dir": "/tmp/artifacts",
            "runtime_dir": "/run/private",
            "wayland_display": "wayland-99",
            "binary_dir": str(common.ROOT / "target/debug"),
            "startup_timeout": 45,
            "dev": True,
            "dev_path": "/custom/rust/bin:/usr/bin",
            "vnc": True,
            "vnc_port": 5909,
        }
        with (
            patch.object(service, "load", return_value=saved),
            patch.dict(
                os.environ,
                {
                    "XDG_SESSION_ID": "45",
                    "PATH": "/pam/bin",
                    "CARGO_TARGET_DIR": "/wrong/build",
                },
                clear=True,
            ),
            patch.object(service, "save"),
            patch.object(service.os, "execve") as execute,
        ):
            service.exec_session("ignored")
        command, environment = execute.call_args.args[1:]
        self.assertIn("--dev", command)
        self.assertNotIn("--installed", command)
        self.assertNotIn("--binary-dir", command)
        self.assertIn("--vnc", command)
        self.assertEqual(command[command.index("--vnc-port") + 1], "5909")
        self.assertEqual(
            environment["PATH"], saved["binary_dir"] + ":/custom/rust/bin:/usr/bin"
        )
        self.assertEqual(environment["CARGO_TARGET_DIR"], str(common.ROOT / "target"))

    def test_pre_vt_dev_builds_match_independent_cargo_child_feature_graphs(self):
        commands = build.development_commands()
        for package in (
            "hearthspace-compositor",
            "hearthspace-settingsd",
            "hearthspace-shell",
            "hearthspace-lock",
            "hearthspace-notifications",
            "hearthspace-settings",
        ):
            matches = [command for command in commands if package in command]
            self.assertEqual(len(matches), 1)
            command = matches[0]
            self.assertEqual(command.count("-p"), 1)
            self.assertEqual(
                "--features" in command, package == "hearthspace-compositor"
            )
            self.assertIn("--locked", command)
            self.assertEqual(
                command[command.index("--target-dir") + 1], str(common.ROOT / "target")
            )

    def test_optional_wayvnc_failure_is_returned_with_a_ready_physical_desktop(self):
        with tempfile.TemporaryDirectory() as directory:
            saved = {
                "unit": "test",
                "session_dir": directory,
                "runtime_dir": directory,
                "command_socket": "/fake/control",
                "settings_socket": "/fake/settings",
                "vnc": True,
            }
            common.save(
                Path(directory) / "wayvnc.json",
                {"ready": False, "error": "port occupied"},
            )
            common.save(
                Path(directory) / "processes.json", {"compositor": 123, "wayvnc": 0}
            )

            def control(_path, command, _timeout):
                if command == "status":
                    return json.dumps(
                        {"backend": "native", "shell_ready": True, "outputs": []}
                    ).encode()
                return b'{"frame_id":1,"presentation":"page-flip"}\n\x89PNG\r\n\x1a\n'

            with (
                patch.object(
                    common, "unit_info", return_value={"ActiveState": "active"}
                ),
                patch.object(common, "control", side_effect=control),
                patch.object(common, "settings_ready", return_value=True),
            ):
                ready = common.wait_ready(saved, 1)
            self.assertEqual(ready["status"], "ready")
            self.assertFalse(ready["wayvnc"]["ready"])
            self.assertEqual(ready["wayvnc"]["error"], "port occupied")

    def test_status_refreshes_wayvnc_failure_after_successful_startup(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            common.save(
                root / "session.json",
                {
                    "unit": "test",
                    "runtime_dir": directory,
                    "command_socket": "/fake/control",
                    "vnc": True,
                    "wayvnc": {"ready": True},
                },
            )
            common.save(root / "wayvnc.json", {"ready": False, "error": "child exited"})
            common.save(root / "processes.json", {"wayvnc": 0, "compositor": 123})
            with (
                patch.object(cli, "unit_info", return_value={"ActiveState": "active"}),
                patch.object(cli, "control", return_value=b"{}"),
            ):
                result = cli.status(root)
            self.assertFalse(result["wayvnc"]["ready"])
            self.assertEqual(result["processes"]["wayvnc"], 0)
            self.assertTrue(result["responsive"])


if __name__ == "__main__":
    unittest.main()
