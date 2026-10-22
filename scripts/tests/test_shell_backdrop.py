"""Popup contrast fixture stays on the private display and closes only its window."""

import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import Mock, call, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib import shell_backdrop
from agentlib.control import ControlError
from agentlib.shell_presentation import Frame, dark_background
from test_agent_visual import capture


class BackdropTests(unittest.TestCase):
    def test_environment_uses_only_explicit_private_display_and_bus(self):
        manifest = {
            "runtime_dir": "/owned",
            "wayland_display": "private-wayland",
            "dbus_session_bus_address": "unix:path=/owned/bus",
        }
        with patch.dict(
            shell_backdrop.os.environ,
            {
                "DISPLAY": ":0",
                "WAYLAND_DISPLAY": "host-wayland",
                "AT_SPI_BUS_ADDRESS": "host-a11y",
                "DBUS_SESSION_BUS_PID": "42",
            },
        ):
            env = shell_backdrop.environment(manifest)
        self.assertEqual(env["WAYLAND_DISPLAY"], "private-wayland")
        self.assertEqual(env["XDG_RUNTIME_DIR"], "/owned")
        self.assertEqual(env["DBUS_SESSION_BUS_ADDRESS"], "unix:path=/owned/bus")
        self.assertNotIn("DISPLAY", env)
        self.assertNotIn("AT_SPI_BUS_ADDRESS", env)
        self.assertNotIn("DBUS_SESSION_BUS_PID", env)

    def test_pixel_readiness_samples_workarea_between_top_and_bottom_corners(self):
        metadata = {
            "width": 320,
            "height": 480,
            "scale": 2,
            "logical_x": -20,
            "logical_y": 10,
        }
        output = {
            "logical_x": -20,
            "logical_y": 10,
            "logical_width": 160,
            "logical_height": 240,
            "scale": 2,
        }
        dark = capture(metadata, [((-8, 88, 128, 172), shell_backdrop.COLOR)])
        self.assertTrue(dark_background(Frame(dark), output))
        self.assertFalse(dark_background(Frame(capture(metadata, [])), output))
        partial = capture(metadata, [((-8, 88, 50, 172), shell_backdrop.COLOR)])
        self.assertFalse(dark_background(Frame(partial), output))

    def test_backdrop_maximizes_then_closes_only_the_new_owned_id(self):
        with tempfile.TemporaryDirectory() as directory:
            smoke = Mock(artifacts=Path(directory))
            smoke.until.side_effect = lambda _name, condition: condition()
            existing = {"id": 7, "app_id": shell_backdrop.APP_ID, "mapped": True}
            owned = existing | {"id": 12}
            smoke.control.status.side_effect = [
                {"windows": [existing]},
                {"windows": [existing, owned]},
                {"windows": [existing]},
            ]
            smoke.window.side_effect = [
                {"titlebar_height": 0},
                {"focused": False},
                None,
            ]
            process = Mock()
            process.poll.return_value = None
            with (
                patch.object(shell_backdrop, "environment", return_value={}),
                patch.object(
                    shell_backdrop.subprocess, "Popen", return_value=process
                ) as launch,
                patch.object(shell_backdrop, "stop_client") as stop,
                patch.object(shell_backdrop, "maximized_at_output", return_value=True),
                patch.object(shell_backdrop, "Input") as inputs,
                shell_backdrop.backdrop(smoke, {"logical_x": -800, "logical_y": 50}),
            ):
                pass
            self.assertEqual(
                [item.args[0] for item in smoke.control.request.call_args_list],
                ["maximize-window 12 true", "close-window 12"],
            )
            self.assertIn(
                "--override=colors.background=101820", launch.call_args.args[0]
            )
            self.assertIn("--override=colors.alpha=1.0", launch.call_args.args[0])
            inputs.return_value.__enter__.return_value.click.assert_called_once_with(
                -796, 54
            )
            stop.assert_called_once_with(process)

    def test_reference_waits_for_explicitly_inactive_owned_window(self):
        smoke = Mock()
        inactive = {"id": 12, "focused": False}
        smoke.window.side_effect = [
            {"id": 12, "focused": True},
            None,
            {"id": 12},  # Missing focus evidence cannot prove readiness.
            inactive,
        ]

        def until(_name, condition):
            for expected in [None, None, None, inactive]:
                self.assertEqual(condition(), expected)
            return inactive

        smoke.until.side_effect = until
        with patch.object(shell_backdrop, "Input") as inputs:
            self.assertEqual(
                shell_backdrop.unfocus(
                    smoke, 12, {"logical_x": 100, "logical_y": -600}
                ),
                inactive,
            )
        inputs.return_value.__enter__.return_value.click.assert_called_once_with(
            104, -596
        )
        self.assertEqual(smoke.window.call_args_list, [call(12)] * 4)

    def test_early_backdrop_exit_fails_before_waiting_for_mapping(self):
        with tempfile.TemporaryDirectory() as directory:
            smoke = Mock(artifacts=Path(directory))
            smoke.control.status.return_value = {"windows": []}
            smoke.until.side_effect = lambda _name, condition: condition()
            process = Mock()
            process.poll.return_value = 1
            with (
                patch.object(shell_backdrop, "environment", return_value={}),
                patch.object(shell_backdrop.subprocess, "Popen", return_value=process),
                patch.object(shell_backdrop, "stop_client") as stop,
                self.assertRaisesRegex(ControlError, "exited before mapping"),
                shell_backdrop.backdrop(smoke, {}),
            ):
                self.fail("failed backdrop must not enter popup workflow")
            stop.assert_called_once_with(process)
            smoke.control.request.assert_not_called()


if __name__ == "__main__":
    unittest.main()
