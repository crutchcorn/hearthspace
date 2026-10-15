"""Safety and evidence assertions for the isolated power UI acceptance fixture."""

import json
import sys
import tempfile
import threading
import unittest
from pathlib import Path
from unittest.mock import Mock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib import power_ui, power_ui_fixture
from agentlib.control import ControlError, Deadline
from agentlib.daily_checks import read_line


class PowerFixtureTests(unittest.TestCase):
    def test_fixture_records_requests_without_invoking_host_power(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            socket = directory / "power-control.sock"
            with power_ui_fixture.PowerSocket(
                socket, directory / "calls.jsonl"
            ) as server:
                thread = threading.Thread(
                    target=server.serve_forever, args=(0.01,), daemon=True
                )
                thread.start()
                try:
                    with patch.object(power_ui_fixture.subprocess, "Popen") as process:
                        for command in ("status", "suspend", "status"):
                            reply = json.loads(
                                read_line(
                                    socket,
                                    (json.dumps({"command": command}) + "\n").encode(),
                                    Deadline(2),
                                )
                            )
                            self.assertEqual(
                                reply,
                                {
                                    "ok": True,
                                    "available": True,
                                    "busy": False,
                                    "error": None,
                                },
                            )
                        process.assert_not_called()
                    self.assertEqual(len(server.suspend_events()), 1)
                    self.assertEqual(socket.stat().st_mode & 0o777, 0o600)
                    for invalid in (
                        {"command": "reboot"},
                        {"command": "suspend", "extra": True},
                        [],
                    ):
                        reply = json.loads(
                            read_line(
                                socket,
                                (json.dumps(invalid) + "\n").encode(),
                                Deadline(2),
                            )
                        )
                        self.assertFalse(reply["ok"])
                    self.assertEqual(len(server.events), 3)
                finally:
                    server.shutdown()
                    thread.join(2)
                self.assertFalse(thread.is_alive())

    def test_fixture_rejects_host_bus_before_creating_services(self):
        directory = Path("/private-test")
        env = {
            "PATH": "/build:/usr/bin",
            "DBUS_SYSTEM_BUS_ADDRESS": "unix:path=/run/dbus/system_bus_socket",
            "DBUS_SESSION_BUS_ADDRESS": "unix:path=/private-test/bus",
        }
        with (
            patch.object(Path, "is_file", return_value=True),
            patch.object(power_ui_fixture, "PowerSocket") as endpoint,
            patch.object(power_ui_fixture.subprocess, "Popen") as process,
            self.assertRaisesRegex(ControlError, "matching owned private buses"),
            power_ui_fixture.services(directory, directory, env, Deadline(1)),
        ):
            self.fail("host-bus fixture unexpectedly started")
        endpoint.assert_not_called()
        process.assert_not_called()

    def test_duplicate_notification_check_includes_expired_history(self):
        smoke = Mock()
        with (
            patch.object(
                power_ui,
                "inbox",
                return_value=[{"id": 1, "active": True}, {"id": 2, "active": False}],
            ),
            self.assertRaisesRegex(ControlError, "unexpected battery notification IDs"),
        ):
            power_ui.no_duplicates(smoke, [1])

    def test_warning_rejects_wrong_urgency_or_remaining_percentage(self):
        for item in (
            {"summary": "Critical battery", "urgency": 1, "body": "4% remaining"},
            {"summary": "Critical battery", "urgency": 2, "body": "14% remaining"},
        ):
            with self.subTest(item=item):
                smoke = Mock()
                smoke.until.side_effect = lambda _name, check: check()
                with (
                    patch.object(power_ui, "inbox", return_value=[item]),
                    patch.object(power_ui.checks, "click") as click,
                    self.assertRaisesRegex(ControlError, "incorrect battery warning"),
                ):
                    power_ui.warning(smoke, "Critical battery", 4, 2)
                click.assert_not_called()


if __name__ == "__main__":
    unittest.main()
