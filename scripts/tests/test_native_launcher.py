"""Host-independent coverage for native session allocation/readiness/recovery."""

import argparse
import json
import os
import socket
import sys
import tempfile
import threading
import time
import unittest
from contextlib import ExitStack, nullcontext
from pathlib import Path
from unittest.mock import Mock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from nativelib import cli, common, recovery, service


def state(directory):
    return {
        "id": "test",
        "uid": 1000,
        "home": "/home/test",
        "unit": "hearthspace-native-test.service",
        "session_dir": str(directory),
        "runtime_dir": "/run/user/1000/hearthspace-native-test",
        "state_file": "/run/hearthspace-native/test.json",
        "original_vt": 2,
        "vt": 7,
        "watchdog": 60,
        "startup_timeout": 30,
        "binary_dir": "/tmp/bin",
        "command_socket": "/tmp/test.sock",
        "wayland_display": "wayland-99",
        "settings_socket": "/tmp/settings.sock",
    }


class NativeServiceTests(unittest.TestCase):
    def test_incomplete_previous_recovery_blocks_new_vt_claim(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            previous = {**state(root), "status": "cleanup_failed", "started_at": 0}
            common.save(root / "current.json", previous)
            args = argparse.Namespace(session=root)
            with (
                patch.object(service, "STATE_ROOT", root),
                patch.object(service, "allocation_lock", return_value=nullcontext()),
                patch.object(service.os, "geteuid", return_value=0),
                patch.dict(os.environ, {"SUDO_UID": str(os.getuid())}),
                patch.object(
                    service, "unit_info", return_value={"ActiveState": "failed"}
                ),
                patch.object(
                    recovery, "recover", return_value={"cleanup": {"complete": False}}
                ),
                patch.object(service, "choose_vt") as claim,
                self.assertRaisesRegex(RuntimeError, "refusing to claim"),
            ):
                service.launch(args)
            claim.assert_not_called()

    def test_service_owns_real_pam_session_and_independent_recovery(self):
        args = service.service_command(state("/tmp/session with spaces"))
        for prop in (
            "User=1000",
            "PAMName=login",
            "StandardInput=tty-fail",
            "TTYPath=/dev/tty7",
            "KillMode=control-group",
            "RuntimeMaxSec=60",
            "TimeoutStopSec=12",
        ):
            self.assertIn(f"--property={prop}", args)
        cleanup = next(
            arg for arg in args if arg.startswith("--property=ExecStopPost=")
        )
        self.assertIn("=+/usr/bin/python3", cleanup)
        self.assertIn("_recover --state", cleanup)
        self.assertIn("--setenv=LIBSEAT_BACKEND=logind", args)
        self.assertIn("--no-block", args)
        self.assertNotIn("tty-force", " ".join(map(str, args)))

    def test_choose_vt_skips_open_and_existing_sessions(self):
        sessions = Mock(stdout="2 1000 user seat0 111 user tty7 no -\n")

        def ioctl(_file, _operation, buffer, _mutate):
            buffer[0] = 2
            buffer[2] = (1 << 2) | (1 << 8)

        with (
            patch.object(service, "run", return_value=sessions),
            patch("builtins.open"),
            patch.object(service.fcntl, "ioctl", side_effect=ioctl),
        ):
            self.assertEqual(service.choose_vt(), 9)

    def test_dbus_activation_inherits_only_private_desktop_environment(self):
        with tempfile.TemporaryDirectory() as directory:
            saved = state(directory)
            with (
                patch.object(service, "load", return_value=saved),
                patch.dict(
                    os.environ,
                    {
                        "XDG_SESSION_ID": "12",
                        "DISPLAY": ":0",
                        "DBUS_SESSION_BUS_ADDRESS": "unix:path=/host/bus",
                    },
                ),
                patch.object(service.os, "execve") as execute,
            ):
                service.exec_session("ignored")
            env = execute.call_args.args[2]
            self.assertEqual(env["XDG_RUNTIME_DIR"], saved["runtime_dir"])
            self.assertEqual(env["WAYLAND_DISPLAY"], saved["wayland_display"])
            self.assertEqual(env["HEARTHSPACE_HOST_XDG_RUNTIME_DIR"], "/run/user/1000")
            self.assertNotIn("DISPLAY", env)
            self.assertNotIn("DBUS_SESSION_BUS_ADDRESS", env)

    def test_systemd_command_escapes_specifiers_and_variables(self):
        value = service.property_command(
            ["/usr/bin/python3", "/tmp/a %n $USER 'quoted'/x"]
        )
        self.assertIn("%%n", value)
        self.assertIn("$$USER", value)
        self.assertIn("'\"'\"'", value)

    def test_recovery_identifies_exact_pam_session(self):
        def result(args, **_kwargs):
            if "list-sessions" in args:
                return Mock(returncode=0, stdout="10 user\n11 user\n12 user\n")
            for name in ("User", "TTY", "Leader"):
                self.assertIn(f"--property={name}", args)
            session_id = args[2]
            return Mock(
                returncode=0,
                stdout=f"User=1000\nTTY=tty7\nLeader={123 if session_id != '12' else 456}\n",
            )

        identity = {"session_id": "11", "leader": 123}
        with (
            patch.object(recovery, "load", return_value=identity),
            patch.object(recovery, "run", side_effect=result),
        ):
            self.assertEqual(recovery.owned_sessions(state("/tmp/session")), ["11"])

    def test_pre_identity_failure_uses_systemd_recorded_main_pid(self):
        def result(args, **_kwargs):
            if "list-sessions" in args:
                return Mock(returncode=0, stdout="10 user\n11 user\n")
            return Mock(
                returncode=0,
                stdout=f"User=1000\nTTY=tty7\nLeader={123 if args[2] == '10' else 456}\n",
            )

        with (
            patch.object(recovery, "load", side_effect=FileNotFoundError),
            patch.object(recovery, "unit_info", return_value={"ExecMainPID": "123"}),
            patch.object(recovery, "run", side_effect=result),
        ):
            self.assertEqual(recovery.owned_sessions(state("/tmp/session")), ["10"])

    def test_logind_query_failure_cannot_certify_cleanup(self):
        def result(args, **_kwargs):
            if "list-sessions" in args:
                return Mock(returncode=0, stdout="11 user\n")
            return Mock(returncode=1, stdout="", stderr="D-Bus disconnected")

        with (
            patch.object(
                recovery, "load", return_value={"session_id": "11", "leader": 123}
            ),
            patch.object(recovery, "run", side_effect=result),
            patch.object(recovery, "unit_info", return_value={"ActiveState": "active"}),
            self.assertRaisesRegex(RuntimeError, "cannot verify"),
        ):
            recovery.owned_sessions(state("/tmp/session"))

    def test_recovery_detaches_inherited_spare_tty(self):
        with (
            patch.object(recovery.os, "isatty", return_value=True),
            patch.object(recovery.os, "ttyname", return_value="/dev/tty7"),
            patch.object(recovery.os, "dup2") as dup,
            patch.object(recovery.signal, "signal"),
            patch.object(recovery.fcntl, "ioctl") as ioctl,
            patch("builtins.open"),
        ):
            recovery.detach_recovery_tty(state("/tmp/session"))
        self.assertEqual(ioctl.call_args.args[1], recovery.termios.TIOCNOTTY)
        self.assertEqual(dup.call_args.args[1], 0)

    def test_teardown_is_idempotent_after_success(self):
        saved = {
            **state("/tmp/session"),
            "cleanup": {"complete": True},
            "status": "stopped",
        }
        with (
            patch.object(recovery.os, "geteuid", return_value=0),
            patch.object(recovery, "read_state", return_value=saved),
            patch.object(recovery, "run") as run,
        ):
            self.assertEqual(recovery.recover("ignored", lock=False), saved)
            run.assert_not_called()

    def test_recovery_always_records_vt_release_failure(self):
        with ExitStack() as stack:
            stack.enter_context(patch.object(recovery.os, "geteuid", return_value=0))
            stack.enter_context(
                patch.object(recovery, "read_state", return_value=state("/tmp/session"))
            )
            stack.enter_context(patch.object(recovery, "active_vt", return_value=2))
            stack.enter_context(
                patch.object(recovery, "owned_sessions", return_value=[])
            )
            stack.enter_context(patch.object(recovery, "run"))
            stack.enter_context(patch.object(recovery, "load", return_value={}))
            stack.enter_context(patch.object(recovery, "save"))
            stack.enter_context(patch.object(Path, "chmod"))
            stack.enter_context(patch.object(Path, "exists", return_value=False))
            stack.enter_context(patch("builtins.open"))
            stack.enter_context(
                patch.object(recovery.fcntl, "ioctl", side_effect=OSError("VT busy"))
            )
            result = recovery.recover("ignored", lock=False)
        self.assertFalse(result["cleanup"]["complete"])
        self.assertTrue(result["cleanup"]["vt_restored"])
        self.assertEqual(result["status"], "cleanup_failed")


class NativeReadinessTests(unittest.TestCase):
    def test_control_reads_fragmented_length_framed_reply(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "socket"
            payload = b"x" * 200_000
            listener = socket.socket(socket.AF_UNIX)
            listener.bind(str(path))
            listener.listen()

            def serve():
                stream, _ = listener.accept()
                with stream:
                    self.assertEqual(stream.recv(128), b"capture\n")
                    for part in (
                        b"o",
                        b"k ",
                        str(len(payload)).encode(),
                        b"\n",
                        payload[:500],
                        payload[500:],
                    ):
                        stream.sendall(part)
                listener.close()

            worker = threading.Thread(target=serve)
            worker.start()
            self.assertEqual(common.control(path, "capture", 2), payload)
            worker.join()

    def test_native_readiness_requires_page_flip_and_shell(self):
        with tempfile.TemporaryDirectory() as directory:
            saved = state(directory)
            status = {
                "backend": "native",
                "shell_ready": True,
                "outputs": [{"name": "Virtual-1"}],
            }

            def capture(_path, command, _timeout):
                if command == "status":
                    return json.dumps(status).encode()
                return b'{"frame_id":1,"presentation":"offscreen"}\n\x89PNG\r\n\x1a\n'

            with (
                patch.object(
                    common, "unit_info", return_value={"ActiveState": "active"}
                ),
                patch.object(common, "control", side_effect=capture),
                patch.object(common, "settings_ready", return_value=True),
                self.assertRaisesRegex(TimeoutError, "presented frame"),
            ):
                common.wait_ready(saved, 0.02)
            self.assertFalse((Path(directory) / "ready.png").exists())

    def test_ready_preserves_evidence_and_child_identity(self):
        with tempfile.TemporaryDirectory() as directory:
            saved = state(directory)
            status = {
                "backend": "native",
                "shell_ready": True,
                "outputs": [{"name": "Virtual-1"}],
            }
            png = b"\x89PNG\r\n\x1a\nimage"

            def capture(_path, command, _timeout):
                if command == "status":
                    return json.dumps(status).encode()
                return b'{"frame_id":4,"presentation":"page-flip"}\n' + png

            with (
                patch.object(
                    common, "unit_info", return_value={"ActiveState": "active"}
                ),
                patch.object(common, "control", side_effect=capture),
                patch.object(common, "settings_ready", return_value=True),
                patch.object(common, "load", return_value={"compositor": 123}),
            ):
                result = common.wait_ready(saved, 1)
            self.assertEqual(result["compositor_pid"], 123)
            self.assertEqual((Path(directory) / "ready.png").read_bytes(), png)

    def test_each_readiness_step_receives_only_remaining_budget(self):
        with tempfile.TemporaryDirectory() as directory:

            def capture(_path, command, _timeout):
                if command == "status":
                    time.sleep(0.02)
                    return b'{"backend":"native","shell_ready":true,"outputs":[]}'
                return b'{"frame_id":1,"presentation":"page-flip"}\n\x89PNG\r\n\x1a\n'

            with (
                patch.object(
                    common, "unit_info", return_value={"ActiveState": "active"}
                ) as info,
                patch.object(common, "control", side_effect=capture),
                patch.object(common, "settings_ready", return_value=True) as settings,
                patch.object(common, "load", return_value={"compositor": 123}),
            ):
                common.wait_ready(state(directory), 0.05)
            self.assertLessEqual(info.call_args.kwargs["timeout"], 0.05)
            self.assertLess(settings.call_args.args[1], 0.04)

    def test_startup_failure_cleans_accepted_service(self):
        with tempfile.TemporaryDirectory() as directory:
            saved = state(directory)
            common.save(Path(directory) / "session.json", saved)
            args = argparse.Namespace(
                session=Path(directory),
                binary_dir=Path("/tmp/bin"),
                timeout=20,
                watchdog=60,
                no_build=True,
                compositor=None,
                log_level="info",
            )
            # Permit an existing fixture manifest only for simulating acceptance.
            with (
                patch.object(cli, "preflight"),
                patch.object(Path, "exists", return_value=False),
                patch.object(cli, "build"),
                patch.object(
                    cli, "privileged", side_effect=[saved, saved]
                ) as privileged,
                patch.object(
                    cli, "wait_ready", side_effect=TimeoutError("hung compositor")
                ),
                self.assertRaises(TimeoutError),
            ):
                cli.start(args)
            self.assertEqual(privileged.call_args_list[-1].args[0], "_stop")

    def test_build_failure_never_submits_a_service(self):
        with tempfile.TemporaryDirectory() as directory:
            args = argparse.Namespace(
                session=Path(directory),
                binary_dir=Path(directory),
                timeout=20,
                watchdog=60,
                no_build=True,
                compositor=None,
                log_level="info",
            )
            with (
                patch.object(cli, "preflight"),
                patch.object(cli, "privileged") as privileged,
                self.assertRaisesRegex(RuntimeError, "missing executable"),
            ):
                cli.start(args)
            privileged.assert_not_called()


if __name__ == "__main__":
    unittest.main()
