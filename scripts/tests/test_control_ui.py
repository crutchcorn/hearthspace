"""Isolation and protocol evidence for the control center acceptance peer."""

import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import Mock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib import control_ui, control_ui_fixture
from agentlib.control import ControlError, Deadline


def environment(directory):
    address = f"unix:path={directory / 'bus'}"
    return {
        **os.environ,
        "PATH": "/usr/bin:/bin",
        "DBUS_SYSTEM_BUS_ADDRESS": address,
        "DBUS_SESSION_BUS_ADDRESS": address,
        "PULSE_SERVER": "unix:/run/user/1000/pulse/native",
    }


class ControlFixtureTests(unittest.TestCase):
    def test_fake_pactl_records_pinned_mutations_without_host_audio(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            env = environment(directory)
            with control_ui_fixture.services(
                directory, directory, env, Deadline(3)
            ) as peer:

                def call(*arguments):
                    return subprocess.run(
                        ["pactl", *arguments],
                        env=env,
                        capture_output=True,
                        text=True,
                        timeout=2,
                        check=False,
                    )

                info = call("--format=json", "info")
                self.assertEqual(info.returncode, 0)
                self.assertEqual(
                    json.loads(info.stdout)["default_sink_name"], "fixture.output"
                )
                sinks = json.loads(call("--format=json", "list", "sinks").stdout)
                self.assertEqual(sinks[0]["description"], "Fixture speakers")
                self.assertEqual(sinks[0]["channel_map"], "front-left,front-right")
                self.assertEqual(
                    call("--", "set-sink-volume", "fixture.output", "73%").returncode, 0
                )
                self.assertEqual(
                    call("--", "set-sink-mute", "fixture.output", "toggle").returncode,
                    0,
                )
                self.assertEqual(peer.status()["volume"], 73)
                self.assertTrue(peer.status()["muted"])
                self.assertEqual(
                    call("--", "set-sink-volume", "host.output", "10%").returncode, 1
                )
                self.assertEqual(peer.status()["volume"], 73)
                self.assertEqual(len(peer.mutations()), 3)
                self.assertTrue(
                    all(
                        event["server"] == f"unix:{directory / 'audio-fixture.sock'}"
                        for event in peer.mutations()
                    )
                )
                peer.update(present=False)
                self.assertEqual(
                    json.loads(call("--format=json", "list", "sinks").stdout), []
                )
                peer.update(
                    present=True, default_sink="replacement", description="Replacement"
                )
                self.assertEqual(
                    call("--", "set-sink-volume", "fixture.output", "25%").returncode, 1
                )
                self.assertEqual(
                    call("--", "set-sink-volume", "replacement", "25%").returncode, 0
                )
                env["PULSE_SERVER"] = "unix:/run/user/1000/pulse/native"
                guarded = call("--", "set-sink-volume", "replacement", "10%")
                self.assertEqual(guarded.returncode, 2)
                self.assertIn("refuses non-private audio", guarded.stderr)
                self.assertEqual(peer.status()["volume"], 25)
                peer.update(service=False)
                env["PULSE_SERVER"] = f"unix:{directory / 'audio-fixture.sock'}"
                disconnected = call("--format=json", "info")
                self.assertEqual(disconnected.returncode, 1)
                self.assertIn("Connection refused", disconnected.stderr)
                self.assertEqual(peer.status()["volume"], 25)

    def test_fixture_refuses_host_bus_before_creating_programs(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            env = environment(directory)
            env["DBUS_SYSTEM_BUS_ADDRESS"] = "unix:path=/run/dbus/system_bus_socket"
            with (
                self.assertRaisesRegex(ControlError, "owned private buses"),
                control_ui_fixture.services(directory, directory, env, Deadline(1)),
            ):
                self.fail("host-bus fixture started")
            self.assertEqual(list(directory.iterdir()), [])

    def test_fixture_rejects_unsupported_or_out_of_range_mutations(self):
        state = {
            "service": True,
            "present": True,
            "default_sink": "owned",
            "volume": 40,
            "muted": False,
        }
        for arguments in (
            ["set-sink-volume", "owned", "40%"],
            ["--", "set-sink-volume", "owned", "-1%"],
            ["--", "set-sink-volume", "owned", "101%"],
            ["--", "set-sink-volume", "owned", "nan%"],
            ["--", "set-sink-volume", "owned", "32768"],
            ["--", "set-sink-volume", "owned", "-1", "32768"],
            ["--", "set-sink-volume", "owned", "32768", "65537"],
            ["--", "set-sink-volume", "owned", "32768", "32768", "32768"],
            ["--", "set-sink-mute", "owned", "invalid"],
        ):
            with self.subTest(arguments=arguments):
                self.assertNotEqual(control_ui_fixture.respond(state, arguments)[0], 0)
                self.assertEqual(state["volume"], 40)
                self.assertFalse(state["muted"])

    def test_fixture_preserves_channel_order_and_records_raw_volume_writes(self):
        state = {
            "service": True,
            "present": True,
            "default_sink": "owned",
            "description": "Stereo",
            "volume": 40,
            "muted": False,
            "raw_volumes": [13107, 26214],
        }
        self.assertEqual(
            control_ui_fixture.respond(
                state, ["--", "set-sink-volume", "owned", "16384", "32768"]
            )[0],
            0,
        )
        self.assertEqual(state["volume"], 50)
        _, payload, _ = control_ui_fixture.respond(
            state, ["--format=json", "list", "sinks"]
        )
        self.assertEqual(
            json.loads(payload)[0]["volume"],
            {
                "front-left": {"value": 16384},
                "front-right": {"value": 32768},
            },
        )
        control_ui.balanced_channels(state)
        state["raw_volumes"] = [32768, 32768]
        with self.assertRaisesRegex(ControlError, "channel balance"):
            control_ui.balanced_channels(state)

    def test_osd_checks_application_focus_and_panel_separation(self):
        smoke = Mock()
        smoke.target = 17
        smoke.until.side_effect = lambda _name, check: check()
        for panels, focused in (([{"kind": "controls"}], True), ([], False)):
            with self.subTest(panels=panels, focused=focused):
                smoke.control.status.return_value = {
                    "control_osds": [{}],
                    "shell_panels": panels,
                }
                with (
                    patch.object(control_ui, "visible", return_value=[{}]),
                    patch.object(control_ui, "focused", return_value=focused),
                    self.assertRaises(ControlError),
                ):
                    control_ui.osd(smoke, "Volume 45%")


if __name__ == "__main__":
    unittest.main()
