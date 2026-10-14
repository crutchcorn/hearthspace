import contextlib
import io
import json
import sys
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib.control import ControlError
from agentlib.smoke import Smoke
from agentlib.startup_failure import verify_startup_failure


class SmokeTests(unittest.TestCase):
    def startup_evidence(self, session):
        manifest = {
            "id": "startup-test",
            "unit": "hearthspace-native-startup-test.service",
            "session_dir": str(session),
            "compositor": str(Path("/bin/false").resolve()),
            "vt": 7,
            "original_vt": 2,
            "uid": 1000,
        }
        identity = {"vt": 7, "uid": 1000, "leader": 1234, "session_id": "test"}
        (session / "session.json").write_text(json.dumps(manifest))
        (session / "identity.json").write_text(json.dumps(identity))
        (session / "session.log").write_text(
            'Error: "compositor exited before readiness: exit status: 1"\n'
        )
        return manifest, identity

    def test_startup_failure_rejects_preflight_errors_without_a_manifest(self):
        with tempfile.TemporaryDirectory() as directory:
            smoke = Smoke(
                SimpleNamespace(
                    session=Path(directory), timeout=10, case="startup-failure"
                )
            )
            with (
                patch("agentlib.smoke.subprocess.run") as run,
                patch.object(smoke, "launcher") as launcher,
                patch.object(smoke, "verify_restored") as restored,
                contextlib.redirect_stdout(io.StringIO()),
            ):
                run.return_value.stdout = "test host"
                launcher.return_value.returncode = 1
                with self.assertRaisesRegex(ControlError, "allocated native session"):
                    smoke.run()
            self.assertEqual(launcher.call_count, 1)
            restored.assert_not_called()

    def test_startup_failure_rejects_stale_evidence_without_stopping_it(self):
        with tempfile.TemporaryDirectory() as directory:
            session = Path(directory)
            self.startup_evidence(session)
            smoke = Smoke(
                SimpleNamespace(session=session, timeout=10, case="startup-failure")
            )
            with (
                patch("agentlib.smoke.subprocess.run") as run,
                patch.object(smoke, "launcher") as launcher,
                contextlib.redirect_stdout(io.StringIO()),
            ):
                run.return_value.stdout = "test host"
                with self.assertRaisesRegex(ControlError, "new session directory"):
                    smoke.run()
            launcher.assert_not_called()

    def test_startup_failure_requires_matching_injection_and_pam_identity(self):
        changes = (
            ("session.json", "compositor", "/bin/true"),
            ("session.json", "vt", 2),
            ("identity.json", "vt", 8),
            ("identity.json", "uid", 1001),
            ("identity.json", "leader", 0),
        )
        with tempfile.TemporaryDirectory() as directory:
            session = Path(directory)
            for filename, key, value in changes:
                with self.subTest(filename=filename, key=key):
                    self.startup_evidence(session)
                    path = session / filename
                    state = json.loads(path.read_text())
                    state[key] = value
                    path.write_text(json.dumps(state))
                    with self.assertRaisesRegex(ControlError, "matching injected"):
                        verify_startup_failure(session, "/bin/false")

    def test_startup_failure_rejects_errors_before_compositor_execution(self):
        with tempfile.TemporaryDirectory() as directory:
            session = Path(directory)
            self.startup_evidence(session)
            (session / "session.log").write_text(
                'Error: "settingsd exited before readiness: exit status: 1"\n'
            )
            with self.assertRaisesRegex(ControlError, "before the injected"):
                verify_startup_failure(session, "/bin/false")

    def test_startup_failure_verifies_injection_then_checks_restoration(self):
        with tempfile.TemporaryDirectory() as directory:
            session = Path(directory)
            smoke = Smoke(
                SimpleNamespace(session=session, timeout=10, case="startup-failure")
            )

            def launcher(operation, *_args, **_kwargs):
                if operation == "start":
                    self.startup_evidence(session)
                return SimpleNamespace(returncode=1)

            with (
                patch("agentlib.smoke.subprocess.run") as run,
                patch.object(smoke, "launcher", side_effect=launcher) as launch,
                patch.object(smoke, "verify_restored") as restored,
                contextlib.redirect_stdout(io.StringIO()),
            ):
                run.return_value.stdout = "test host"
                smoke.run()
            self.assertEqual(
                [call.args[0] for call in launch.call_args_list], ["start", "stop"]
            )
            restored.assert_called_once()
            self.assertEqual(smoke.results[-1]["step"], "injected-startup-failure")
            self.assertEqual(smoke.results[-1]["session_id"], "test")

    def test_cursor_region_uses_exact_scale_with_rounded_bounds_and_offset(self):
        with tempfile.TemporaryDirectory() as directory:
            smoke = Smoke(SimpleNamespace(session=Path(directory)))
            metadata = {
                "width": 801,
                "height": 601,
                "scale": 2,
                "logical_width": 401,
                "logical_height": 301,
                "logical_x": -401,
                "logical_y": 200,
                "output": "secondary",
                "cursor": "embedded",
            }
            smoke.capture = lambda *_: (metadata, b"png")
            with (
                patch("agentlib.smoke.Input") as inputs,
                patch(
                    "agentlib.smoke.crop", side_effect=[b"cursor", b"background"]
                ) as crop,
                contextlib.redirect_stdout(io.StringIO()),
            ):
                smoke.cursor(metadata)
            self.assertEqual(crop.call_args.args[1], (20, 180, 110, 280))
            self.assertEqual(
                [
                    call.args
                    for call in inputs.return_value.__enter__.return_value.motion.call_args_list
                ],
                [(-381, 300), (-311, 300)],
            )

    def test_fractional_timeout_sends_integer_watchdog_and_tracing_filter(self):
        with tempfile.TemporaryDirectory() as directory:
            session = Path(directory)
            (session / "session.json").write_text("{}")
            args = SimpleNamespace(
                session=session, timeout=90.25, case="success", log_level="debug"
            )
            smoke = Smoke(args)
            smoke.success = lambda: None
            smoke.verify_restored = lambda: None
            with (
                patch("agentlib.smoke.shutil.which", return_value="/usr/bin/foot"),
                patch("agentlib.smoke.subprocess.run") as run,
                contextlib.redirect_stdout(io.StringIO()),
            ):
                run.return_value = SimpleNamespace(returncode=0, stdout="{}", stderr="")
                smoke.run()
            start = next(
                call.args[0] for call in run.call_args_list if "start" in call.args[0]
            )
            self.assertEqual(start[start.index("--watchdog") + 1], "181")
            self.assertEqual(start[start.index("--log-level") + 1], "debug")

    def test_recovery_failure_does_not_hide_original_interaction_error(self):
        with tempfile.TemporaryDirectory() as directory:
            session = Path(directory)
            (session / "session.json").write_text("{}")
            smoke = Smoke(SimpleNamespace(session=session, timeout=10, case="success"))

            def launcher(operation, *_):
                if operation == "stop":
                    raise ControlError("cleanup failed")

            smoke.launcher = launcher
            smoke.success = lambda: (_ for _ in ()).throw(ControlError("typing failed"))
            with (
                patch("agentlib.smoke.shutil.which", return_value="/usr/bin/foot"),
                contextlib.redirect_stdout(io.StringIO()),
                self.assertRaisesRegex(ControlError, "typing failed"),
            ):
                smoke.run()
            failures = [
                result for result in smoke.results if result["step"].endswith("failed")
            ]
            self.assertEqual(
                [result["error"] for result in failures],
                ["typing failed", "cleanup failed"],
            )

    def test_unmapped_toplevel_is_not_ready_for_input(self):
        with tempfile.TemporaryDirectory() as directory:
            smoke = Smoke(SimpleNamespace(session=Path(directory)))
            smoke.window = lambda: {"height": 30, "titlebar_height": 30}
            self.assertIsNone(smoke.mapped_terminal())
            smoke.window = lambda: {"height": 524, "titlebar_height": 30}
            self.assertIsNotNone(smoke.mapped_terminal())


if __name__ == "__main__":
    unittest.main()
