import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import Mock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib.accessibility import run_accessibility
from agentlib.cli import parser
from agentlib.control import ControlError, Deadline


class AccessibilityTests(unittest.TestCase):
    def test_locked_session_never_accesses_private_app_semantics(self):
        with tempfile.TemporaryDirectory() as directory:
            session = Path(directory)
            (session / "session.json").write_text(
                json.dumps(
                    {
                        "dbus_session_bus_address": "unix:path=/private/bus",
                        "binary_dir": directory,
                    }
                )
            )
            for operation in ("snapshot", "find", "activate"):
                arguments = ["--session", directory, "a11y", operation]
                if operation != "snapshot":
                    arguments.append("//*[@name='Secret']")
                args = parser().parse_args(arguments)
                control = Mock()
                control.status.return_value = {"locked": True}
                with (
                    self.subTest(operation=operation),
                    patch("agentlib.accessibility.subprocess.run") as run,
                    self.assertRaisesRegex(ControlError, "session is locked"),
                ):
                    run_accessibility(args, control, Deadline(5))
                run.assert_not_called()

    def test_explicit_session_bus_is_required_without_host_fallback(self):
        args = parser().parse_args(["--socket", "/tmp/socket", "a11y", "snapshot"])
        with self.assertRaisesRegex(ControlError, "requires --session"):
            run_accessibility(args, Mock(), Deadline(5))
        with tempfile.TemporaryDirectory() as directory:
            session = Path(directory)
            (session / "session.json").write_text("{}")
            args = parser().parse_args(["--session", directory, "a11y", "snapshot"])
            with self.assertRaisesRegex(ControlError, "no private D-Bus"):
                run_accessibility(args, Mock(), Deadline(5))

    def test_exact_selector_private_bus_and_action_confirmation(self):
        with tempfile.TemporaryDirectory() as directory:
            session = Path(directory)
            (session / "hearthspace-a11y").touch()
            (session / "session.json").write_text(
                json.dumps(
                    {
                        "dbus_session_bus_address": "unix:path=/private/bus",
                        "binary_dir": directory,
                    }
                )
            )
            args = parser().parse_args(
                [
                    "--session",
                    directory,
                    "a11y",
                    "activate",
                    "//PushButton[@name='Save']",
                ]
            )
            control = Mock()
            control.capture.return_value = ({"frame_id": 12}, b"png")
            success = subprocess.CompletedProcess(
                [], 0, '{"ok":true,"accepted":true}', ""
            )
            with patch(
                "agentlib.accessibility.subprocess.run", return_value=success
            ) as call:
                result = run_accessibility(args, control, Deadline(5))
            command = call.call_args.args[0]
            self.assertEqual(command[1:3], ["--bus", "unix:path=/private/bus"])
            self.assertEqual(command[-2:], ["activate", "//PushButton[@name='Save']"])
            self.assertEqual(result["capture"]["frame_id"], 12)
            control.status.assert_called_once()
            control.capture.reset_mock()
            rejected = subprocess.CompletedProcess(
                [], 1, "", '{"ok":false,"error":"matched 2"}'
            )
            with (
                patch("agentlib.accessibility.subprocess.run", return_value=rejected),
                self.assertRaisesRegex(ControlError, "matched 2"),
            ):
                run_accessibility(args, control, Deadline(5))
            control.capture.assert_not_called()


if __name__ == "__main__":
    unittest.main()
