import sys
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
