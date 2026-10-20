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
            (icons + ["Apps List"], False),
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
        smoke.until.return_value = [
            {"bounds": {"x": 8, "y": 8, "width": 72, "height": 72}}
        ]
        with patch.object(shell_checks, "Input") as input_type:
            shell_checks.click(smoke, "Apps List")
        input_type.return_value.__enter__.return_value.click.assert_called_once_with(
            798, 544
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
