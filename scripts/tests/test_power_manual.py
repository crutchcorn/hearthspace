"""Local unlock acceptance observes real-owner state without authenticating."""

import sys
import unittest
from contextlib import ExitStack
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import Mock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib import power_manual
from agentlib.control import ControlError, Deadline
from agentlib.power import PowerSmoke

LOCKED = {
    "locked": True,
    "lock_confirmed": True,
    "recovery_ready": True,
    "active": True,
    "windows": [],
}
UNLOCKED = LOCKED | {"locked": False, "lock_confirmed": False}
OWNER = {"pid": 20, "start_ticks": 400, "parent_pid": 10, "comm": "hearthspace-loc"}
COMPOSITOR = {"pid": 10, "start_ticks": 300}


class ManualConfigurationTests(unittest.TestCase):
    def test_old_cli_spelling_selects_only_the_new_locker(self):
        args = SimpleNamespace(manual_unlock=True, locker="swaylock")
        power_manual.configure(args)
        self.assertEqual(args.locker, "hearthspace-lock")
        self.assertEqual(args.total_timeout, 540)

    def test_default_mode_never_waits_for_a_password(self):
        args = SimpleNamespace()
        power_manual.configure(args)
        self.assertFalse(args.manual_unlock)
        self.assertIsNone(args.unlock_timeout)
        self.assertEqual(args.total_timeout, 240)

    def test_manual_opt_in_expands_total_budget_and_keeps_explicit_larger_budget(self):
        args = SimpleNamespace(manual_unlock=True, locker="hearthspace-lock")
        power_manual.configure(args)
        self.assertEqual((args.unlock_timeout, args.total_timeout), (300, 540))
        args.unlock_timeout, args.total_timeout = 900, 1200
        power_manual.configure(args)
        self.assertEqual((args.unlock_timeout, args.total_timeout), (900, 1200))

    def test_unsafe_flag_combinations_fail_before_hardware_initialization(self):
        for values in (
            {"manual_unlock": True, "locker": "fixture"},
            {"unlock_timeout": 300},
            {"manual_unlock": True, "locker": "hearthspace-lock", "unlock_timeout": 0},
            {
                "manual_unlock": True,
                "locker": "hearthspace-lock",
                "unlock_timeout": 901,
            },
            {
                "manual_unlock": True,
                "locker": "hearthspace-lock",
                "unlock_timeout": float("nan"),
            },
            {
                "manual_unlock": True,
                "locker": "hearthspace-lock",
                "unlock_timeout": float("inf"),
            },
            {"manual_unlock": True, "locker": "hearthspace-lock", "total_timeout": 539},
            {
                "manual_unlock": True,
                "locker": "hearthspace-lock",
                "total_timeout": float("inf"),
            },
        ):
            args = SimpleNamespace(
                allow_system_suspend=True, wake_after=45, timeout=75, **values
            )
            with (
                self.subTest(values=values),
                patch("agentlib.power.WakeAlarm") as alarm,
                self.assertRaises(ControlError),
            ):
                PowerSmoke(args)
            alarm.assert_not_called()


