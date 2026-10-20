"""Four-corner purposes and surface-local pointer coordinates."""

import sys
import unittest
from pathlib import Path
from unittest.mock import Mock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib import shell_checks
from agentlib.control import ControlError


def corner_status():
    return {
        "outputs": [
            {
                "name": "test",
                "logical_x": 0,
                "logical_y": 0,
                "logical_width": 1000,
                "logical_height": 600,
                "scale": 1,
            }
        ],
        "shell_corners": [
            {
                "corner": corner,
                "output": "test",
                "x": x,
                "y": y,
                "width": 200,
                "height": 54,
            }
            for corner, x, y in (
                ("notifications", 24, 24),
                ("system", 776, 24),
                ("canvas", 24, 522),
                ("apps", 776, 522),
            )
        ],
    }


def target_snapshot(name, width, *, scale=1, x=4, role="Button"):
    return {
        "elements": [
            {
                "role": "Frame",
                "bounds": {
                    "x": 0,
                    "y": 0,
                    "width": width * scale,
                    "height": 54 * scale,
                },
            },
            {
                "role": role,
                "name": name,
                "bounds": {
                    "x": x * scale,
                    "y": 4 * scale,
                    "width": 36 * scale,
                    "height": 36 * scale,
                },
            },
        ]
    }


