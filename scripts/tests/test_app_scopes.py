import sys
import tempfile
import time
import types
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from nativelib import app_scopes


class AppScopeRecoveryTests(unittest.TestCase):
    def setUp(self):
        self.state = {
            "uid": 1000,
            "id": "abc123",
            "runtime_dir": "/run/user/1000/hearthspace-native-abc123",
        }
        self.unit = "hearthspace-apps-hearthspace-native-abc123.service"
        self.scope = "snap.firefox.firefox-0123.scope"
        self.ptyxis_scope = "ptyxis-spawn-01234567-89ab-cdef-0123-456789abcdef.scope"
        self.deadline = time.monotonic() + 3

    def response(self, text, code=0):
        return types.SimpleNamespace(stdout=text, stderr="", returncode=code)

    def test_owner_name_comes_from_validated_root_state(self):
        self.assertEqual(app_scopes.owner_identity(self.state)[1], self.unit)
        for change in [
            {"runtime_dir": "/tmp/hearthspace-native-abc123"},
            {"id": "elsewhere"},
        ]:
            with self.subTest(change=change), self.assertRaises(RuntimeError):
                app_scopes.owner_identity(self.state | change)

    def test_missing_owner_requires_no_stop_or_host_environment_change(self):
        with (
            patch.object(Path, "exists", return_value=False),
            patch.object(
                app_scopes,
                "run",
                return_value=self.response("LoadState=not-found\n", 1),
            ) as run,
        ):
            self.assertTrue(app_scopes.cleanup_app_scopes(self.state, self.deadline))
        self.assertEqual(run.call_count, 1)
        self.assertIn("show", run.call_args.args[0])
        self.assertNotIn("import-environment", run.call_args.args[0])

    def test_stop_checks_owner_description_and_all_bound_scopes(self):
        initial = self.response(
            f"LoadState=loaded\nActiveState=active\nDescription=Hearthspace app scopes for {self.state['runtime_dir']}\nBoundBy={self.scope}\n"
        )
        replies = [
            initial,
            self.response(""),
            self.response("LoadState=not-found\n"),
            self.response("ActiveState=inactive\n"),
        ]
        with (
            patch.object(Path, "exists", return_value=False),
            patch.object(app_scopes, "run", side_effect=replies) as run,
        ):
            self.assertTrue(app_scopes.cleanup_app_scopes(self.state, self.deadline))
        self.assertEqual(run.call_args_list[1].args[0][-2:], ["stop", self.unit])
        self.assertIn(self.scope, run.call_args_list[-1].args[0])

    def test_stop_checks_recorded_and_bound_ptyxis_scopes(self):
        bound = "ptyxis-spawn-abcdef01-2345-6789-abcd-ef0123456789.scope"
        replies = [
            self.response(
                f"Description=Hearthspace app scopes for {self.state['runtime_dir']}\nBoundBy={bound}\n"
            ),
            self.response(""),
            self.response("ActiveState=inactive\n"),
            self.response("ActiveState=deactivating\n"),
            self.response("LoadState=not-found\n"),
            self.response("LoadState=not-found\n"),
            self.response("LoadState=not-found\n"),
        ]
        with (
            patch.object(Path, "exists", return_value=False),
            patch.object(app_scopes, "scope_records", return_value={self.ptyxis_scope}),
            patch.object(app_scopes, "run", side_effect=replies) as run,
            patch.object(app_scopes.time, "sleep"),
        ):
            self.assertTrue(app_scopes.cleanup_app_scopes(self.state, self.deadline))
        commands = [call.args[0] for call in run.call_args_list]
        self.assertEqual(commands[1][-2:], ["stop", self.unit])
        self.assertIn(self.ptyxis_scope, commands[-2])
        self.assertIn(bound, commands[-1])
        self.assertTrue(all("import-environment" not in argv for argv in commands))

    def test_foreign_or_malformed_bound_scopes_never_receive_stop(self):
        for name in [
            "app-other.scope",
            "ptyxis-spawn-not-a-uuid.scope",
            self.ptyxis_scope.replace("01234567", "0123456g"),
            self.ptyxis_scope.replace("-89ab-", "-89a-"),
            self.ptyxis_scope + ".scope",
            "../" + self.ptyxis_scope,
            "snap." + "a" * 250 + ".scope",
        ]:
            with (
                self.subTest(name=name),
                patch.object(Path, "exists", return_value=False),
                patch.object(
                    app_scopes,
                    "run",
                    return_value=self.response(
                        f"Description=Hearthspace app scopes for {self.state['runtime_dir']}\nBoundBy={name}\n"
                    ),
                ) as run,
                self.assertRaisesRegex(RuntimeError, "unexpected bound"),
            ):
                app_scopes.cleanup_app_scopes(self.state, self.deadline)
            self.assertEqual(run.call_count, 1)

    def test_scope_records_accept_owned_snap_and_exact_ptyxis_names(self):
        with tempfile.TemporaryDirectory() as temporary:
            runtime = Path(temporary)
            records = runtime / "app-scopes"
            records.mkdir()
            for name in (self.scope, self.ptyxis_scope):
                (records / name).touch()
            self.assertEqual(
                app_scopes.scope_records(runtime), {self.scope, self.ptyxis_scope}
            )

    def test_scope_records_reject_foreign_and_malformed_ptyxis_names(self):
        with tempfile.TemporaryDirectory() as temporary:
            runtime = Path(temporary)
            records = runtime / "app-scopes"
            records.mkdir()
            for name in [
                "app-other.scope",
                "ptyxis-spawn-not-a-uuid.scope",
                self.ptyxis_scope.replace("01234567", "0123456g"),
                self.ptyxis_scope.replace("-89ab-", "-89a-"),
                self.ptyxis_scope + ".scope",
            ]:
                with self.subTest(name=name):
                    path = records / name
                    path.touch()
                    with self.assertRaisesRegex(RuntimeError, "invalid.*records"):
                        app_scopes.scope_records(runtime)
                    path.unlink()

    def test_scope_records_reject_a_valid_ptyxis_name_that_is_a_symlink(self):
        with tempfile.TemporaryDirectory() as temporary:
            runtime = Path(temporary)
            records = runtime / "app-scopes"
            records.mkdir()
            target = runtime / "target"
            target.touch()
            (records / self.ptyxis_scope).symlink_to(target)
            with self.assertRaisesRegex(RuntimeError, "invalid.*records"):
                app_scopes.scope_records(runtime)

    def test_changed_identity_never_receives_stop(self):
        with (
            patch.object(Path, "exists", return_value=False),
            patch.object(
                app_scopes,
                "run",
                return_value=self.response(
                    "Description=someone else\nActiveState=active\n"
                ),
            ) as run,
            self.assertRaisesRegex(RuntimeError, "identity changed"),
        ):
            app_scopes.cleanup_app_scopes(self.state, self.deadline)
        self.assertEqual(run.call_count, 1)

    def test_still_active_bound_scope_fails_cleanup(self):
        replies = [
            self.response(
                f"Description=Hearthspace app scopes for {self.state['runtime_dir']}\nBoundBy={self.scope}\n"
            ),
            self.response(""),
            self.response("ActiveState=inactive\n"),
            self.response("ActiveState=active\n"),
        ]
        with (
            patch.object(Path, "exists", return_value=False),
            patch.object(app_scopes, "run", side_effect=replies),
            patch.object(app_scopes.time, "monotonic", side_effect=[0, 0, 0, 0, 2]),
            self.assertRaisesRegex(TimeoutError, "still active"),
        ):
            app_scopes.cleanup_app_scopes(self.state, 1)

    def test_collected_owner_still_requires_recorded_scope_to_stop(self):
        replies = [
            self.response("LoadState=not-found\n"),
            self.response("ActiveState=deactivating\n"),
            self.response("LoadState=not-found\n"),
        ]
        with (
            patch.object(Path, "exists", return_value=False),
            patch.object(app_scopes, "scope_records", return_value={self.scope}),
            patch.object(app_scopes, "run", side_effect=replies) as run,
            patch.object(app_scopes.time, "sleep"),
        ):
            self.assertTrue(app_scopes.cleanup_app_scopes(self.state, self.deadline))
        self.assertEqual(run.call_count, 3)
        self.assertTrue(all("stop" not in call.args[0] for call in run.call_args_list))

    def test_empty_properties_and_elapsed_deadlines_fail_closed(self):
        self.assertFalse(app_scopes.stopped({}))
        with self.assertRaises(TimeoutError):
            app_scopes.properties(1000, self.unit, time.monotonic() - 1)


if __name__ == "__main__":
    unittest.main()
