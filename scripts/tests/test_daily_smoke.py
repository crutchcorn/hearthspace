import contextlib
import io
import json
import socket
import sys
import tempfile
import threading
import time
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import Mock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib import daily_checks
from agentlib.control import ControlError, Deadline
from agentlib.daily import DailySmoke


def arguments(directory, **extra):
    return SimpleNamespace(
        session=Path(directory),
        timeout=45,
        total_timeout=240,
        captures=3,
        notifications=False,
        log_level="info",
        **extra,
    )


class DailySmokeTests(unittest.TestCase):
    def test_stale_manifest_or_prior_evidence_is_never_reused_or_stopped(self):
        for stale in ["session.json", "artifacts/results.json"]:
            with self.subTest(stale=stale), tempfile.TemporaryDirectory() as directory:
                path = Path(directory) / stale
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("{}")
                with patch.object(DailySmoke, "launcher") as launcher:
                    with self.assertRaisesRegex(ControlError, "fresh session"):
                        DailySmoke(arguments(directory))
                    launcher.assert_not_called()
                self.assertEqual(path.read_text(), "{}")

    def test_failed_workflow_always_stops_and_checks_owned_session_recovery(self):
        for cleanup_fails in [False, True]:
            with (
                self.subTest(cleanup_fails=cleanup_fails),
                tempfile.TemporaryDirectory() as directory,
            ):
                smoke = DailySmoke(arguments(directory))
                operations = []

                def launcher(
                    operation,
                    *_,
                    operations=operations,
                    smoke=smoke,
                    cleanup_fails=cleanup_fails,
                ):
                    operations.append(operation)
                    if operation == "start":
                        (smoke.directory / "session.json").write_text("{}")
                    if operation == "stop" and cleanup_fails:
                        raise ControlError("cleanup failed")

                with (
                    contextlib.redirect_stdout(io.StringIO()),
                    patch("agentlib.daily.shutil.which", return_value="/test/binary"),
                    patch.object(smoke, "launcher", side_effect=launcher),
                    patch.object(
                        smoke, "workflow", side_effect=ControlError("selector failed")
                    ),
                    patch.object(smoke, "verify_restored") as restored,
                    self.assertRaisesRegex(ControlError, "selector failed"),
                ):
                    smoke.run()
                self.assertEqual(operations, ["start", "stop"])
                self.assertEqual(restored.call_count, 0 if cleanup_fails else 1)
                entries = json.loads((smoke.artifacts / "results.json").read_text())
                self.assertTrue(
                    any(entry["step"] == "operation-failed" for entry in entries)
                )
                if cleanup_fails:
                    self.assertEqual(entries[-1]["step"], "recovery-failed")

    def test_step_budget_cannot_extend_the_aggregate_deadline(self):
        with (
            tempfile.TemporaryDirectory() as directory,
            contextlib.redirect_stdout(io.StringIO()),
        ):
            smoke = DailySmoke(arguments(directory))
            smoke.manifest = {"command_socket": "/unused"}
            with patch("time.monotonic", return_value=100) as clock:
                smoke.total.end = 105

                def action(instance):
                    self.assertEqual(instance.control.deadline.end, 105)
                    clock.return_value = 106

                with self.assertRaisesRegex(ControlError, "deadline"):
                    smoke.step("bounded", action)
                self.assertFalse(
                    any(entry["step"] == "step-passed" for entry in smoke.results)
                )

    def test_cleanup_has_reserved_time_after_total_budget_is_exhausted(self):
        with (
            tempfile.TemporaryDirectory() as directory,
            contextlib.redirect_stdout(io.StringIO()),
        ):
            smoke = DailySmoke(arguments(directory))
            smoke.total.end = time.monotonic() - 1
            result = SimpleNamespace(returncode=0, stdout="{}", stderr="")
            with patch("agentlib.daily.subprocess.run", return_value=result) as run:
                smoke.launcher("stop")
                self.assertEqual(run.call_args.kwargs["timeout"], 45)
                self.assertIn("stop", run.call_args.args[0])
                with self.assertRaisesRegex(ControlError, "deadline"):
                    smoke.launcher("start")
                self.assertEqual(run.call_count, 1)

    def test_repeated_captures_require_same_output_and_strictly_new_frames(self):
        for frames, output, succeeds in [
            ([1, 2, 3], "eDP-1", True),
            ([1, 1, 2], "eDP-1", False),
            ([1, 2, 3], "HDMI-1", False),
        ]:
            with self.subTest(frames=frames, output=output):
                smoke = Mock()
                smoke.args.captures = 3
                smoke.control.status.return_value = {"outputs": [{"name": "eDP-1"}]}
                smoke.capture.side_effect = [
                    ({"output": output, "frame_id": frame}, b"png") for frame in frames
                ]
                if succeeds:
                    daily_checks.captures(smoke)
                else:
                    with self.assertRaises(ControlError):
                        daily_checks.captures(smoke)

    def test_service_response_cannot_stall_after_a_partial_reply(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "service.sock"
            listener = socket.socket(socket.AF_UNIX)
            listener.bind(str(path))
            listener.listen()

            def server():
                with listener, listener.accept()[0] as peer:
                    peer.recv(1024)
                    peer.sendall(b"ok partial")
                    time.sleep(0.1)

            thread = threading.Thread(target=server)
            thread.start()
            try:
                with self.assertRaises((TimeoutError, ControlError)):
                    daily_checks.read_line(path, b"get\n", Deadline(0.03))
            finally:
                thread.join(timeout=1)
            self.assertFalse(thread.is_alive())

    def test_selector_detects_an_accidental_duplicate_launch(self):
        smoke = Mock(target=1, marker="UNIQUE")
        smoke.control.status.side_effect = [
            {"windows": [{"id": 1}]},
            {"windows": [{"id": 1}, {"id": 2}]},
            {"windows": [{"id": 1}, {"id": 2}, {"id": 3}]},
        ]
        smoke.until.return_value = {"id": 1, "focused": True}
        with (
            patch("agentlib.daily_checks.Input"),
            self.assertRaisesRegex(ControlError, "spawned or removed"),
        ):
            daily_checks.selector(smoke)

    def test_failed_enter_keeps_original_error_after_requesting_post_action_evidence(
        self,
    ):
        smoke = Mock(target=1, marker="UNIQUE")
        smoke.control.status.side_effect = [
            {"windows": [{"id": 1}]},
            {"windows": [{"id": 1}, {"id": 2}]},
        ]
        smoke.until.side_effect = [True, True, ControlError("selector deadline")]
        with (
            patch("agentlib.daily_checks.Input"),
            self.assertRaisesRegex(ControlError, "selector deadline"),
        ):
            daily_checks.selector(smoke)
        smoke.failure_evidence.assert_called_once_with("running-window-submit-failed")

    def test_failure_evidence_shares_total_budget_and_restores_expired_action_control(
        self,
    ):
        for capture_fails in [False, True]:
            with (
                self.subTest(capture_fails=capture_fails),
                tempfile.TemporaryDirectory() as directory,
                contextlib.redirect_stdout(io.StringIO()),
            ):
                smoke = DailySmoke(arguments(directory))
                original = Mock(path="/unused")
                smoke.control = original
                diagnostic = Mock()
                diagnostic.status.return_value = {"windows": []}
                with (
                    patch("time.monotonic", return_value=100),
                    patch("agentlib.daily.Control", return_value=diagnostic) as control,
                    patch.object(smoke, "capture") as capture,
                    patch(
                        "agentlib.daily.checks.a11y",
                        return_value={"snapshot": "<desktop/>"},
                    ) as snapshot,
                ):
                    smoke.total.end = 105
                    if capture_fails:
                        capture.side_effect = ControlError("capture unavailable")
                    smoke.failure_evidence("failed")
                    self.assertEqual(control.call_args.args[1].end, 105)
                    self.assertIs(smoke.control, original)
                    if capture_fails:
                        self.assertEqual(
                            smoke.results[-1]["step"], "failure-evidence-unavailable"
                        )
                    else:
                        self.assertEqual(
                            (smoke.artifacts / "failed.xml").read_text(), "<desktop/>"
                        )
                    smoke.total.end = 99
                    control.reset_mock()
                    snapshot.reset_mock()
                    smoke.failure_evidence("no-time")
                    control.assert_not_called()
                    snapshot.assert_not_called()
                    self.assertIs(smoke.control, original)


if __name__ == "__main__":
    unittest.main()
