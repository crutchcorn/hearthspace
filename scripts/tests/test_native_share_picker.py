"""Prepare consent helpers before an owned session takes the VT."""

import hashlib
import json
import sys
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from nativelib import build, common


class NativeSharePickerTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.source = self.root / "scripts/runtime/hearthspace-share-picker"
        self.source.parent.mkdir(parents=True)
        self.source.write_bytes(b"#!/usr/bin/python3\n# fresh consent\n")
        self.source.chmod(0o755)

    def prepare(self, location, dev=False):
        binaries = self.root / location
        binaries.mkdir(parents=True)
        for name in common.BINARIES:
            path = binaries / name
            path.write_bytes(b"fixture")
            path.chmod(0o755)
        return SimpleNamespace(binary_dir=binaries, dev=dev, no_build=True)

    def test_default_prebuilt_target_stages_and_records_current_picker(self):
        args = self.prepare("target/debug")
        target = args.binary_dir / self.source.name
        target.write_bytes(b"old picker")
        with patch.object(build, "ROOT", self.root):
            build.build(args, self.root)
        self.assertEqual(target.read_bytes(), self.source.read_bytes())
        record = json.loads((self.root / "binaries.json").read_text())[self.source.name]
        self.assertEqual(record["path"], str(target))
        self.assertEqual(
            record["sha256"], hashlib.sha256(target.read_bytes()).hexdigest()
        )

    def test_custom_release_is_not_modified_to_hide_a_missing_picker(self):
        args = self.prepare("release/bin")
        with (
            patch.object(build, "ROOT", self.root),
            self.assertRaisesRegex(RuntimeError, "complete runtime"),
        ):
            build.build(args, self.root)
        self.assertFalse((args.binary_dir / self.source.name).exists())
        self.assertFalse((self.root / "binaries.json").exists())

    def test_development_session_records_source_picker_without_staging(self):
        args = self.prepare("target/debug", dev=True)
        with patch.object(build, "ROOT", self.root):
            build.build(args, self.root)
        self.assertFalse((args.binary_dir / self.source.name).exists())
        record = json.loads((self.root / "binaries.json").read_text())[self.source.name]
        self.assertEqual(record["path"], str(self.source))


if __name__ == "__main__":
    unittest.main()