class ShellCornerTests(unittest.TestCase):
    def test_waits_for_every_corner_to_register_before_validating_controls(self):
        smoke = Mock()
        buttons = [
            {
                "name": name,
                "bounds": {"x": 4, "y": 4, "width": 36, "height": 36},
            }
            for name in shell_checks.BAR_ICONS
        ]
        partial = [button for button in buttons if button["name"] != "Apps List"]
        smoke.control.status.return_value = corner_status()

        def until(_name, condition):
            self.assertIsNone(condition(), "a partial set must keep waiting")
            return condition()

        smoke.until.side_effect = until
        with (
            patch.object(shell_checks, "closed"),
            patch.object(
                shell_checks,
                "a11y",
                side_effect=[
                    {"elements": partial},
                    {"elements": buttons},
                    {"elements": []},
                ],
            ),
        ):
            shell_checks.bar(smoke)
        self.assertEqual(smoke.record.call_args.kwargs["buttons"], buttons)

    def test_corner_controls_allow_running_apps_but_reject_duplicates_and_old_actions(
        self,
    ):
        icons = list(shell_checks.BAR_ICONS)
        for names, succeeds in [
            (icons, True),
            (icons + ["Switch to Foot, active"], True),
            (icons + ["Lock"], False),
            (icons + ["Overview"], False),
            (icons + ["Control center"], False),
            (icons + ["Apps List"], False),
            ([name for name in icons if name != "Brightness settings"], False),
            (icons[:-1], False),
        ]:
            with self.subTest(names=names):
                smoke = Mock()
                buttons = [
                    {
                        "name": name,
                        "bounds": {"x": 4, "y": 4, "width": 36, "height": 36},
                    }
                    for name in names
                ]
                smoke.until.side_effect = [True, buttons]
                smoke.control.status.return_value = corner_status()
                with patch.object(shell_checks, "a11y", return_value={"elements": []}):
                    if succeeds:
                        shell_checks.bar(smoke)
                    else:
                        with self.assertRaises(ControlError):
                            shell_checks.bar(smoke)

    def test_wrong_corner_position_is_rejected_even_with_correct_button_names(self):
        smoke = Mock()
        status = corner_status()
        status["shell_corners"][3]["y"] = 12
        smoke.control.status.return_value = status
        smoke.until.side_effect = [
            True,
            [{"name": name} for name in shell_checks.BAR_ICONS],
        ]
        with self.assertRaisesRegex(ControlError, "wrong purpose/position"):
            shell_checks.bar(smoke)

    def test_apps_list_click_uses_mapped_bottom_corner_and_scaled_local_bounds(self):
        smoke = Mock()
        status = corner_status()
        status["outputs"][0]["scale"] = 2
        smoke.control.status.return_value = status
        smoke.until.side_effect = lambda _name, condition: condition()
        with (
            patch.object(
                shell_checks,
                "a11y",
                return_value=target_snapshot("Apps List", 200, scale=2),
            ),
            patch.object(shell_checks, "Input") as input_type,
        ):
            shell_checks.click(smoke, "Apps List")
        input_type.return_value.__enter__.return_value.click.assert_called_once_with(
            798, 544
        )

    def test_battery_reading_uses_explicit_system_corner_and_scaled_local_bounds(self):
        smoke = Mock()
        status = corner_status()
        status["outputs"][0]["scale"] = 2
        smoke.control.status.return_value = status
        smoke.until.side_effect = lambda _name, condition: condition()
        with (
            patch.object(
                shell_checks,
                "a11y",
                return_value=target_snapshot("Battery: 72%, Charging", 200, scale=2),
            ),
            patch.object(shell_checks, "Input") as input_type,
        ):
            shell_checks.click(smoke, "Battery: 72%, Charging", corner="system")
        input_type.return_value.__enter__.return_value.click.assert_called_once_with(
            798, 46
        )

    def assert_dock_resize_waits(self, old_width, width, scale):
        smoke = Mock()
        status = corner_status()
        status["outputs"][0]["scale"] = scale
        status["shell_corners"][3].update(x=976 - width, width=width)
        smoke.control.status.return_value = status
        stale = target_snapshot("Apps List", old_width, scale=scale, x=old_width - 40)
        settled = target_snapshot("Apps List", width, scale=scale, x=width - 40)
        with (
            patch.object(
                shell_checks, "a11y", side_effect=[stale, settled]
            ) as accessibility,
            patch.object(shell_checks, "Input") as input_type,
        ):

            def until(_name, condition):
                self.assertIsNone(condition(), "stale frame must keep waiting")
                input_type.assert_not_called()
                return condition()

            smoke.until.side_effect = until
            shell_checks.click(smoke, "Apps List")
        input_type.return_value.__enter__.return_value.click.assert_called_once_with(
            954, 544
        )
        self.assertEqual(accessibility.call_count, 2)
        self.assertIn("/ancestor::*[@role='frame'][1]", accessibility.call_args.args[2])
        self.assertIn(" | ", accessibility.call_args.args[2])
        self.assertEqual(
            smoke.record.call_args.kwargs["elements"], [settled["elements"][1]]
        )

    def test_dock_growth_waits_for_its_accessibility_frame_before_clicking(self):
        # Old local x still lies inside the grown surface, over another app.
        self.assert_dock_resize_waits(old_width=54, width=150, scale=1)

    def test_dock_shrink_waits_for_fresh_bounds_at_two_times_output_scale(self):
        # Old local x lies outside the narrowed corner and must never be used.
        self.assert_dock_resize_waits(old_width=150, width=54, scale=2)

    def test_apps_list_click_waits_for_bottom_corner_to_map_during_startup(self):
        smoke = Mock()
        initial = corner_status()
        initial["shell_corners"] = initial["shell_corners"][:2]
        initial["shell_ready"] = True
        settled = corner_status()
        smoke.control.status.side_effect = [initial, settled, settled]
        with (
            patch.object(
                shell_checks, "a11y", return_value=target_snapshot("Apps List", 200)
            ) as accessibility,
            patch.object(shell_checks, "Input") as input_type,
        ):

            def until(_name, condition):
                self.assertIsNone(condition(), "unmapped corner must keep waiting")
                input_type.assert_not_called()
                accessibility.assert_not_called()
                return condition()

            smoke.until.side_effect = until
            shell_checks.click(smoke, "Apps List")
        input_type.return_value.__enter__.return_value.click.assert_called_once_with(
            798, 544
        )
        accessibility.assert_called_once()

    def test_corner_click_rejects_duplicate_or_wrong_output_instead_of_waiting(self):
        for problem in ("duplicate", "wrong output"):
            with self.subTest(problem=problem):
                smoke = Mock()
                status = corner_status()
                if problem == "duplicate":
                    status["shell_corners"].append(status["shell_corners"][3].copy())
                else:
                    status["shell_corners"][3]["output"] = "another-output"
                smoke.control.status.return_value = status
                smoke.until.side_effect = lambda _name, condition: condition()
                with (
                    patch.object(shell_checks, "Input") as input_type,
                    self.assertRaisesRegex(ControlError, "one mapped apps"),
                ):
                    shell_checks.click(smoke, "Apps List")
                input_type.assert_not_called()

    def test_panel_toggle_click_waits_for_matching_frame_and_mapped_panel(self):
        smoke = Mock()
        status = corner_status()
        status["shell_panels"] = [
            {"kind": "search", "x": 300, "y": 100, "width": 650, "height": 54}
        ]
        smoke.control.status.return_value = status
        smoke.until.side_effect = lambda _name, condition: condition()
        with (
            patch.object(
                shell_checks,
                "a11y",
                return_value=target_snapshot(
                    "Draw to launch", 650, role="ToggleButton"
                ),
            ),
            patch.object(shell_checks, "Input") as input_type,
        ):
            shell_checks.click(smoke, "Draw to launch", "search")
        input_type.return_value.__enter__.return_value.click.assert_called_once_with(
            322, 122
        )

    def test_legacy_bar_height_or_inset_is_rejected(self):
        for field, value in (("height", 48), ("x", 12)):
            with self.subTest(field=field):
                smoke = Mock()
                status = corner_status()
                status["shell_corners"][0][field] = value
                smoke.control.status.return_value = status
                smoke.until.side_effect = [
                    True,
                    [{"name": name} for name in shell_checks.BAR_ICONS],
                ]
                with self.assertRaisesRegex(ControlError, "54px height/24px inset"):
                    shell_checks.bar(smoke)


if __name__ == "__main__":
    unittest.main()
