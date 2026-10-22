"""Reject false overflow success, clipped endpoints, and leaked app gestures."""

import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import Mock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib import tray_overflow_checks as checks
from agentlib import tray_overflow_fixture as fixtures
from agentlib.control import ControlError


class TrayOverflowTests(unittest.TestCase):
    def test_fixture_count_exceeds_real_output_without_exceeding_watcher_limit(self):
        for width, height in ((1024, 768), (1920, 1080), (1080, 1920), (2304, 1440)):
            with self.subTest(width=width, height=height):
                count = checks.item_count(
                    {"logical_width": width, "logical_height": height}
                )
                self.assertGreater(38 * count - 4, max(width, height))
                self.assertLessEqual(count, 64)
        with self.assertRaisesRegex(ControlError, "64-item"):
            checks.item_count({"logical_width": 3840, "logical_height": 2160})

    def test_bounds_use_output_origin_and_reject_oversized_native_surface(self):
        output = {
            "logical_x": -1200,
            "logical_y": 80,
            "logical_width": 1200,
            "logical_height": 800,
        }
        surface = {"x": -1188, "y": 92, "width": 1176, "height": 54}
        checks.bounded(surface, output)
        for changes in (
            {"width": 1200},
            {"x": -1201},
            {"y": 79},
            {"height": 800},
            {"width": 0},
            {"x": float("nan")},
        ):
            with self.subTest(changes=changes), self.assertRaises(ControlError):
                checks.bounded(surface | changes, output)

    def test_visible_requires_whole_scaled_icon_and_not_just_showing_state(self):
        surface = {"width": 200, "height": 54}
        bounds = {"x": 316, "y": 10, "width": 68, "height": 68}
        self.assertTrue(checks.fully_visible({"bounds": bounds}, surface, 2))
        for changes in (
            {"x": 326},
            {"x": 350},
            {"x": -20},
            {"y": 70},
            {"height": 0},
            {"x": float("nan")},
        ):
            with self.subTest(changes=changes):
                self.assertFalse(
                    checks.fully_visible(
                        {"bounds": bounds | changes, "showing": True}, surface, 2
                    )
                )
        self.assertFalse(checks.fully_visible({}, surface, 2))

    def test_overflow_requires_all_unique_icons_and_an_unreachable_trailing_icon(self):
        icons = [{"name": fixtures.title(index)} for index in range(24)]
        observed = {"icons": icons, "visible": icons[:12]}
        self.assertTrue(checks.require_overflow(observed, 24))
        self.assertFalse(checks.require_overflow(observed | {"visible": icons}, 24))
        self.assertFalse(checks.require_overflow(observed | {"icons": icons[:-1]}, 24))
        self.assertFalse(
            checks.require_overflow(observed | {"icons": icons + icons[:1]}, 24)
        )
        self.assertFalse(
            checks.require_overflow(observed | {"visible": icons[1:12]}, 24)
        )

    def test_input_proof_rejects_activation_scroll_canvas_motion_and_dead_peer(self):
        viewport = {"x": 0, "y": 0, "scale": 1, "pinch_active": False}
        smoke, peer = Mock(), Mock()
        smoke.control.status.return_value = {"viewport": viewport}
        peer.process.poll.return_value = None
        peer.events.return_value = [{"action": "ready"}]
        with patch.object(checks.shell_checks, "closed"):
            checks.unchanged(smoke, peer, viewport, 1)
            for action in ("Activate", "ContextMenu", "Scroll"):
                peer.events.return_value = [{"action": "ready"}, {"action": action}]
                with (
                    self.subTest(action=action),
                    self.assertRaisesRegex(ControlError, "leaked app actions"),
                ):
                    checks.unchanged(smoke, peer, viewport, 1)
            peer.events.return_value = [{"action": "ready"}]
            for changes in ({"x": 1}, {"y": 1}, {"scale": 1.1}, {"pinch_active": True}):
                smoke.control.status.return_value = {"viewport": viewport | changes}
                with (
                    self.subTest(changes=changes),
                    self.assertRaisesRegex(ControlError, "canvas"),
                ):
                    checks.unchanged(smoke, peer, viewport, 1)
            peer.process.poll.return_value = 1
            with self.assertRaisesRegex(ControlError, "peer died"):
                checks.unchanged(smoke, peer, viewport, 1)

    def test_peer_cleanup_on_failure_uses_only_the_explicit_session_bus(self):
        with tempfile.TemporaryDirectory() as directory:
            smoke = Mock(
                artifacts=Path(directory),
                manifest={
                    "runtime_dir": directory,
                    "dbus_session_bus_address": "unix:path=/owned-bus",
                },
            )
            process = Mock(pid=123, returncode=-15)
            process.poll.return_value = None
            with (
                patch.object(
                    fixtures.subprocess, "Popen", return_value=process
                ) as launch,
                self.assertRaisesRegex(ValueError, "failed"),
                fixtures.fixture(smoke, 40),
            ):
                raise ValueError("failed")
            env = launch.call_args.kwargs["env"]
            self.assertEqual(env["DBUS_SESSION_BUS_ADDRESS"], "unix:path=/owned-bus")
            self.assertEqual(
                env["HEARTHSPACE_SESSION_BUS_ADDRESS"], "unix:path=/owned-bus"
            )
            self.assertNotIn("AT_SPI_BUS_ADDRESS", env)
            process.terminate.assert_called_once()
            process.wait.assert_called_once_with(timeout=2)


if __name__ == "__main__":
    unittest.main()
