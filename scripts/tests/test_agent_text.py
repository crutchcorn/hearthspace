import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import Mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib.cli import action, parser
from agentlib.control import MAX_COMMAND_BYTES, ControlError
from agentlib.text import MAX_TEXT_BYTES, clipboard_command


class ClipboardInputTests(unittest.TestCase):
    def test_unicode_and_newlines_are_exact_single_line_utf8(self):
        text = "café — 日本語 🦀\nsecond line\r\n\t"
        command = clipboard_command(text)
        self.assertEqual(bytes.fromhex(command.split()[1]).decode("utf-8"), text)
        self.assertNotIn("\n", command)
        self.assertEqual(clipboard_command(""), "clipboard-set ")

    def test_byte_limit_fits_control_transport(self):
        command = clipboard_command("é" * (MAX_TEXT_BYTES // 2))
        self.assertLess(len(command.encode()), MAX_COMMAND_BYTES)
        with self.assertRaisesRegex(ControlError, "16 KiB"):
            clipboard_command("é" * (MAX_TEXT_BYTES // 2 + 1))
        with self.assertRaisesRegex(ControlError, "NUL"):
            clipboard_command("hello\0world")

    def test_file_input_preserves_exact_line_endings_and_has_a_bound(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "text.txt"
            expected = "café\r\n日本語\r\n".encode()
            path.write_bytes(expected)
            self.assertEqual(
                clipboard_command(None, path), f"clipboard-set {expected.hex()}"
            )
            path.write_bytes(b"x" * (MAX_TEXT_BYTES + 1))
            with self.assertRaisesRegex(ControlError, "16 KiB"):
                clipboard_command(None, path)

    def test_text_and_file_are_exclusive_and_one_is_required(self):
        for text, path in [(None, None), ("text", Path("unused"))]:
            with (
                self.subTest(text=text),
                self.assertRaisesRegex(ControlError, "either"),
            ):
                clipboard_command(text, path)

    def test_paste_replaces_clipboard_then_sends_terminal_shortcut(self):
        args = parser().parse_args(
            [
                "--socket",
                "/tmp/unused.sock",
                "paste",
                "日本語",
                "--shortcut",
                "ctrl+shift+v",
                "--wait",
                "none",
            ]
        )
        control = Mock(path="/tmp/unused.sock")
        action(args, control)
        self.assertEqual(
            [call.args[0] for call in control.request.call_args_list],
            [
                clipboard_command("日本語"),
                "key-down 29",
                "key-down 42",
                "key-down 47",
                "key-up 47",
                "key-up 42",
                "key-up 29",
            ],
        )

    def test_invalid_paste_does_not_change_clipboard_or_type(self):
        for text, shortcut in [
            ("valid", "invalid+v"),
            ("valid", "ctrl+ctrl+v"),
            ("bad\0", "ctrl+v"),
        ]:
            with self.subTest(text=text, shortcut=shortcut):
                args = parser().parse_args(
                    [
                        "--socket",
                        "/tmp/unused.sock",
                        "paste",
                        text,
                        "--shortcut",
                        shortcut,
                        "--wait",
                        "none",
                    ]
                )
                control = Mock(path="/tmp/unused.sock")
                with self.assertRaises(ControlError):
                    action(args, control)
                control.request.assert_not_called()


if __name__ == "__main__":
    unittest.main()
