"""Evidence helpers reject missing icons and incomplete app action delivery."""

import json
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import Mock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib import tray_checks, tray_fixture, tray_library
from agentlib.control import ControlError


class TrayEvidenceTests(unittest.TestCase):
    def test_standard_icon_proof_rejects_empty_fallback_center(self):
        frame = Mock(width=100)
        frame.metadata = {"logical_x": 0, "logical_y": 0, "scale": 1}
        frame.rect.return_value = (0, 0, 40, 40)
        frame.rgb.side_effect = lambda x, y: (
            (30, 30, 30) if x < 15 or x > 25 else (255, 255, 255)
        )
        self.assertEqual(
            tray_library.central_ink(
                frame, {}, {"x": 700, "y": 12, "width": 100, "height": 48}
            ),
            0,
        )
        frame.rgb.side_effect = lambda x, y: (
            (30, 30, 30) if 18 <= x <= 21 and 12 <= y <= 28 else (255, 255, 255)
        )
        self.assertGreater(
            tray_library.central_ink(
                frame, {}, {"x": 700, "y": 12, "width": 100, "height": 48}
            ),
            6,
        )

    def test_blue_focus_ring_cannot_satisfy_app_icon_update(self):
        frame = Mock(width=100)
        frame.metadata = {"logical_x": 0, "logical_y": 0, "scale": 1}
        frame.rect.return_value = (0, 0, 40, 40)
        frame.rgb.side_effect = lambda x, y: (
            (32, 96, 224) if x < 4 or y < 4 or x > 35 or y > 35 else (224, 48, 32)
        )
        self.assertEqual(tray_checks.color_pixels(frame, {}, "blue", {}), 0)
        self.assertEqual(tray_checks.color_pixels(frame, {}, "red", {}), 400)

    def test_pixel_proof_counts_only_the_observed_app_icon_rectangle(self):
        frame = Mock(width=100)
        frame.metadata = {"logical_x": 0, "logical_y": 0, "scale": 1}
        frame.rect.return_value = (10, 10, 12, 12)
        frame.rgb.side_effect = lambda x, y: (
            (224, 48, 32) if (x, y) == (10, 10) else (32, 96, 224)
        )
        bounds = {"x": 10, "y": 10, "width": 2, "height": 2}
        self.assertEqual(tray_checks.color_pixels(frame, bounds, "red", {}), 1)
        self.assertEqual(tray_checks.color_pixels(frame, bounds, "blue", {}), 3)
        self.assertEqual(frame.rgb.call_count, 8)

    def test_incomplete_action_log_is_not_accepted_as_delivery(self):
        with tempfile.TemporaryDirectory() as temporary:
            smoke = Mock(artifacts=Path(temporary), manifest={"runtime_dir": temporary})
            peer = tray_fixture.Fixture(smoke, 1)
            peer.actions.write_text('{"action":"activate"}\n{"action":"scroll"')
            self.assertEqual(peer.events(), [{"action": "activate"}])
            with peer.actions.open("a") as stream:
                stream.write(',"delta":15}\n')
            self.assertEqual(peer.events()[1], {"action": "scroll", "delta": 15})

    def test_control_errors_are_not_silently_accepted(self):
        smoke = Mock(
            artifacts=Path("/private/evidence"),
            manifest={"runtime_dir": "/private/runtime"},
        )
        peer = tray_fixture.Fixture(smoke, 1)
        with (
            patch.object(
                tray_fixture,
                "read_line",
                return_value=json.dumps({"ok": False, "error": "unsupported"}),
            ) as read,
            self.assertRaisesRegex(ControlError, "fixture command failed"),
        ):
            peer.command("passive")
        self.assertEqual(
            read.call_args.args[:2],
            (Path("/private/runtime/tray-fixture.sock"), b"passive\n"),
        )

    def test_action_requires_matching_fields_and_a_live_peer(self):
        with tempfile.TemporaryDirectory() as temporary:
            smoke = Mock(artifacts=Path(temporary), manifest={"runtime_dir": temporary})
            smoke.until.side_effect = lambda _label, check: check()
            peer = tray_fixture.Fixture(smoke, 1)
            peer.process = Mock()
            peer.process.poll.return_value = None
            event = {"action": "menu", "id": 6, "event": "clicked"}
            peer.actions.write_text(json.dumps(event) + "\n")
            self.assertEqual(peer.expect("menu", id=6, event="clicked"), event)
            self.assertIsNone(peer.expect("menu", after=1, id=6, event="clicked"))
            self.assertIsNone(peer.expect("menu", id=3, event="clicked"))
            peer.process.poll.return_value = 1
            with self.assertRaisesRegex(ControlError, "exited before receiving"):
                peer.expect("menu", id=6)

    def test_fixture_uses_manifest_bus_and_cleans_up_on_body_failure(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            binary = directory / "examples/tray_fixture"
            binary.parent.mkdir()
            binary.touch()
            smoke = Mock(
                artifacts=directory,
                manifest={
                    "runtime_dir": temporary,
                    "binary_dir": temporary,
                    "dbus_session_bus_address": "unix:path=/owned-session-bus",
                },
            )
            process = Mock(pid=123, returncode=-15)
            process.poll.return_value = None
            with (
                patch.object(
                    tray_fixture.subprocess, "Popen", return_value=process
                ) as launch,
                self.assertRaisesRegex(ValueError, "body failure"),
                tray_fixture.fixture(smoke),
            ):
                raise ValueError("body failure")
            self.assertEqual(
                launch.call_args.kwargs["env"]["DBUS_SESSION_BUS_ADDRESS"],
                "unix:path=/owned-session-bus",
            )
            self.assertNotIn("AT_SPI_BUS_ADDRESS", launch.call_args.kwargs["env"])
            process.terminate.assert_called_once()
            process.wait.assert_called_once_with(timeout=2)


if __name__ == "__main__":
    unittest.main()
