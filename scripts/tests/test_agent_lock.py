import sys
import unittest
from pathlib import Path
from unittest.mock import Mock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib.cli import parser, run
from agentlib.control import ControlError, Deadline
from agentlib.lock import lock_session


class LockTests(unittest.TestCase):
    def test_default_lock_waits_for_confirmation_without_requesting_capture(self):
        control = Mock(deadline=Deadline(1))
        control.status.side_effect = [
            {"locked": False},
            {"locked": True, "lock_confirmed": False},
            {"locked": True, "lock_confirmed": True},
        ]
        with patch("agentlib.lock.time.sleep"):
            result = lock_session(control)
        self.assertTrue(result["lock_confirmed"])
        self.assertEqual(result["verification"], "locked")
        control.request.assert_called_once_with("lock-session")
        control.capture.assert_not_called()

    def test_deadline_does_not_turn_partial_lock_into_a_secured_result(self):
        control = Mock()
        control.status.return_value = {"locked": True, "lock_confirmed": False}
        control.deadline.remaining.side_effect = ControlError("deadline expired")
        with self.assertRaisesRegex(ControlError, "deadline"):
            lock_session(control)
        control.request.assert_not_called()
        control.capture.assert_not_called()

    def test_repeated_lock_waits_for_existing_pending_lock_without_relaunching(self):
        control = Mock(deadline=Deadline(1))
        control.status.side_effect = [
            {"locked": True, "lock_confirmed": False},
            {"locked": True, "lock_confirmed": True},
        ]
        self.assertTrue(lock_session(control)["lock_confirmed"])
        control.request.assert_not_called()
        control.capture.assert_not_called()

    def test_accepted_only_mode_is_explicit_and_already_locked_is_idempotent(self):
        control = Mock()
        control.status.return_value = {"locked": False}
        result = lock_session(control, "none")
        self.assertNotIn("lock_confirmed", result)
        self.assertEqual(result["verification"], "none")
        control.status.assert_called_once()
        control.reset_mock()
        control.status.return_value = {"locked": True, "lock_confirmed": True}
        self.assertTrue(lock_session(control)["lock_confirmed"])
        control.request.assert_not_called()

    def test_cli_routes_lock_without_normal_action_frame_wait(self):
        args = parser().parse_args(["--socket", "/unused", "lock"])
        with patch("agentlib.cli.Control") as constructor:
            constructor.return_value.status.return_value = {"locked": False}
            constructor.return_value.request.side_effect = ControlError(
                "missing hearthspace-lock"
            )
            with self.assertRaisesRegex(ControlError, "missing hearthspace-lock"):
                run(args)
            constructor.return_value.capture.assert_not_called()


if __name__ == "__main__":
    unittest.main()
