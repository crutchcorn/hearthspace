"""Sharing consent contract with a substitute only at the external GTK dialog."""

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

PICKER = Path(__file__).resolve().parents[1] / "runtime/hearthspace-share-picker"
SOURCES = "Monitor: eDP-1 Built-in display\nWindow: Notes (window-42)\n"


class SharePickerTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.directory = Path(temporary.name)
        self.record = self.directory / "dialog.json"
        dialog = self.directory / "zenity"
        dialog.write_text(
            f"#!{sys.executable}\n"
            "import json, os, sys\n"
            "from pathlib import Path\n"
            "Path(os.environ['RECORD']).write_text(json.dumps({\n"
            "'argv': sys.argv[1:], 'input': sys.stdin.read(),\n"
            "'backend': os.environ.get('GDK_BACKEND'),\n"
            "'portal': os.environ.get('GTK_USE_PORTAL')}))\n"
            "print(os.environ.get('SELECTION', ''))\n"
            "sys.exit(int(os.environ.get('EXIT', '0')))\n"
        )
        dialog.chmod(0o755)

    def run_picker(self, sources=SOURCES, selection="1", code=0):
        return subprocess.run(
            [sys.executable, "-B", PICKER],
            input=sources,
            text=True,
            capture_output=True,
            timeout=5,
            check=False,
            env={
                "PATH": str(self.directory),
                "RECORD": str(self.record),
                "SELECTION": selection,
                "EXIT": str(code),
                "GDK_BACKEND": "x11",
                "GTK_USE_PORTAL": "1",
            },
        )

    def test_explicit_window_selection_returns_exact_offered_source(self):
        result = self.run_picker()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, "Window: Notes (window-42)\n")
        dialog = json.loads(self.record.read_text())
        self.assertEqual(
            dialog["input"], "Screen: eDP-1 Built-in display\n0\nWindow: Notes\n1\n"
        )
        self.assertIn("--ok-label=Share", dialog["argv"])
        self.assertIn("--cancel-label=Cancel", dialog["argv"])
        self.assertEqual((dialog["backend"], dialog["portal"]), ("wayland", "0"))

    def test_explicit_monitor_selection_returns_exact_offered_source(self):
        result = self.run_picker(selection="0")
        self.assertEqual(result.stdout, "Monitor: eDP-1 Built-in display\n")

    def test_duplicate_and_untitled_windows_retain_visible_identity(self):
        for title in ("Notes", ""):
            with self.subTest(title=title):
                source = f"Window: {title} (first)\nWindow: {title} (second)\n"
                result = self.run_picker(source)
                self.assertEqual(result.stdout, f"Window: {title} (second)\n")
                rows = json.loads(self.record.read_text())["input"]
                self.assertEqual(
                    rows, f"Window: {title} (first)\n0\nWindow: {title} (second)\n1\n"
                )

    def test_cancel_and_dialog_failure_never_fall_back_to_first_screen(self):
        for code in (1, 5, 127):
            with self.subTest(code=code):
                self.assertEqual(self.run_picker(selection="0", code=code).stdout, "")

    def test_title_ending_with_another_identifier_cannot_create_duplicate_rows(self):
        sources = (
            "Window: Notes (first)\nWindow: Notes (second)\n"
            "Window: Notes (first) (third)\n"
        )
        result = self.run_picker(sources, selection="2")
        self.assertEqual(result.stdout, "Window: Notes (first) (third)\n")
        rows = json.loads(self.record.read_text())["input"].splitlines()[::2]
        self.assertEqual(rows, sources.splitlines())

    def test_empty_invalid_or_multiple_selection_never_grants_a_source(self):
        for selection in ("", "999", "-1", "0|1", "0\n1", "01", "Window: Notes"):
            with self.subTest(selection=selection):
                self.assertEqual(self.run_picker(selection=selection).stdout, "")

    def test_markup_and_shell_text_remain_literal_dialog_content(self):
        source = "Window: <b>$(touch never)</b> --title=Other (window-7)\n"
        result = self.run_picker(source, selection="0")
        self.assertEqual(result.stdout, source)
        dialog = json.loads(self.record.read_text())
        self.assertIn("--no-markup", dialog["argv"])
        self.assertNotIn("--title=Other", dialog["argv"])
        self.assertIn("<b>$(touch never)</b> --title=Other", dialog["input"])

    def test_ambiguous_malformed_and_excessive_sources_fail_before_dialog(self):
        for source in (
            SOURCES + SOURCES,
            "Unknown: a\n",
            "Monitor: a\0b\n",
            "x" * 262145,
        ):
            with self.subTest(source=source[:40]):
                result = self.run_picker(source)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(result.stdout, "")
                self.assertFalse(self.record.exists())

    def test_no_sources_or_missing_dialog_never_grants_a_source(self):
        self.assertEqual(self.run_picker("").stdout, "")
        self.assertFalse(self.record.exists())
        (self.directory / "zenity").unlink()
        result = self.run_picker()
        self.assertEqual(result.stdout, "")
        self.assertNotEqual(result.returncode, 0)


if __name__ == "__main__":
    unittest.main()
