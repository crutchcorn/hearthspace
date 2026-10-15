"""Map dismissal restores keyboard focus without revealing distant Firefox."""

import sys
import unittest
from pathlib import Path
from unittest.mock import Mock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib import firefox_shell, map_checks
from agentlib.control import ControlError


class FirefoxMapFocusTests(unittest.TestCase):
    def run_close(
        self,
        *,
        focused_before=False,
        focused_after=True,
        changed_view=None,
        changed_window=None,
    ):
        smoke = Mock(target=7)
        view = {"x": 0, "y": 0, "scale": 0.85}
        window = {"focused": focused_before, "canvas_x": 100000, "canvas_y": 100000}
        closed = False

        def current_window(_target):
            return (
                (window | {"focused": focused_after} | (changed_window or {}))
                if closed
                else window
            )

        def until(name, condition):
            if not condition():
                raise ControlError(name)

        def close(_smoke):
            nonlocal closed
            self.assertFalse(current_window(7)["focused"])
            closed = True

        smoke.window.side_effect = current_window
        smoke.until.side_effect = until
        with (
            patch.object(map_checks, "close", side_effect=close) as dismiss,
            patch.object(
                firefox_shell,
                "settled",
                side_effect=[view, view | (changed_view or {})],
            ),
        ):
            firefox_shell.close_map_preserving_view(smoke)
        dismiss.assert_called_once_with(smoke)
        return smoke

    def test_dismissal_restores_client_focus_after_preserving_open_map_focus(self):
        smoke = self.run_close()
        records = smoke.record.call_args_list
        self.assertEqual(
            [entry.args[0] for entry in records],
            ["shell-map-holds-focus", "shell-map-close-restores-focus"],
        )
        self.assertFalse(records[0].kwargs["window"]["focused"])
        self.assertTrue(records[1].kwargs["window"]["focused"])
        self.assertEqual(records[0].kwargs["viewport"], records[1].kwargs["viewport"])

    def test_open_map_cannot_return_keyboard_focus_before_dismissal(self):
        with self.assertRaisesRegex(ControlError, "open canvas map lost focus"):
            self.run_close(focused_before=True)

    def test_dismissal_must_restore_keyboard_focus(self):
        with self.assertRaisesRegex(ControlError, "restores Firefox keyboard focus"):
            self.run_close(focused_after=False)

    def test_restored_focus_cannot_hide_viewport_or_saved_geometry_changes(self):
        for change in ({"x": 100000}, {"y": 100000}, {"scale": 1}):
            with (
                self.subTest(view=change),
                self.assertRaisesRegex(ControlError, "panned or zoomed"),
            ):
                self.run_close(changed_view=change)
        for change in ({"canvas_x": 0}, {"canvas_y": 0}):
            with (
                self.subTest(window=change),
                self.assertRaisesRegex(ControlError, "saved position"),
            ):
                self.run_close(changed_window=change)


if __name__ == "__main__":
    unittest.main()
