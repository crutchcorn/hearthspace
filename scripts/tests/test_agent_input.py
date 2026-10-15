import sys
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib.control import ControlError, Deadline
from agentlib.input import Input, text_keys


class RecordingControl:
    path = "unused"
    deadline = Deadline(60)

    def __init__(self, fail=None):
        self.commands = []
        self.fail = fail

    def request(self, command):
        self.commands.append(command)
        if command == self.fail:
            raise ControlError("lost acknowledgement")


class InputTests(unittest.TestCase):
    def test_device_control_key_names_use_paired_evdev_input(self):
        for name, code in [
            ("volume-up", 115),
            ("volume-down", 114),
            ("volume-mute", 113),
            ("brightness-up", 225),
            ("brightness-down", 224),
        ]:
            with self.subTest(name=name):
                control = RecordingControl()
                with Input(control) as inputs:
                    inputs.chord(name)
                self.assertEqual(
                    control.commands, [f"key-down {code}", f"key-up {code}"]
                )

    def test_chord_press_order_and_reverse_release_order(self):
        control = RecordingControl()
        with Input(control) as inputs:
            inputs.chord("ctrl+shift+a")
        self.assertEqual(
            control.commands,
            [
                "key-down 29",
                "key-down 42",
                "key-down 30",
                "key-up 30",
                "key-up 42",
                "key-up 29",
            ],
        )

    def test_cleanup_includes_press_with_lost_acknowledgement(self):
        control = RecordingControl("key-down 30")
        cleanup = RecordingControl()
        with (
            patch("agentlib.input.Control", return_value=cleanup),
            self.assertRaises(ControlError),
            Input(control) as inputs,
        ):
            inputs.chord("ctrl+a")
        self.assertEqual(cleanup.commands, ["key-up 30", "key-up 29"])

    def test_drag_releases_button_on_motion_failure(self):
        control = RecordingControl("pointer-motion-abs 3.0 4.0")
        cleanup = RecordingControl()
        with (
            patch("agentlib.input.Control", return_value=cleanup),
            self.assertRaises(ControlError),
            Input(control) as inputs,
        ):
            inputs.drag(1, 2, 3, 4, duration=0)
        self.assertEqual(cleanup.commands, ["pointer-button-up 272"])

    def test_scroll_holds_modifier_during_axis_and_releases_afterwards(self):
        control = RecordingControl()
        with Input(control) as inputs:
            inputs.scroll(0, -120, ["super"])
        self.assertEqual(
            control.commands, ["key-down 125", "axis 0 -120", "key-up 125"]
        )

    def test_text_is_validated_before_any_keys_are_sent(self):
        control = RecordingControl()
        with self.assertRaisesRegex(ControlError, "US ASCII"), Input(control) as inputs:
            inputs.type("hello ☃")
        self.assertEqual(control.commands, [])
        self.assertEqual(
            text_keys("A!a\n"), [(30, True), (2, True), (30, False), (28, False)]
        )

    def test_click_orders_motion_press_release(self):
        control = RecordingControl()
        with Input(control) as inputs:
            inputs.click(5, 6)
        self.assertEqual(
            control.commands,
            [
                "pointer-motion-abs 5 6",
                "pointer-button-down 272",
                "pointer-button-up 272",
            ],
        )


if __name__ == "__main__":
    unittest.main()
