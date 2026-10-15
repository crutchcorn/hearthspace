"""Automatic lock acceptance never authenticates or borrows another session's locker."""

import os
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import Mock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib import power_evidence as verify
from agentlib.control import ControlError
from agentlib.power import PowerSmoke


class AutomaticLockerTests(unittest.TestCase):
    def test_automatic_mode_never_launches_or_releases_the_test_fixture(self):
        smoke = object.__new__(PowerSmoke)
        smoke.locker = "swaylock"
        smoke.connect = Mock(
            return_value={
                "processes": {
                    key: 10
                    for key in ("supervisor", "settingsd", "compositor", "shell")
                }
            }
        )
        smoke.control = Mock()
        smoke.control.status.return_value = {"outputs": [{}]}
        smoke.step = Mock()
        with (
            patch("agentlib.power.verify.process_identity", return_value={"pid": 10}),
            patch("agentlib.power.subprocess.Popen") as fixture,
        ):
            smoke.workflow()
            fixture.assert_not_called()
        self.assertEqual(
            [call.args[0] for call in smoke.step.call_args_list],
            [
                "ready",
                "power-ready",
                "preserved-app",
                "unlocked-before-sleep",
                "suspend-resume",
                "automatic-swaylock",
            ],
        )
        smoke.control.request.assert_not_called()

    def test_initially_locked_desktop_cannot_prove_automatic_locking(self):
        smoke = Mock()
        for status in (
            {"locked": True, "lock_confirmed": True},
            {"locked": True, "lock_confirmed": False},
            {"locked": False, "lock_confirmed": True},
            {},
        ):
            smoke.control.status.return_value = status
            with (
                self.subTest(status=status),
                self.assertRaisesRegex(ControlError, "unlocked"),
            ):
                PowerSmoke.unlocked(smoke)
        smoke.record.assert_not_called()

    def test_preexisting_swaylock_cannot_prove_prepare_for_sleep_launched_it(self):
        smoke = Mock()
        smoke.manifest = {"compositor_pid": 10}
        smoke.control.status.return_value = {"locked": False, "lock_confirmed": False}
        with (
            patch("agentlib.power.verify.owned_swaylocks", return_value=[{"pid": 20}]),
            self.assertRaisesRegex(ControlError, "already exists"),
        ):
            PowerSmoke.unlocked(smoke)
        smoke.record.assert_not_called()

    def test_automatic_lock_pass_explicitly_leaves_authentication_untested(self):
        smoke = Mock()
        smoke.until.return_value = [{"pid": 20}]
        PowerSmoke.automatic_locker(smoke)
        smoke.preserved_processes.assert_called_once()
        smoke.locked.assert_called_once()
        smoke.control.request.assert_not_called()
        self.assertFalse(smoke.record.call_args.kwargs["authenticated_unlock_tested"])
        self.assertEqual(
            smoke.record.call_args.kwargs["cleanup"], "owned-native-session-stop"
        )

    def test_only_same_cgroup_same_user_compositor_children_count(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            compositor = root / "10"
            compositor.mkdir()
            group = "0::/user.slice/session-103.scope\n"
            (compositor / "cgroup").write_text(group)
            for pid, ppid, uid, cgroup, name in (
                (20, 10, os.getuid(), group, "swaylock"),
                (21, 10, os.getuid(), "0::/other.scope\n", "swaylock"),
                (22, 11, os.getuid(), group, "swaylock"),
                (23, 10, os.getuid(), group, "fake"),
                (24, 10, os.getuid() + 1, group, "swaylock"),
            ):
                path = root / str(pid)
                path.mkdir()
                (path / "comm").write_text(name)
                (path / "status").write_text(
                    f"PPid:\t{ppid}\nUid:\t{uid} {uid} {uid} {uid}\n"
                )
                (path / "cgroup").write_text(cgroup)
                (path / "stat").write_text(f"{pid} ({name}) S " + "0 " * 18 + "444")
            found = verify.owned_swaylocks(10, root)
            self.assertEqual([item["pid"] for item in found], [20])
            self.assertEqual(found[0]["start_ticks"], 444)
            self.assertEqual(found[0]["cgroup"], group)


if __name__ == "__main__":
    unittest.main()
