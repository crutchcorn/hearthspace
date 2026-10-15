"""Shell acceptance evidence and fixture sequencing regressions."""

import contextlib
import io
import json
import sys
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import Mock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib import daily_checks, shell_checks, shell_smoke
from agentlib.control import ControlError
from agentlib.viewport_fixture import set_viewport


class ShellEvidenceTests(unittest.TestCase):
    def test_settings_launch_requires_a_new_focused_window_and_closes_only_that_id(
        self,
    ):
        smoke = Mock(target=3)
        existing = {
            "id": 7,
            "title": "Hearthspace Settings",
            "mapped": True,
            "focused": True,
        }
        launched = existing | {"id": 9}
        smoke.control.status.side_effect = [
            {"windows": [existing]},
            {"windows": [existing, launched | {"mapped": False}]},
            {"windows": [existing, launched | {"focused": False}]},
            {"windows": [existing, launched]},
        ]
        smoke.window.side_effect = [None, {"focused": True}]

        def until(name, condition):
            for _ in range(5):
                if result := condition():
                    return result
            raise ControlError(name)

        smoke.until.side_effect = until
        with (
            patch.object(shell_smoke.checks, "click") as click,
            patch.object(shell_smoke.checks, "opened"),
            patch.object(shell_smoke.checks, "closed"),
            patch.object(shell_smoke.shell_search, "selected") as selected,
            patch.object(shell_smoke, "Input") as input_type,
        ):
            shell_smoke.launch_settings(smoke)
        self.assertEqual(
            [call.args[1] for call in click.call_args_list],
            ["Search"],
        )
        selected.assert_called_once_with(smoke, "Open: Hearthspace Settings")
        input_type.return_value.__enter__.return_value.chord.assert_called_once_with(
            "enter"
        )
        smoke.control.request.assert_called_once_with("close-window 9")
        smoke.record.assert_called_once_with(
            "settings-launched-from-search", window=launched, closed=True
        )

    def test_notification_client_failure_does_not_wait_for_deadline(self):
        for direct in (False, True):
            with (
                self.subTest(direct=direct),
                tempfile.TemporaryDirectory() as directory,
            ):
                smoke = Mock(
                    notification_via_dbus=direct,
                    marker="FIXTURE",
                    artifacts=Path(directory),
                    manifest={
                        "runtime_dir": directory,
                        "dbus_session_bus_address": "unix:path=/private-test-bus",
                    },
                )
                smoke.until.side_effect = lambda _name, condition: condition()
                process = Mock()
                process.poll.return_value = 1
                with (
                    patch.object(
                        daily_checks.subprocess, "Popen", return_value=process
                    ) as launch,
                    patch.object(daily_checks, "read_line") as inbox,
                    self.assertRaisesRegex(
                        ControlError, "notification client exited 1 before delivery"
                    ),
                ):
                    daily_checks.notifications(smoke)
                inbox.assert_not_called()
                command = launch.call_args.args[0]
                self.assertEqual(command[0], "gdbus" if direct else "notify-send")
                if direct:
                    self.assertIn("unix:path=/private-test-bus", command)
                    self.assertIn("org.freedesktop.Notifications.Notify", command)

    def test_notification_client_failure_after_dismissal_is_not_reported_as_passed(
        self,
    ):
        with tempfile.TemporaryDirectory() as directory:
            smoke = Mock(
                notification_via_dbus=False,
                marker="FIXTURE",
                artifacts=Path(directory),
                manifest={
                    "runtime_dir": directory,
                    "dbus_session_bus_address": "unix:path=/private-test-bus",
                },
            )
            smoke.until.side_effect = lambda _name, condition: condition()
            process = Mock()
            process.poll.side_effect = [None, 1]
            process.wait.return_value = 1
            active = {"id": 1, "summary": "Daily notification FIXTURE", "active": True}
            closed = active | {"active": False, "close_reason": 2}
            replies = [
                json.dumps({"ok": True, "notifications": [item]})
                for item in (active, active, closed)
            ]
            with (
                patch.object(daily_checks.subprocess, "Popen", return_value=process),
                patch.object(daily_checks, "read_line", side_effect=replies),
                patch.object(daily_checks, "a11y", return_value={"elements": [{}]}),
                self.assertRaisesRegex(ControlError, "client failed after dismissal"),
            ):
                daily_checks.notifications(smoke)
            smoke.record.assert_not_called()

    def test_fixture_waits_for_zoom_before_pan_can_cancel_animation(self):
        smoke = Mock()
        calls = []
        smoke.control.request.side_effect = lambda command: calls.append(command)
        smoke.control.status.side_effect = [
            {"viewport": {"scale": 1, "animating": True}},
            {"viewport": {"scale": 1, "animating": False}},
            {"viewport": {"scale": 1, "x": 30, "y": -40}},
        ]

        def until(_name, condition):
            calls.append("wait")
            return condition() or condition()

        smoke.until.side_effect = until
        set_viewport(smoke, x=30, y=-40, scale=1)
        self.assertEqual(calls, ["zoom-to 1", "wait", "pan-to 30 -40", "wait"])

    def test_multiple_matching_panels_are_not_clicked_ambiguously(self):
        smoke = Mock()
        smoke.control.status.return_value = {
            "shell_panels": [{"kind": "search"}, {"kind": "search"}]
        }
        with self.assertRaisesRegex(ControlError, "one matching panel"):
            shell_checks.panel(smoke, "search")

    def test_compact_bar_rejects_old_actions_and_nonadjacent_icons(self):
        icons = list(shell_checks.BAR_ICONS)
        for names, succeeds in [
            (icons, True),
            (icons + ["Lock"], False),
            (icons + ["Overview"], False),
            (icons + ["Extra action"], False),
            (icons + ["Search"], False),
            (["Search", "Canvas map", "Notifications", "Power"], False),
        ]:
            with self.subTest(names=names):
                smoke = Mock()
                buttons = [
                    {"name": name, "bounds": {"x": index * 40}}
                    for index, name in enumerate(names)
                ]
                smoke.until.side_effect = [True, buttons]
                with patch.object(shell_checks, "a11y", return_value={"elements": []}):
                    if succeeds:
                        shell_checks.bar(smoke)
                    else:
                        with self.assertRaises(ControlError):
                            shell_checks.bar(smoke)

    def test_query_text_uses_the_manifest_private_bus_and_observed_node(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory)
            (path / "session.json").write_text(
                json.dumps({"dbus_session_bus_address": "unix:path=/private"})
            )
            smoke = Mock(directory=path)
            node = {"bus_name": ":1.42", "object_path": "/observed/search"}
            with (
                patch.object(shell_checks, "a11y", return_value={"elements": [node]}),
                patch.object(
                    shell_checks,
                    "dbus_value",
                    side_effect=["unix:path=/private-atspi", "settings"],
                ) as call,
            ):
                self.assertEqual(shell_checks.query_text(smoke), "settings")
            self.assertEqual(call.call_args_list[0].args[1], "unix:path=/private")
            self.assertEqual(
                call.call_args.args[1:4],
                ("unix:path=/private-atspi", ":1.42", "/observed/search"),
            )

    def test_power_workflow_only_checks_lock_and_cancels_logout(self):
        smoke = Mock(marker="FIXTURE")
        smoke.control.status.return_value = {"locked": False}
        with (
            patch.object(shell_smoke.checks, "click") as click,
            patch.object(shell_smoke.checks, "opened"),
            patch.object(shell_smoke.checks, "closed"),
            patch.object(shell_smoke.checks, "elements", return_value=[]),
            patch.object(shell_smoke.checks, "panel", return_value=None),
            patch.object(shell_smoke.checks, "tab_to") as tab,
            patch.object(shell_smoke, "Input") as input_type,
        ):
            shell_smoke.pointer_panels(smoke)
        names = [call.args[1] for call in click.call_args_list]
        self.assertIn("Log out", names)
        self.assertNotIn("Cancel", names)
        self.assertNotIn("Confirm log out", names)
        self.assertNotIn("Lock", names)
        self.assertEqual(
            [call.args[1] for call in tab.call_args_list], ["Lock", "Cancel"]
        )
        input_type.return_value.__enter__.return_value.chord.assert_called_once_with(
            "enter"
        )
        smoke.control.request.assert_not_called()

    def test_keyboard_focus_requires_observed_focus_and_never_activates_target(self):
        smoke = Mock()
        with (
            patch.object(shell_checks, "Input") as input_type,
            patch.object(
                shell_checks,
                "a11y",
                side_effect=[{"elements": []}, {"elements": [{"name": "Lock"}]}],
            ) as a11y,
        ):
            shell_checks.tab_to(smoke, "Lock")
        self.assertEqual(
            [
                call.args
                for call in input_type.return_value.__enter__.return_value.chord.call_args_list
            ],
            [("tab",), ("tab",)],
        )
        self.assertIn("@focused='true'", a11y.call_args.args[2])
        self.assertIn("@name='Lock'", a11y.call_args.args[2])
        smoke.record.assert_called_once()

    def test_keyboard_focus_failure_stops_after_eight_tabs(self):
        smoke = Mock()
        with (
            patch.object(shell_checks, "Input") as input_type,
            patch.object(shell_checks, "a11y", return_value={"elements": []}),
            self.assertRaisesRegex(ControlError, "eight presses"),
        ):
            shell_checks.tab_to(smoke, "Cancel")
        chords = input_type.return_value.__enter__.return_value.chord.call_args_list
        self.assertEqual([call.args for call in chords], [("tab",)] * 8)
        smoke.record.assert_not_called()

    def test_failure_keeps_owned_session_cleanup(self):
        with (
            tempfile.TemporaryDirectory() as directory,
            contextlib.redirect_stdout(io.StringIO()),
        ):
            smoke = shell_smoke.ShellSmoke(
                SimpleNamespace(
                    session=Path(directory),
                    timeout=30,
                    total_timeout=120,
                    log_level="info",
                )
            )
            operations = []

            def launcher(operation, *_):
                operations.append(operation)
                (smoke.directory / "session.json").write_text("{}")

            with (
                patch("agentlib.daily.shutil.which", return_value="/test/tool"),
                patch.object(smoke, "launcher", side_effect=launcher),
                patch.object(
                    smoke, "workflow", side_effect=ControlError("query lost characters")
                ),
                patch.object(smoke, "verify_restored") as restored,
                self.assertRaisesRegex(ControlError, "query lost characters"),
            ):
                smoke.run()
            self.assertEqual(operations, ["start", "stop"])
            restored.assert_called_once()


if __name__ == "__main__":
    unittest.main()
