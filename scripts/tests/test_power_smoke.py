"""Reject unsafe sleep attempts and false recovery evidence without power actions."""

import sys
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import Mock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib import power_evidence as verify
from agentlib.control import ControlError
from agentlib.power import PowerSmoke
from agentlib.power_rtc import WakeAlarm


class PowerEvidenceTests(unittest.TestCase):
    def test_kernel_success_and_elapsed_suspend_are_both_required(self):
        before = {"success": 5, "fail": 1, "suspended_seconds": 100}
        after = {"success": 6, "fail": 1, "suspended_seconds": 120}
        self.assertEqual(verify.resumed(before, after), 20)
        for wrong in (
            before,
            after | {"success": 7},
            after | {"fail": 2},
            after | {"suspended_seconds": 100.1},
        ):
            with self.subTest(wrong=wrong), self.assertRaises(ControlError):
                verify.resumed(before, wrong)

    def test_resume_requires_rearmed_inhibitor_and_fresh_lock_presentation(self):
        before = {"sleep_count": 3, "resume_count": 2}
        after = {
            "phase": "awake",
            "connected": True,
            "sleep_inhibited": True,
            "lock_confirmed": True,
            "active": True,
            "sleep_count": 4,
            "resume_count": 3,
            "last_error": None,
        }
        verify.recovered(before, after)
        for key, wrong in (
            ("phase", "recovering"),
            ("connected", False),
            ("sleep_inhibited", False),
            ("lock_confirmed", False),
            ("active", False),
            ("sleep_count", 3),
            ("resume_count", 2),
            ("last_error", "resume failed"),
        ):
            with self.subTest(key=key), self.assertRaises(ControlError):
                verify.recovered(before, after | {key: wrong})

    def test_locked_metadata_is_redacted_and_presentation_is_confirmed(self):
        status = {"locked": True, "lock_confirmed": True, "windows": []}
        verify.locked(status)
        for wrong in ({"locked": False}, {"lock_confirmed": False}, {"windows": [{}]}):
            with self.subTest(wrong=wrong), self.assertRaises(ControlError):
                verify.locked(status | wrong)

    def test_preserved_window_rejects_relaunch_and_lost_geometry(self):
        window = {
            "id": 5,
            "app_id": "foot",
            "title": "survivor",
            "canvas_x": 60,
            "canvas_y": 70,
            "width": 800,
            "height": 600,
            "mapped": True,
            "visible": True,
        }
        verify.preserved(window, window)
        for wrong in (
            None,
            window | {"id": 6},
            window | {"title": "new shell"},
            window | {"canvas_x": 20},
            window | {"width": 600},
            window | {"mapped": False},
            window | {"visible": False},
        ):
            with self.subTest(wrong=wrong), self.assertRaises(ControlError):
                verify.preserved(window, wrong)

    def test_resume_requires_new_flip_on_the_same_output(self):
        before = {"output": "eDP-1", "frame_id": 40, "presentation": "page-flip"}
        after = before | {"frame_id": 41}
        verify.fresh_frame(before, after)
        for wrong in (
            before,
            after | {"output": "HDMI-1"},
            after | {"presentation": "render"},
        ):
            with self.subTest(wrong=wrong), self.assertRaises(ControlError):
                verify.fresh_frame(before, wrong)

    def test_suspend_requires_explicit_opt_in_before_touching_hardware(self):
        args = SimpleNamespace(allow_system_suspend=False)
        with patch("agentlib.power.WakeAlarm") as alarm:
            with self.assertRaisesRegex(ControlError, "allow-system-suspend"):
                PowerSmoke(args)
            alarm.assert_not_called()

    def test_bounded_wake_and_command_timeouts_are_required(self):
        for seconds, timeout in ((0, 60), (46, 90), (20, 25)):
            args = SimpleNamespace(
                allow_system_suspend=True, wake_after=seconds, timeout=timeout
            )
            with (
                self.subTest(seconds=seconds, timeout=timeout),
                self.assertRaises(ControlError),
            ):
                PowerSmoke(args)


class WakeAlarmTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.path = self.root / "rtc0"
        (self.path / "device/power").mkdir(parents=True)
        (self.path / "device/power/wakeup").write_text("enabled\n")
        (self.path / "wakealarm").write_text("")
        (self.path / "since_epoch").write_text("1000\n")
        self.record = Mock()
        self.alarm = WakeAlarm("rtc0", self.record, self.root)

    def test_foreign_alarm_is_never_overwritten(self):
        (self.path / "wakealarm").write_text("2000\n")
        with patch.object(self.alarm, "rtcwake") as rtcwake:
            for operation in (self.alarm.prove_interrupt, lambda: self.alarm.arm(20)):
                with self.assertRaisesRegex(ControlError, "another task"):
                    operation()
            rtcwake.assert_not_called()
        self.assertEqual((self.path / "wakealarm").read_text(), "2000\n")

    def test_disabled_wakeup_refuses_suspend_without_modifying_system_policy(self):
        (self.path / "device/power/wakeup").write_text("disabled\n")
        with self.assertRaisesRegex(ControlError, "already be enabled"):
            self.alarm.available()

    def test_alarm_must_be_read_back_before_suspend(self):
        with (
            patch.object(self.alarm, "rtcwake", return_value={"argv": []}),
            self.assertRaisesRegex(ControlError, "readback"),
        ):
            self.alarm.arm(20)
        self.assertIsNone(self.alarm.owned)

    def test_arm_and_cleanup_only_touch_the_owned_alarm(self):
        def rtcwake(mode, _seconds=None):
            (self.path / "wakealarm").write_text("1020\n" if mode == "no" else "")
            return {"argv": [mode]}

        with patch.object(self.alarm, "rtcwake", side_effect=rtcwake) as run:
            self.alarm.arm(20)
            self.assertEqual(self.alarm.owned, 1020)
            self.alarm.clear()
            self.assertEqual(run.call_args.args, ("disable",))
            self.assertIsNone(self.alarm.owned)

    def test_alarm_changed_by_another_owner_is_preserved(self):
        self.alarm.owned = 1020
        (self.path / "wakealarm").write_text("2000\n")
        with patch.object(self.alarm, "rtcwake") as rtcwake:
            with self.assertRaisesRegex(ControlError, "changed ownership"):
                self.alarm.clear()
            rtcwake.assert_not_called()

    def test_failed_command_does_not_claim_an_unrelated_new_alarm(self):
        def failing_command(*_):
            (self.path / "wakealarm").write_text("2000\n")
            raise ControlError("command failed")

        with (
            patch.object(self.alarm, "rtcwake", side_effect=failing_command),
            self.assertRaisesRegex(ControlError, "refusing to claim"),
        ):
            self.alarm.arm(20)
        self.assertIsNone(self.alarm.owned)
        self.assertEqual((self.path / "wakealarm").read_text(), "2000\n")

    def test_timed_out_awake_probe_clears_only_its_own_alarm(self):
        def rtcwake(mode, _seconds=None):
            if mode == "on":
                (self.path / "wakealarm").write_text("1003\n")
                raise TimeoutError("no interrupt")
            (self.path / "wakealarm").write_text("")

        with (
            patch.object(self.alarm, "rtcwake", side_effect=rtcwake) as run,
            self.assertRaisesRegex(TimeoutError, "no interrupt"),
        ):
            self.alarm.prove_interrupt()
        self.assertEqual(run.call_args.args, ("disable",))
        self.assertIsNone(self.alarm.owned)

    def test_fired_alarm_needs_no_additional_privileged_command(self):
        self.alarm.owned = 1020
        with patch.object(self.alarm, "rtcwake") as rtcwake:
            self.alarm.clear()
            rtcwake.assert_not_called()
        self.assertIsNone(self.alarm.owned)

    def test_awake_interrupt_probe_never_uses_a_sleep_mode(self):
        with (
            patch.object(self.alarm, "rtcwake", return_value={"argv": ["on"]}) as run,
            patch("agentlib.power_rtc.time.monotonic", side_effect=[100, 103]),
        ):
            self.alarm.prove_interrupt()
            run.assert_called_once_with("on", 3)
        self.assertEqual(self.record.call_args.args, ("rtc-interrupt-proven",))

    def test_invalid_device_names_never_reach_sysfs(self):
        for value in ("../rtc0", "rtc0/wakealarm", "/dev/rtc0", "other"):
            with self.subTest(value=value), self.assertRaises(ControlError):
                WakeAlarm(value, Mock(), self.root)


if __name__ == "__main__":
    unittest.main()
