"""Native stop waits for persistence without signalling unrelated processes."""

import os
import signal
import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from nativelib import graceful_stop, recovery


class GracefulStopTests(unittest.TestCase):
    def test_private_bus_wrapper_survives_until_owned_child_saves(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            supervisor = root / "hearthspace-session"
            supervisor.symlink_to(Path(sys.executable).resolve())
            runtime = f"/run/user/{os.getuid()}/hearthspace-native-test"
            output = root / "layout.json"
            code = "import signal,time,pathlib,sys\ndef stop(*_):\n time.sleep(.15)\n pathlib.Path(sys.argv[1]).write_text('saved')\n sys.exit(0)\nsignal.signal(signal.SIGTERM,stop)\nprint('ready',flush=True)\ntime.sleep(30)"
            state = {
                "uid": os.getuid(),
                "id": "test",
                "runtime_dir": runtime,
                "unit": "hearthspace-native-test.service",
                "binary_dir": directory,
            }
            with subprocess.Popen(
                [
                    "/usr/bin/dbus-run-session",
                    "--",
                    str(supervisor),
                    "-c",
                    code,
                    str(output),
                    "--session-dir",
                    runtime,
                ],
                stdout=subprocess.PIPE,
                text=True,
                start_new_session=True,
            ) as wrapper:
                try:
                    self.assertEqual(wrapper.stdout.readline().strip(), "ready")
                    with (
                        patch.object(recovery, "read_state", return_value=state),
                        patch.object(
                            graceful_stop,
                            "unit_info",
                            return_value={"MainPID": str(wrapper.pid)},
                        ),
                    ):
                        result = graceful_stop.request_stop("ignored")
                    self.assertTrue(result["graceful_requested"])
                    self.assertTrue(result["graceful_completed"])
                    self.assertEqual(output.read_text(), "saved")
                    self.assertEqual(wrapper.wait(timeout=2), 0)
                finally:
                    if wrapper.poll() is None:
                        os.killpg(wrapper.pid, signal.SIGKILL)
                    wrapper.wait()

    def test_process_identity_requires_executable_uid_arguments_and_parent(self):
        with subprocess.Popen(
            [sys.executable, "-c", "import time; time.sleep(30)"]
        ) as child:
            try:
                valid = {
                    "pid": child.pid,
                    "uid": os.getuid(),
                    "executable": Path(sys.executable),
                    "arguments": ["-c", "import time; time.sleep(30)"],
                    "parent": os.getpid(),
                }
                self.assertTrue(graceful_stop.process_matches(**valid))
                for changes in [
                    {"uid": os.getuid() + 1},
                    {"executable": Path("/usr/bin/true")},
                    {"arguments": ["--session-dir", "/unrelated"]},
                    {"parent": os.getpid() + 1},
                ]:
                    self.assertFalse(graceful_stop.process_matches(**(valid | changes)))
            finally:
                child.kill()
                child.wait()

    def test_signal_wait_allows_delayed_save_and_leaves_peer_alive(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "layout.json"
            code = "import signal,time,pathlib,sys\ndef stop(*_):\n time.sleep(.15)\n pathlib.Path(sys.argv[1]).write_text('saved')\n sys.exit(0)\nsignal.signal(signal.SIGTERM,stop)\nprint('ready',flush=True)\ntime.sleep(30)"
            with (
                subprocess.Popen(
                    [sys.executable, "-c", code, str(output)],
                    stdout=subprocess.PIPE,
                    text=True,
                ) as child,
                subprocess.Popen(
                    [sys.executable, "-c", "import time; time.sleep(30)"]
                ) as peer,
            ):
                pidfd = os.pidfd_open(child.pid)
                try:
                    self.assertEqual(child.stdout.readline().strip(), "ready")
                    started = time.monotonic()
                    self.assertTrue(graceful_stop.signal_and_wait(pidfd, 2))
                    self.assertGreaterEqual(time.monotonic() - started, 0.1)
                    self.assertEqual(output.read_text(), "saved")
                    self.assertIsNone(peer.poll())
                    self.assertEqual(child.wait(timeout=1), 0)
                finally:
                    os.close(pidfd)
                    child.kill()
                    peer.kill()
                    child.wait()
                    peer.wait()

    def test_timeout_is_bounded_without_force_killing(self):
        code = "import signal,time; signal.signal(signal.SIGTERM,signal.SIG_IGN); print('ready',flush=True); time.sleep(30)"
        with subprocess.Popen(
            [sys.executable, "-c", code], stdout=subprocess.PIPE, text=True
        ) as child:
            pidfd = os.pidfd_open(child.pid)
            try:
                self.assertEqual(child.stdout.readline().strip(), "ready")
                started = time.monotonic()
                self.assertFalse(graceful_stop.signal_and_wait(pidfd, 0.05))
                self.assertLess(time.monotonic() - started, 1)
                self.assertIsNone(child.poll())
            finally:
                os.close(pidfd)
                child.kill()
                child.wait()

    def test_already_exited_pidfd_cannot_signal_reused_pid(self):
        with (
            patch.object(graceful_stop, "exited", return_value=True),
            patch.object(signal, "pidfd_send_signal") as send,
        ):
            self.assertTrue(graceful_stop.signal_and_wait(10, 1))
        send.assert_not_called()

    def test_stop_refuses_changed_wrapper_identity(self):
        state = {
            "uid": 1000,
            "id": "test",
            "runtime_dir": "/run/user/1000/hearthspace-native-test",
            "unit": "hearthspace-native-test.service",
            "binary_dir": "/tmp/bin",
        }
        with (
            patch.object(recovery, "read_state", return_value=state),
            patch.object(graceful_stop, "unit_info", return_value={"MainPID": "20"}),
            patch.object(os, "pidfd_open", return_value=10),
            patch.object(os, "close"),
            patch.object(graceful_stop, "exited", return_value=False),
            patch.object(graceful_stop, "process_matches", return_value=False),
            patch.object(graceful_stop, "signal_and_wait") as stop,
        ):
            result = graceful_stop.request_stop("ignored")
        self.assertFalse(result["graceful_requested"])
        stop.assert_not_called()

    def test_stop_does_not_signal_unrelated_wrapper_child(self):
        state = {
            "uid": 1000,
            "id": "test",
            "runtime_dir": "/run/user/1000/hearthspace-native-test",
            "unit": "hearthspace-native-test.service",
            "binary_dir": "/tmp/bin",
        }
        with (
            patch.object(recovery, "read_state", return_value=state),
            patch.object(graceful_stop, "unit_info", return_value={"MainPID": "20"}),
            patch.object(os, "pidfd_open", side_effect=[10, 11]),
            patch.object(os, "close"),
            patch.object(Path, "read_text", return_value="21"),
            patch.object(graceful_stop, "exited", return_value=False),
            patch.object(graceful_stop, "process_matches", side_effect=[True, False]),
            patch.object(graceful_stop, "signal_and_wait") as stop,
        ):
            result = graceful_stop.request_stop("ignored")
        self.assertFalse(result["graceful_requested"])
        stop.assert_not_called()

    def test_identity_enumeration_stops_at_the_operation_deadline(self):
        state = {
            "uid": 1000,
            "id": "test",
            "runtime_dir": "/run/user/1000/hearthspace-native-test",
            "unit": "hearthspace-native-test.service",
            "binary_dir": "/tmp/bin",
        }
        with (
            patch.object(recovery, "read_state", return_value=state),
            patch.object(graceful_stop, "unit_info", return_value={"MainPID": "20"}),
            patch.object(os, "pidfd_open", return_value=10) as opened,
            patch.object(os, "close"),
            patch.object(Path, "read_text", return_value="21"),
            patch.object(graceful_stop, "exited", return_value=False),
            patch.object(graceful_stop, "process_matches", return_value=True),
            patch.object(graceful_stop.time, "monotonic", side_effect=[0, 7]),
            patch.object(graceful_stop, "signal_and_wait") as stop,
        ):
            result = graceful_stop.request_stop("ignored")
        self.assertFalse(result["graceful_requested"])
        self.assertEqual(result["reason"], "identity deadline exceeded")
        opened.assert_called_once_with(20)
        stop.assert_not_called()

    def test_stop_checks_direct_supervisor_and_private_runtime_before_signal(self):
        state = {
            "uid": 1000,
            "id": "test",
            "runtime_dir": "/run/user/1000/hearthspace-native-test",
            "unit": "hearthspace-native-test.service",
            "binary_dir": "/tmp/bin",
        }
        with (
            patch.object(recovery, "read_state", return_value=state),
            patch.object(graceful_stop, "unit_info", return_value={"MainPID": "20"}),
            patch.object(os, "pidfd_open", side_effect=[10, 11]),
            patch.object(os, "close"),
            patch.object(Path, "read_text", return_value="21"),
            patch.object(graceful_stop, "exited", return_value=False),
            patch.object(
                graceful_stop, "process_matches", return_value=True
            ) as identity,
            patch.object(graceful_stop, "signal_and_wait", return_value=True) as stop,
        ):
            result = graceful_stop.request_stop("ignored")
        self.assertTrue(result["graceful_completed"])
        self.assertEqual(
            identity.call_args.args,
            (
                21,
                1000,
                Path("/tmp/bin/hearthspace-session"),
                ["--session-dir", state["runtime_dir"]],
            ),
        )
        self.assertEqual(identity.call_args.kwargs, {"parent": 20})
        self.assertEqual(stop.call_args.args[0], 11)
        self.assertLessEqual(stop.call_args.args[1], graceful_stop.GRACE_SECONDS)


if __name__ == "__main__":
    unittest.main()
