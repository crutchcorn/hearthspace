"""Acceptance-driver phase and failure contracts at the control transport boundary."""

import sys
import unittest
from pathlib import Path
from unittest.mock import Mock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib import canvas_pan_ui as ui
from agentlib.canvas_search_touch import early_typing
from agentlib.control import ControlError


def one_observation(_name, condition):
    value = condition()
    if not value:
        raise ControlError("expected native state was not observed")
    return value


class ShortcutPhaseTests(unittest.TestCase):
    def smoke(self):
        smoke = Mock()
        smoke.control.path = "/owned-shortcut-control"
        smoke.control.status.return_value = {"shell_panels": []}
        smoke.until.side_effect = one_observation
        # A fixed transport response sequence, not an emulated compositor:
        # baseline + before/after presentation for four explicit input phases.
        smoke.control.query.side_effect = [
            {"canvas_tool": tool}
            for tool in ("select", "select", "select", *("pan",) * 6)
        ]
        return smoke

    def test_tool_is_observed_while_both_keys_held_and_after_each_release(self):
        for meta_first in (False, True):
            with self.subTest(meta_first=meta_first):
                smoke = self.smoke()
                captured_after = []
                smoke.capture.side_effect = (
                    lambda _name, captured_after=captured_after, smoke=smoke: (
                        captured_after.append(
                            [
                                call.args[0]
                                for call in smoke.control.request.call_args_list
                            ]
                        )
                    )
                )
                ui.shortcut(smoke, "h", "pan", meta_first=meta_first)
                releases = (
                    ["key-up 125", "key-up 35"]
                    if meta_first
                    else ["key-up 35", "key-up 125"]
                )
                commands = ["key-down 125", "key-down 35", *releases]
                self.assertEqual(captured_after, [commands[:i] for i in range(1, 5)])
                self.assertEqual(smoke.control.query.call_count, 9)
                self.assertEqual(smoke.control.status.call_count, 4)

    def test_successful_input_ack_without_tool_change_is_not_a_pass(self):
        smoke = self.smoke()
        smoke.control.query.side_effect = None
        smoke.control.query.return_value = {"canvas_tool": "select"}
        with (
            patch("agentlib.input.Control") as cleanup,
            self.assertRaisesRegex(ControlError, "not observed"),
        ):
            ui.shortcut(smoke, "h", "pan")
        self.assertEqual(
            [call.args[0] for call in cleanup.return_value.request.call_args_list],
            ["key-up 35", "key-up 125"],
        )

    def test_meta_release_must_not_open_apps_list(self):
        smoke = self.smoke()
        smoke.control.status.side_effect = [
            {"shell_panels": []},
            {"shell_panels": []},
            {"shell_panels": []},
            {"shell_panels": [{"kind": "search"}]},
        ]
        with self.assertRaisesRegex(ControlError, "meta-up opened Apps List"):
            ui.shortcut(smoke, "h", "pan")

    def test_late_tool_change_after_release_capture_is_rejected(self):
        smoke = self.smoke()
        smoke.control.query.side_effect = [
            {"canvas_tool": tool}
            for tool in ("select", "select", "select", *("pan",) * 5, "select")
        ]
        with self.assertRaisesRegex(ControlError, "meta-up changed the selected tool"):
            ui.shortcut(smoke, "h", "pan")

    def test_failed_pressed_frame_releases_both_owned_keys(self):
        smoke = self.smoke()
        smoke.capture.side_effect = [None, ControlError("capture failed")]
        with (
            patch("agentlib.input.Control") as cleanup,
            self.assertRaisesRegex(ControlError, "capture failed"),
        ):
            ui.shortcut(smoke, "h", "pan")
        self.assertEqual(
            [call.args[0] for call in cleanup.return_value.request.call_args_list],
            ["key-up 35", "key-up 125"],
        )


class SearchOpeningTests(unittest.TestCase):
    def test_cold_search_uses_meta_k_and_types_before_any_readiness_query(self):
        smoke = Mock(marker="HV")
        smoke.control.path = "/owned-search-control"
        smoke.control.query.side_effect = [
            {"query": "HV", "scene": [], "active": True},
            {"active": False},
        ]
        events = []
        smoke.control.request.side_effect = lambda command: events.append(command)

        def observe(name, condition):
            events.append("observe")
            return one_observation(name, condition)

        smoke.until.side_effect = observe
        early_typing(smoke)
        self.assertEqual(
            events,
            [
                "key-down 125",
                "key-down 37",
                "key-up 37",
                "key-up 125",
                "key-down 42",
                "key-down 35",
                "key-up 35",
                "key-up 42",
                "key-down 42",
                "key-down 47",
                "key-up 47",
                "key-up 42",
                "observe",
                "key-down 1",
                "key-up 1",
                "observe",
            ],
        )


if __name__ == "__main__":
    unittest.main()
