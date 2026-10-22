"""Reject early activation, lost persistence and pinch takeover of held Alt+Tab."""

import copy
import sys
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import Mock, call, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib.control import ControlError
from agentlib.window_switcher_pinch import focused, unchanged_focus
from agentlib.window_switcher_smoke import WindowSwitcherSmoke
from test_window_switcher_smoke import preview_rects, status


def state(*, active=False, source="gesture", selected=3, focused=2):
    result = status(active=active, focused=focused)
    result["window_switcher"].update(source=source, selected_window_id=selected)
    return result


class SwitcherPinchEvidenceTests(unittest.TestCase):
    def test_held_pinch_cannot_open_the_switcher_before_gesture_end(self):
        before = state()
        unchanged_focus(before, state(), active=False, target=3, source="gesture")
        with self.assertRaisesRegex(ControlError, "lifetime"):
            unchanged_focus(
                before, state(active=True), active=False, target=3, source="gesture"
            )

    def test_successful_inward_pinch_keeps_next_mru_preview_open_after_lift(self):
        before = state()
        unchanged_focus(
            before, state(active=True), active=True, target=3, source="gesture"
        )
        with self.assertRaisesRegex(ControlError, "lifetime"):
            unchanged_focus(before, state(), active=True, target=3, source="gesture")
        with self.assertRaisesRegex(ControlError, "wrong client"):
            unchanged_focus(
                before,
                state(active=True, selected=2),
                active=True,
                target=3,
                source="gesture",
            )

    def test_repeated_inward_pinch_cannot_cycle_selection(self):
        before = state(active=True)
        unchanged_focus(
            before, copy.deepcopy(before), active=True, target=3, source="gesture"
        )
        with self.assertRaisesRegex(ControlError, "wrong client"):
            unchanged_focus(
                before,
                state(active=True, selected=1),
                active=True,
                target=3,
                source="gesture",
            )

    def test_outward_pinch_dismisses_without_committing_selected_client(self):
        before = state(active=True)
        unchanged_focus(before, state(), active=False, target=3, source="gesture")
        with self.assertRaisesRegex(ControlError, "actual client focus"):
            unchanged_focus(
                before, state(focused=3), active=False, target=3, source="gesture"
            )

    def test_keyboard_owner_cannot_be_closed_or_replaced_by_a_pinch(self):
        before = state(active=True, source="keyboard")
        unchanged_focus(
            before, copy.deepcopy(before), active=True, target=3, source="keyboard"
        )
        with self.assertRaisesRegex(ControlError, "owner"):
            unchanged_focus(
                before, state(active=True), active=True, target=3, source="keyboard"
            )
        with self.assertRaisesRegex(ControlError, "lifetime"):
            unchanged_focus(before, state(), active=True, target=3, source="keyboard")

    def test_active_pinch_cannot_reorder_or_move_the_existing_grid(self):
        before = state(active=True)
        for key, value in (("window_ids", [3, 2, 1]), ("output", "other")):
            after = copy.deepcopy(before)
            after["window_switcher"][key] = value
            with self.subTest(key=key), self.assertRaisesRegex(ControlError, "order"):
                unchanged_focus(before, after, active=True, target=3, source="gesture")

    def test_native_suite_includes_pinch_stages_for_supported_contact_counts(self):
        smoke = Mock(args=SimpleNamespace(device="both"))
        smoke.control.status.return_value = {"outputs": [{"name": "test"}]}
        WindowSwitcherSmoke.workflow(smoke)
        steps = {call.args[0] for call in smoke.step.call_args_list}
        for prefix in ("touchpad-4", "touchscreen-4", "touchscreen-5"):
            for case in (
                "in-subthreshold",
                "in-open",
                "in-repeat",
                "out-subthreshold",
                "out-dismiss",
                "keyboard-owner",
                "focused-view",
            ):
                self.assertIn(f"{prefix}-pinch-{case}", steps)
        self.assertIn("touchpad-4-pinch-in-cancelled", steps)
        self.assertIn("touchscreen-4-pinch-in-cancelled", steps)
        self.assertFalse(any(step.startswith("touchpad-5") for step in steps))

    def test_focused_pinch_dismisses_then_reopens_and_commits_with_enter(self):
        before = state()
        original = before["windows"][1]
        original.update(
            focused_view=True,
            maximized=True,
            content_width=1200,
            content_height=756,
            presentation_output="test",
        )
        held = copy.deepcopy(before)
        held["window_switcher"] = status(active=True, display=preview_rects())[
            "window_switcher"
        ]
        held["window_switcher"]["source"] = "gesture"
        after = status(4500, 2650, focused=3)
        after["windows"][1].update(
            maximized=True, content_width=1152, content_height=564
        )
        sample = status(
            4500, 2650, focused=3, animating=True, display=preview_rects(70)
        )
        smoke = Mock(targets={1: (0, 255, 0), 2: (255, 0, 0), 3: (0, 0, 255)})

        def observe(_smoke, operation, *, active):
            self.assertFalse(active)
            operation()
            return [sample, after]

        with (
            patch(
                "agentlib.window_switcher_pinch.focused_checks.enter",
                return_value=before,
            ),
            patch(
                "agentlib.window_switcher_pinch.focused_checks.visible",
                return_value=100,
            ),
            patch("agentlib.window_switcher_pinch.gesture") as gesture,
            patch(
                "agentlib.window_switcher_pinch.settled",
                side_effect=[held, before, held, after],
            ),
            patch("agentlib.window_switcher_pinch.Input") as inputs,
            patch("agentlib.window_switcher_pinch.observe", side_effect=observe),
        ):
            focused(smoke, "touchscreen", 5)
        self.assertEqual(
            gesture.call_args_list,
            [
                call(smoke, "touchscreen", 5, label="focused-open"),
                call(smoke, "touchscreen", 5, outward=True, label="focused-dismiss"),
                call(smoke, "touchscreen", 5, label="focused-reopen"),
            ],
        )
        inputs.return_value.__enter__.return_value.chord.assert_called_once_with(
            "enter"
        )
        self.assertEqual(
            smoke.record.call_args.args[0], "focused-switcher-pinch-committed"
        )


if __name__ == "__main__":
    unittest.main()
