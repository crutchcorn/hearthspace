"""Login runtime deletion through a real, isolated cleanup-helper boundary."""

import concurrent.futures
import importlib.machinery
import importlib.util
import tempfile
import time
import unittest
from pathlib import Path
from unittest.mock import patch

SOURCE = Path(__file__).resolve().parents[1] / "runtime/hearthspace-login-guard"
LOADER = importlib.machinery.SourceFileLoader("login_guard_cleanup", str(SOURCE))
SPEC = importlib.util.spec_from_loader(LOADER.name, LOADER)
guard = importlib.util.module_from_spec(SPEC)
LOADER.exec_module(guard)


def helper_script(root, body):
    binary = root / "hearthspace-app-scope-guard"
    binary.write_text(
        "#!/usr/bin/python3 -I\n"
        "import signal, sys, time\nfrom pathlib import Path\n"
        "signal.alarm(4)\n"
        "runtime = Path(sys.argv[2])\nroot = runtime.parent\n" + body
    )
    binary.chmod(0o700)


class LoginGuardCleanupTests(unittest.TestCase):
    def test_delayed_app_scopes_keep_runtime_until_helper_barrier_completes(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            runtime = root / "session-42"
            runtime.mkdir()
            record = runtime / "app-scope-owner"
            record.write_text("hearthspace-apps-session-42.service")
            helper_script(
                root,
                "assert sys.argv[1] == '--stop-app-scopes'\n"
                "assert sys.argv[3] == '/run/user/1000'\n"
                "(root / 'barrier-entered').touch()\n"
                "while not (root / 'release-scope').exists(): time.sleep(0.01)\n",
            )
            with (
                patch.object(guard, "__file__", str(root / "hearthspace-login-guard")),
                patch.object(guard, "runtime_identity", return_value=(1, 2)),
                patch.object(guard, "mounted_paths", return_value=[]),
                concurrent.futures.ThreadPoolExecutor(max_workers=1) as executor,
            ):
                finished = executor.submit(
                    guard.cleanup_runtime,
                    runtime,
                    {"uid": 1000, "runtime_identity": [1, 2]},
                )
                try:
                    deadline = time.monotonic() + 3
                    while not (root / "barrier-entered").exists():
                        if finished.done():
                            finished.result()
                            self.fail(
                                "cleanup completed without waiting for app scopes"
                            )
                        self.assertLess(time.monotonic(), deadline)
                        time.sleep(0.01)
                    self.assertFalse(finished.done())
                    self.assertTrue(record.is_file())
                finally:
                    (root / "release-scope").touch()
                finished.result(timeout=3)
            self.assertFalse(runtime.exists())

    def test_scope_stop_failure_preserves_runtime_and_activation_record(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            runtime = root / "session-42"
            runtime.mkdir()
            record = runtime / "activation-environment.json"
            record.write_text("{}")
            helper_script(
                root,
                "assert sys.argv[1] == '--stop-app-scopes'\n"
                "print('scope stop failed', file=sys.stderr)\nsys.exit(7)\n",
            )
            with (
                patch.object(guard, "__file__", str(root / "hearthspace-login-guard")),
                patch.object(guard, "runtime_identity", return_value=(1, 2)),
                patch.object(guard, "mounted_paths", return_value=[]),
                self.assertRaisesRegex(RuntimeError, "scope stop failed"),
            ):
                guard.cleanup_runtime(
                    runtime, {"uid": 1000, "runtime_identity": [1, 2]}
                )
            self.assertTrue(record.is_file())

    def test_retirement_failure_preserves_records_after_scopes_stop(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            runtime = root / "session-42"
            runtime.mkdir()
            record = runtime / "activation-environment.json"
            record.write_text("{}")
            owner = runtime / "app-scope-owner"
            owner.write_text("hearthspace-apps-session-42.service")
            helper_script(
                root,
                "if sys.argv[1] == '--stop-app-scopes':\n"
                "    (root / 'scopes-stopped').touch()\n"
                "elif sys.argv[1] == '--retire-activation':\n"
                "    assert (root / 'scopes-stopped').exists()\n"
                "    print('retirement failed', file=sys.stderr)\n"
                "    sys.exit(7)\n"
                "else: sys.exit(99)\n",
            )
            with (
                patch.object(guard, "__file__", str(root / "hearthspace-login-guard")),
                patch.object(guard, "runtime_identity", return_value=(1, 2)),
                patch.object(guard, "mounted_paths", return_value=[]),
                self.assertRaisesRegex(RuntimeError, "retirement failed"),
            ):
                guard.cleanup_runtime(
                    runtime, {"uid": 1000, "runtime_identity": [1, 2]}
                )
            self.assertTrue(record.is_file())
            self.assertTrue(owner.is_file())

    def test_known_mounts_detach_only_after_scope_barrier_and_retirement(self):
        with tempfile.TemporaryDirectory() as directory:
            runtime = Path(directory)
            (runtime / "activation-environment.json").write_text("{}")
            mounts = [(runtime / "doc", "fuse.portal")]
            with (
                patch.object(guard, "runtime_identity", return_value=(1, 2)),
                patch.object(guard, "mounted_paths", side_effect=[mounts, mounts, []]),
                patch.object(guard, "command", return_value="") as command,
            ):
                guard.cleanup_runtime(
                    runtime, {"uid": 1000, "runtime_identity": [1, 2]}
                )
            calls = [call.args[0] for call in command.call_args_list]
            self.assertEqual(calls[0][1], "--stop-app-scopes")
            self.assertEqual(calls[1][1], "--retire-activation")
            self.assertEqual(
                calls[2], ["fusermount3", "-u", "-z", "--", str(runtime / "doc")]
            )
            self.assertFalse(runtime.exists())

    def test_runtime_replacement_during_scope_shutdown_prevents_removal(self):
        with tempfile.TemporaryDirectory() as directory:
            runtime = Path(directory)
            record = runtime / "activation-environment.json"
            record.write_text("replacement session's activation receipt")
            with (
                patch.object(guard, "runtime_identity", side_effect=[(1, 2), (1, 3)]),
                patch.object(guard, "mounted_paths", return_value=[]),
                patch.object(guard, "command", return_value="") as command,
                self.assertRaisesRegex(RuntimeError, "runtime directory was replaced"),
            ):
                guard.cleanup_runtime(
                    runtime, {"uid": 1000, "runtime_identity": [1, 2]}
                )
            self.assertTrue(runtime.is_dir())
            self.assertTrue(record.is_file())
            self.assertEqual(command.call_count, 1)

    def test_unknown_mount_added_during_scope_shutdown_prevents_removal(self):
        with tempfile.TemporaryDirectory() as directory:
            runtime = Path(directory)
            with (
                patch.object(guard, "runtime_identity", return_value=(1, 2)),
                patch.object(
                    guard,
                    "mounted_paths",
                    side_effect=[[], [(runtime / "other", "ext4")]],
                ),
                patch.object(guard, "command", return_value="") as command,
                self.assertRaisesRegex(RuntimeError, "unknown mount"),
            ):
                guard.cleanup_runtime(
                    runtime, {"uid": 1000, "runtime_identity": [1, 2]}
                )
            self.assertEqual(command.call_count, 1)
            self.assertTrue(runtime.is_dir())


if __name__ == "__main__":
    unittest.main()