class ManualObservationTests(unittest.TestCase):
    def setUp(self):
        self.stack = ExitStack()
        self.addCleanup(self.stack.close)
        self.now = 100.0
        self.stack.enter_context(
            patch("agentlib.control.time.monotonic", side_effect=lambda: self.now)
        )
        self.stack.enter_context(
            patch("agentlib.power_manual.time.sleep", side_effect=self.advance)
        )
        self.control = Mock()
        self.controls = self.stack.enter_context(
            patch("agentlib.power_manual.Control", return_value=self.control)
        )
        self.identity = self.stack.enter_context(
            patch(
                "agentlib.power_manual.verify.process_identity", return_value=COMPOSITOR
            )
        )
        self.owners = self.stack.enter_context(
            patch("agentlib.power_manual.verify.owned_lockers", return_value=[OWNER])
        )
        self.smoke = SimpleNamespace(
            args=SimpleNamespace(unlock_timeout=5, timeout=1),
            total=Deadline(100),
            manifest={"command_socket": "/owned/control.sock"},
            processes_before={"compositor": COMPOSITOR},
            owned_lockers=[OWNER],
            record=Mock(),
            preserved_processes=Mock(),
        )

    def advance(self, seconds):
        self.now += seconds

    def test_wait_can_exceed_normal_step_without_sending_input_or_unlock(self):
        def status():
            return LOCKED if self.now < 102 else UNLOCKED

        self.control.status.side_effect = status
        self.owners.side_effect = lambda _: [OWNER] if self.now < 102 else []
        power_manual.observe(self.smoke)
        self.assertGreater(self.now, 101)
        self.control.request.assert_not_called()
        self.control.capture.assert_not_called()
        self.smoke.preserved_processes.assert_called_once()
        steps = [call.args[0] for call in self.smoke.record.call_args_list]
        self.assertIn("manual-unlock-observed", steps)
        self.assertEqual(steps[-1], "step-passed")

    def test_already_unlocked_or_unconfirmed_state_is_not_manual_evidence(self):
        for state in (UNLOCKED, LOCKED | {"lock_confirmed": False}, {}):
            self.control.status.return_value = state
            with self.subTest(state=state), self.assertRaises(ControlError):
                power_manual.observe(self.smoke)
        self.smoke.record.assert_not_called()

    def test_owner_disappearance_without_unlock_times_out_closed(self):
        self.control.status.return_value = LOCKED
        self.owners.side_effect = lambda _: [OWNER] if self.now == 100 else []
        with self.assertRaisesRegex(ControlError, "timed out"):
            power_manual.observe(self.smoke)
        self.assertLessEqual(self.now, 105)
        self.control.request.assert_not_called()
        self.smoke.preserved_processes.assert_not_called()

    def test_wait_preserves_post_unlock_step_budget_inside_total_deadline(self):
        self.control.status.return_value = LOCKED
        self.smoke.total = Deadline(3)
        with self.assertRaisesRegex(ControlError, "timed out"):
            power_manual.observe(self.smoke)
        self.assertLessEqual(self.now, 102)
        self.assertGreaterEqual(self.smoke.total.remaining(), 1)
        start = self.smoke.record.call_args_list[0]
        self.assertEqual(start.kwargs["budget"], 2)

    def test_replaced_locker_or_restarted_compositor_cannot_pass(self):
        self.control.status.return_value = LOCKED
        self.owners.return_value = [OWNER | {"start_ticks": 401}]
        with self.assertRaisesRegex(ControlError, "hearthspace-lock changed"):
            power_manual.observe(self.smoke)
        self.owners.return_value = [OWNER]
        self.identity.return_value = COMPOSITOR | {"start_ticks": 301}
        with self.assertRaisesRegex(ControlError, "compositor changed"):
            power_manual.observe(self.smoke)

    def test_unknown_or_unrecovered_unlock_does_not_enable_desktop_input(self):
        for state in (
            {},
            UNLOCKED | {"active": False},
            UNLOCKED | {"recovery_ready": False},
        ):
            self.control.status.side_effect = [LOCKED, state]
            self.owners.side_effect = [[OWNER], []]
            with self.subTest(state=state), self.assertRaises(ControlError):
                power_manual.observe(self.smoke)
        self.control.request.assert_not_called()
        self.smoke.preserved_processes.assert_not_called()


class ManualWorkflowTests(unittest.TestCase):
    def test_desktop_checks_follow_successful_observation_only(self):
        for failure in (None, ControlError("local unlock timed out")):
            smoke = object.__new__(PowerSmoke)
            smoke.locker, smoke.manual_unlock = "hearthspace-lock", True
            smoke.connect = Mock(
                return_value={
                    "processes": {
                        name: 10
                        for name in ("supervisor", "settingsd", "compositor", "shell")
                    }
                }
            )
            smoke.control = Mock()
            smoke.control.status.return_value = {"outputs": [{}]}
            smoke.step, smoke.record = Mock(), Mock()
            with (
                self.subTest(failure=failure),
                patch(
                    "agentlib.power.verify.process_identity", return_value=COMPOSITOR
                ),
                patch(
                    "agentlib.power.power_manual.observe", side_effect=failure
                ) as observe,
                patch("agentlib.power.subprocess.Popen") as fixture,
            ):
                if failure:
                    with self.assertRaises(ControlError):
                        smoke.workflow()
                    smoke.record.assert_not_called()
                else:
                    smoke.workflow()
                    self.assertTrue(
                        smoke.record.call_args.kwargs["authenticated_unlock_tested"]
                    )
                observe.assert_called_once_with(smoke)
                fixture.assert_not_called()
            steps = [call.args[0] for call in smoke.step.call_args_list]
            self.assertEqual("recovered-desktop" in steps, failure is None)


if __name__ == "__main__":
    unittest.main()
