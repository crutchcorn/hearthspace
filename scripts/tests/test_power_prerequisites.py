"""Reject an unusable real locker before any suspend or native interaction."""

import stat
import sys
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import Mock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib import power_evidence
from agentlib.control import ControlError
from agentlib.power import PowerSmoke


class LockerPrerequisiteTests(unittest.TestCase):
    def test_real_mode_checks_fixed_locker_before_hardware_commands(self):
        smoke = Mock(locker="hearthspace-lock")
        with (
            patch.object(
                power_evidence,
                "locker_ready",
                side_effect=ControlError("policy unavailable"),
            ) as ready,
            patch("agentlib.power.command") as command,
            self.assertRaisesRegex(ControlError, "policy unavailable"),
        ):
            PowerSmoke.preflight(smoke)
        self.assertEqual(ready.call_args.args[0].name, "hearthspace-lock")
        command.assert_not_called()
        smoke.alarm.prove_interrupt.assert_not_called()

    def test_policy_is_fixed_regular_root_owned_and_0644(self):
        binary = Mock()
        binary.is_file.return_value = True
        policy = Mock()
        valid = {"st_mode": stat.S_IFREG | 0o644, "st_uid": 0, "st_gid": 0}
        with patch.object(power_evidence.os, "access", return_value=True):
            policy.lstat.return_value = SimpleNamespace(**valid)
            power_evidence.locker_ready(binary, policy)
            for changes in (
                {"st_uid": 1000},
                {"st_gid": 1000},
                {"st_mode": stat.S_IFREG | 0o666},
                {"st_mode": stat.S_IFLNK | 0o644},
            ):
                policy.lstat.return_value = SimpleNamespace(**(valid | changes))
                with self.subTest(changes=changes), self.assertRaises(ControlError):
                    power_evidence.locker_ready(binary, policy)
            policy.lstat.side_effect = FileNotFoundError
            with self.assertRaisesRegex(
                ControlError, "fixed /etc/pam.d/hearthspace-lock"
            ):
                power_evidence.locker_ready(binary, policy)

    def test_missing_binary_is_not_replaced_by_a_path_lookup(self):
        with (
            tempfile.TemporaryDirectory() as directory,
            self.assertRaisesRegex(ControlError, "build the native locker"),
        ):
            power_evidence.locker_ready(Path(directory) / "hearthspace-lock")


if __name__ == "__main__":
    unittest.main()
