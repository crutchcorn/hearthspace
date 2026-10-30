"""Native preparation records the exact locker before any service or VT launch."""

import hashlib
import json
import sys
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from nativelib import build, cli, common


class NativeLockerTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.directory = Path(temporary.name)
        self.binary_dir = self.directory / "target/debug"
        self.binary_dir.mkdir(parents=True)
        for name in (*common.BINARIES, "hearthspace-share-picker"):
            binary = self.binary_dir / name
            binary.write_bytes(name.encode())
            binary.chmod(0o755)
        source = self.directory / "scripts/runtime/hearthspace-share-picker"
        source.parent.mkdir(parents=True)
        source.write_text("#!/usr/bin/python3\n")
        source.chmod(0o755)
        self.args = cli.parser().parse_args(
            [
                "start",
                "--session",
                str(self.directory),
                "--binary-dir",
                str(self.binary_dir),
                "--no-build",
            ]
        )

    def test_missing_or_nonexecutable_locker_fails_before_privileged_start(self):
        binary = self.binary_dir / "hearthspace-lock"
        binary.unlink()
        for present in (False, True):
            if present:
                binary.write_bytes(b"not executable")
                binary.chmod(0o644)
            with (
                self.subTest(present=present),
                patch.object(cli, "preflight"),
                patch.object(cli, "privileged") as privileged,
                self.assertRaisesRegex(
                    RuntimeError, "missing executable .*hearthspace-lock"
                ),
            ):
                cli.start(self.args)
            privileged.assert_not_called()
            self.assertFalse((self.directory / "binaries.json").exists())

    def test_no_build_still_hashes_the_selected_locker(self):
        with patch.object(build.subprocess, "run") as run:
            build.build(self.args, self.directory)
        run.assert_not_called()
        records = json.loads((self.directory / "binaries.json").read_text())
        locker = records["hearthspace-lock"]
        self.assertEqual(locker["path"], str(self.binary_dir / "hearthspace-lock"))
        self.assertEqual(
            locker["sha256"], hashlib.sha256(b"hearthspace-lock").hexdigest()
        )

    def test_standard_native_build_includes_the_locker_package(self):
        self.args.no_build = False
        with (
            patch.object(build, "ROOT", self.directory),
            patch.object(
                build.subprocess, "run", return_value=SimpleNamespace(returncode=0)
            ) as run,
        ):
            build.build(self.args, self.directory)
        command = run.call_args.args[0]
        index = command.index("hearthspace-lock")
        self.assertEqual(command[index - 1], "-p")
        self.assertIn("hearthspace-compositor/udev", command)


if __name__ == "__main__":
    unittest.main()
