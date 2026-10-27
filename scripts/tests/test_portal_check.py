"""Process-only regressions: no compositor, D-Bus daemon or PipeWire needed."""

import os
import signal
import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from portal_checklib import OwnedProcess, cleanup, fixture_environment


class PortalCheckTests(unittest.TestCase):
    def setUp(self):
        self.runtime = tempfile.TemporaryDirectory()
        self.addCleanup(self.runtime.cleanup)
        self.directory = Path(self.runtime.name)

    def spawn(self, code, *args):
        child = OwnedProcess(
            [sys.executable, "-c", code, *args],
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        self.addCleanup(child.stop)
        return child

    def wait_for(self, predicate):
        deadline = time.monotonic() + 5
        while not predicate():
            if time.monotonic() >= deadline:
                self.fail("subprocess fixture did not become ready")
            time.sleep(0.01)

    def descendant_fixture(self, exit_leader):
        ready = self.directory / "descendant"
        leader = self.spawn(
            """
import os, signal, sys, time
from pathlib import Path
ready = Path(sys.argv[1])
if os.fork() == 0:
    signal.signal(signal.SIGTERM, signal.SIG_IGN)
    ready.write_text(str(os.getpid()))
    while True:
        time.sleep(1)
while not ready.exists():
    time.sleep(0.01)
if sys.argv[2] == 'exit':
    os._exit(0)
while True:
    time.sleep(1)
""",
            str(ready),
            "exit" if exit_leader else "wait",
        )
        self.wait_for(lambda: ready.exists() and ready.read_text())
        return leader, int(ready.read_text())

    def assert_terminated(self, pid):
        try:
            state = Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()[0]
        except FileNotFoundError:
            return
        self.assertIn(state, ("Z", "X"), f"descendant {pid} is still live")

    def test_exited_leader_does_not_hide_live_descendant(self):
        leader, descendant = self.descendant_fixture(exit_leader=True)
        self.wait_for(lambda: leader.poll() is not None)
        os.kill(descendant, 0)
        leader.stop(term_timeout=0.05, kill_timeout=2)
        self.assert_terminated(descendant)
        self.assertEqual(leader.poll(), 0)
        leader.stop()  # A finished group must never be signalled again.

    def test_term_exiting_leader_does_not_skip_descendant_escalation(self):
        leader, descendant = self.descendant_fixture(exit_leader=False)
        leader.stop(term_timeout=0.05, kill_timeout=2)
        self.assert_terminated(descendant)
        self.assertEqual(leader.poll(), -signal.SIGTERM)

    def test_failed_group_cleanup_still_stops_other_processes_and_fails(self):
        failed = self.spawn("import time; time.sleep(60)")
        other = self.spawn("import time; time.sleep(60)")
        killpg = os.killpg

        def deny_one_group(pgid, signum):
            if pgid == failed.pid:
                raise PermissionError("fixture denies group signal")
            return killpg(pgid, signum)

        report = {"success": True}
        closed = []
        with (
            patch("portal_checklib.os.killpg", side_effect=deny_one_group),
            self.assertRaisesRegex(RuntimeError, "incomplete portal check cleanup"),
        ):
            cleanup(
                [
                    ("failed group", failed.stop),
                    ("other group", other.stop),
                    ("remaining resources", lambda: closed.append(True)),
                ],
                report,
            )
        self.assertIsNotNone(other.poll())
        self.assertIsNone(failed.poll())
        self.assertEqual(closed, [True])
        self.assertFalse(report["success"])
        self.assertFalse(report["cleanup"])
        self.assertIn("failed group: PermissionError", report["cleanup_errors"][0])

    def test_fixture_environment_cannot_inherit_host_integration(self):
        env = fixture_environment(
            {
                "HEARTHSPACE_SETTINGS_SOCKET": "/host/settings.sock",
                "HEARTHSPACE_LAYOUT_PATH": "/host/layout.json",
                "HEARTHSPACE_FUTURE_INTEGRATION": "/host/unknown",
                "HEARTHSPACE_SESSION_BUS_ADDRESS": "unix:path=/host/bus",
                "DBUS_SYSTEM_BUS_ADDRESS": "unix:path=/host/system-bus",
                "DBUS_STARTER_ADDRESS": "unix:path=/host/starter",
                "PIPEWIRE_CONFIG_NAME": "/host/pipewire.conf",
                "PIPEWIRE_REMOTE": "/host/pipewire-0",
                "DISABLE_RTKIT": "0",
                "PATH": "/usr/bin",
            },
            self.directory,
        )
        self.assertEqual(
            {key for key in env if key.startswith("HEARTHSPACE_")},
            {"HEARTHSPACE_WAYLAND_DISPLAY", "HEARTHSPACE_COMMAND_SOCKET"},
        )
        self.assertEqual(
            env["DBUS_SYSTEM_BUS_ADDRESS"],
            "unix:path=" + str(self.directory / "no-system-bus"),
        )
        self.assertFalse((self.directory / "no-system-bus").exists())
        self.assertEqual(env["DISABLE_RTKIT"], "1")
        self.assertEqual(env["PIPEWIRE_RUNTIME_DIR"], str(self.directory))
        self.assertEqual(env["PIPEWIRE_REMOTE"], "pipewire-0")
        self.assertNotIn("PIPEWIRE_CONFIG_NAME", env)
        self.assertNotIn("DBUS_STARTER_ADDRESS", env)
        self.assertEqual(env["PATH"], "/usr/bin")


if __name__ == "__main__":
    unittest.main()
