import contextlib
import io
import json
import signal
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib.control import ControlError
from agentlib.recording import main, parser, run


class RecordingCliTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.path = Path(temporary.name)
        self.arguments = [
            "--session",
            str(self.path),
            "--output",
            str(self.path / "demo.mp4"),
        ]
        self.args = parser().parse_args(self.arguments)
        self.manifest = {
            "status": "ready",
            "command_socket": str(self.path / "missing.sock"),
        }
        self.save_manifest()
        self.status = {
            "backend": "native",
            "active": True,
            "locked": False,
            "outputs": [{"name": "eDP-1", "width": 2560, "height": 1600}],
        }
        dependency = mock.patch(
            "agentlib.recording.shutil.which", return_value="/usr/bin/tool"
        )
        dependency.start()
        self.addCleanup(dependency.stop)

    def save_manifest(self):
        (self.path / "session.json").write_text(json.dumps(self.manifest))

    def test_duration_and_width_rejections_do_not_connect_or_create_artifacts(self):
        for value in (0, 0.9, 181, float("inf"), float("nan")):
            with (
                self.subTest(duration=value),
                mock.patch("agentlib.recording.Control") as control,
            ):
                self.args.duration = value
                with self.assertRaisesRegex(ControlError, "duration"):
                    run(self.args)
                control.assert_not_called()
        self.args.duration = 1
        for value in (0, 319, 321, 3842):
            with (
                self.subTest(width=value),
                mock.patch("agentlib.recording.Control") as control,
            ):
                self.args.width = value
                with self.assertRaisesRegex(ControlError, "encoded width"):
                    run(self.args)
                control.assert_not_called()
        self.assertFalse((self.path / "demo.recording").exists())

    def test_output_extension_and_existing_stop_file_are_rejected(self):
        self.args.output = self.path / "demo.webm"
        with self.assertRaisesRegex(ControlError, "new .mp4"):
            run(self.args)
        self.args.output = self.path / "demo.mp4"
        self.args.stop_file = self.path / "already-stopped"
        self.args.stop_file.touch()
        with self.assertRaisesRegex(ControlError, "stop file already exists"):
            run(self.args)

    def test_inactive_and_locked_native_sessions_are_rejected(self):
        for change in ({"active": False}, {"locked": True}):
            with (
                self.subTest(change=change),
                mock.patch("agentlib.recording.Control") as control,
            ):
                control.return_value.status.return_value = {**self.status, **change}
                with self.assertRaisesRegex(ControlError, "active, unlocked"):
                    run(self.args)
                control.return_value.capture.assert_not_called()
        self.assertFalse((self.path / "demo.recording").exists())

    def test_stopped_starting_or_cleaned_manifest_never_connects(self):
        for change in (
            {"status": "stopped"},
            {"status": "starting"},
            {"cleanup": {"complete": True}},
        ):
            self.manifest = {"status": "ready", "command_socket": "/unused", **change}
            self.save_manifest()
            with (
                self.subTest(change=change),
                mock.patch("agentlib.recording.Control") as control,
            ):
                with self.assertRaisesRegex(ControlError, "ready, running"):
                    run(self.args)
                control.assert_not_called()

    def test_stale_ready_manifest_reports_unavailable_socket_without_artifacts(self):
        with contextlib.redirect_stderr(io.StringIO()) as stderr:
            self.assertEqual(main(self.arguments), 1)
        self.assertIn("control socket", stderr.getvalue())
        self.assertFalse((self.path / "demo.recording").exists())

    def test_missing_and_malformed_manifests_report_cli_failure(self):
        manifest = self.path / "session.json"
        manifest.unlink()
        with contextlib.redirect_stderr(io.StringIO()) as stderr:
            self.assertEqual(main(self.arguments), 1)
        self.assertIn("session.json", stderr.getvalue())
        manifest.write_text("{not json}")
        with contextlib.redirect_stderr(io.StringIO()) as stderr:
            self.assertEqual(main(self.arguments), 1)
        self.assertIn("recording failed", stderr.getvalue())

    def test_unknown_monitor_and_upscaling_are_rejected(self):
        with mock.patch("agentlib.recording.Control") as control:
            control.return_value.status.return_value = self.status
            self.args.monitor = "HDMI-absent"
            with self.assertRaisesRegex(ControlError, "monitor is missing"):
                run(self.args)
            self.args.monitor = None
            self.args.width = 3200
            with self.assertRaisesRegex(ControlError, "exceeds native width"):
                run(self.args)
        self.assertFalse((self.path / "demo.recording").exists())

    def test_capture_failure_restores_signals_and_retains_failure_provenance(self):
        before = {
            number: signal.getsignal(number)
            for number in (signal.SIGINT, signal.SIGTERM)
        }
        with (
            mock.patch("agentlib.recording.Control") as control,
            mock.patch(
                "agentlib.recording.collect",
                side_effect=ControlError("session is locked"),
            ),
            mock.patch("agentlib.recording.encode") as encode,
        ):
            control.return_value.status.return_value = self.status
            with self.assertRaisesRegex(ControlError, "session is locked"):
                run(self.args)
            encode.assert_not_called()
        for number, handler in before.items():
            self.assertEqual(signal.getsignal(number), handler)
        result = json.loads(
            (self.path / "demo.recording" / "recording.json").read_text()
        )
        self.assertFalse(result["complete"])
        self.assertEqual(result["error"], "session is locked")
        self.assertFalse(self.args.output.exists())

    def test_bad_binary_provenance_cannot_change_signal_handlers(self):
        (self.path / "binaries.json").write_text("{invalid}")
        with (
            mock.patch("agentlib.recording.Control") as control,
            mock.patch("agentlib.recording.signal.signal") as install_handler,
        ):
            control.return_value.status.return_value = self.status
            with self.assertRaises(ValueError):
                run(self.args)
            install_handler.assert_not_called()


if __name__ == "__main__":
    unittest.main()
