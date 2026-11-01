"""Isolation and protocol evidence for audio and brightness acceptance."""

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
from agentlib.smoke import Smoke


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

    def test_osd_can_expire_while_accessibility_reads_its_text(self):
        smoke = Mock()
        smoke.target = 17
        smoke.until.side_effect = lambda _name, check: check()
        geometry = [{"x": 520, "y": 676, "width": 240, "height": 76}]
        smoke.control.status.return_value = {
            "control_osds": geometry,
            "shell_panels": [],
        }
        events = []

        def receive_input(check_live):
            self.assertEqual(check_live(), geometry)
            events.append("input")

        def capture(_name):
            self.assertEqual(smoke.control.status()["control_osds"], geometry)
            events.append("capture")

        smoke.capture.side_effect = capture

        def snapshot(*_args):
            events.append("text")
            smoke.control.status.return_value = {
                "control_osds": [],
                "shell_panels": [],
            }
            return {"elements": [{"name": "Volume muted"}]}

        with (
            patch.object(control_ui, "a11y", side_effect=snapshot) as a11y,
            patch.object(control_ui, "focused", return_value=True) as focused,
        ):
            control_ui.osd(
                smoke, "Volume muted", capture="muted-osd", while_visible=receive_input
            )
            self.assertEqual(events, ["input", "capture", "text"])
            a11y.assert_called_once_with(
                smoke, "find", control_ui.selector("Volume muted")
            )
            focused.assert_called_with(smoke, 17)
            smoke.capture.assert_called_once_with("muted-osd")
            smoke.record.assert_called_once_with(
                "control-osd", text="Volume muted", focus=17, geometry=geometry
            )
            focus_checks = focused.call_count
            control_ui.dismissed(smoke)
            self.assertEqual(focused.call_count, focus_checks + 1)
            self.assertEqual(
                smoke.record.call_args.args, ("osd-automatically-dismissed",)
            )

    def test_empty_osd_text_retries_only_while_still_visible(self):
        for expired in (False, True):
            with self.subTest(expired=expired):
                smoke = Mock(target=17)
                smoke.until.side_effect = lambda name, check, smoke=smoke: Smoke.until(
                    smoke, name, check
                )
                smoke.control.status.return_value = {"control_osds": [{}]}
                smoke.control.deadline.remaining.side_effect = [
                    1,
                    ControlError("unexpected extra retry"),
                ]
                snapshots = iter(([], [{"name": "Volume muted"}]))

                def snapshot(*_args, expired=expired, smoke=smoke, snapshots=snapshots):
                    if expired:
                        smoke.control.status.return_value = {"control_osds": []}
                    return {"elements": next(snapshots)}

                with (
                    patch.object(control_ui, "a11y", side_effect=snapshot) as a11y,
                    patch.object(control_ui, "focused", return_value=True),
                    patch("agentlib.smoke.time.sleep") as sleep,
                ):
                    if expired:
                        with self.assertRaisesRegex(ControlError, "OSD disappeared"):
                            control_ui.osd(smoke, "Volume muted")
                        self.assertEqual(a11y.call_count, 1)
                        sleep.assert_not_called()
                        smoke.control.deadline.remaining.assert_not_called()
                        smoke.record.assert_not_called()
                    else:
                        control_ui.osd(smoke, "Volume muted")
                        self.assertEqual(a11y.call_count, 2)
                        sleep.assert_called_once_with(0.05)
                        smoke.record.assert_called_once_with(
                            "control-osd", text="Volume muted", focus=17, geometry=[{}]
                        )
                    a11y.assert_called_with(
                        smoke, "find", control_ui.selector("Volume muted")
                    )

    def test_osd_rechecks_live_state_after_input_and_capture(self):
        for stage in ("input", "capture"):
            for fault in ("disappeared", "focus", "panel", "geometry"):
                with self.subTest(stage=stage, fault=fault):
                    smoke = Mock(target=17)
                    smoke.until.side_effect = lambda _name, check: check()
                    smoke.control.status.return_value = {"control_osds": [{"x": 0}]}
                    with (
                        patch.object(control_ui, "a11y") as a11y,
                        patch.object(control_ui, "focused", return_value=True) as focus,
                    ):

                        def disrupt(*_args, fault=fault, smoke=smoke, focus=focus):
                            if fault == "focus":
                                focus.return_value = False
                            elif fault == "panel":
                                smoke.control.status.return_value["shell_panels"] = [{}]
                            else:
                                smoke.control.status.return_value = {
                                    "control_osds": []
                                    if fault == "disappeared"
                                    else [{"x": 1}]
                                }

                        if stage == "capture":
                            smoke.capture.side_effect = disrupt
                        with self.assertRaisesRegex(ControlError, fault):
                            control_ui.osd(
                                smoke,
                                "Volume 45%",
                                capture="volume-up-osd",
                                while_visible=disrupt if stage == "input" else None,
                            )
                        a11y.assert_not_called()
                        smoke.record.assert_not_called()
                        if stage == "input":
                            smoke.capture.assert_not_called()

    def test_osd_rejects_multiple_mapped_surfaces(self):
        smoke = Mock()
        smoke.until.side_effect = lambda _name, check: check()
        smoke.control.status.return_value = {"control_osds": [{}, {}]}
        with (
            patch.object(control_ui, "visible", return_value=[{}]),
            self.assertRaisesRegex(ControlError, "exactly one OSD"),
        ):
            control_ui.osd(smoke, "Volume 45%")

    def test_popup_feedback_rejects_immediate_and_late_osds(self):
        for statuses in (
            [{"control_osds": [{}]}],
            [{"control_osds": []}, {"control_osds": [{}]}],
        ):
            with self.subTest(statuses=statuses):
                smoke = Mock()
                smoke.control.status.side_effect = statuses
                with (
                    patch.object(control_ui.checks, "opened"),
                    self.assertRaisesRegex(ControlError, "adjustment.*OSD"),
                ):
                    control_ui.popup_feedback(smoke, "pointer")

    def test_popup_feedback_verifies_slider_focus_and_captures_without_osd(self):
        smoke = Mock()
        smoke.control.status.return_value = {"control_osds": []}
        smoke.until.side_effect = lambda _name, check: check()
        with (
            patch.object(control_ui.checks, "opened") as opened,
            patch.object(control_ui, "a11y", return_value={"elements": [{}]}) as a11y,
        ):
            control_ui.popup_feedback(smoke, "keyboard", focused_slider="Volume")
        opened.assert_called_once_with(smoke, "controls", exclusive=True)
        self.assertIn("@role='slider'", a11y.call_args.args[2])
        self.assertIn("@name='Volume'", a11y.call_args.args[2])
        self.assertIn("@focused='true'", a11y.call_args.args[2])
        smoke.capture.assert_called_once_with("keyboard")
        smoke.record.assert_called_once_with(
            "dropdown-feedback",
            capture="keyboard",
            focused_slider="Volume",
            osd=False,
        )


if __name__ == "__main__":
    unittest.main()
