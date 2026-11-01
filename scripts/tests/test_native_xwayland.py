"""Validate selected Xwayland companions before submitting a native service."""

import hashlib
import json
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from installlib import xwayland
from nativelib import build, cli
from xwayland_fixture import companions, runtime


class NativeXwaylandTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.binaries = self.root / "binaries"
        runtime(self.binaries)
        self.session = self.root / "session"
        self.session.mkdir()
        self.args = cli.parser().parse_args(
            [
                "start",
                "--session",
                str(self.session),
                "--no-build",
                "--binary-dir",
                str(self.binaries),
            ]
        )

    def test_no_build_hashes_binary_and_receipt_without_starting_a_process(self):
        with patch.object(build.subprocess, "run") as run:
            build.build(self.args, self.session)
        run.assert_not_called()
        records = json.loads((self.session / "binaries.json").read_text())
        for name in (xwayland.BINARY, xwayland.RECEIPT):
            path = self.binaries / name
            self.assertEqual(records[name]["path"], str(path))
            self.assertEqual(
                records[name]["sha256"], hashlib.sha256(path.read_bytes()).hexdigest()
            )

    def test_missing_or_tampered_companion_prevents_privileged_service_submission(self):
        for name in (xwayland.BINARY, xwayland.RECEIPT):
            for missing in (True, False):
                with self.subTest(name=name, missing=missing):
                    companions(self.binaries)
                    path = self.binaries / name
                    if missing:
                        path.unlink()
                    else:
                        path.write_bytes(b"tampered")
                    with (
                        patch.object(cli, "preflight"),
                        patch.object(cli, "privileged") as privileged,
                        self.assertRaisesRegex(RuntimeError, "prepare-xwayland"),
                    ):
                        cli.start(self.args)
                    privileged.assert_not_called()
                    self.assertFalse((self.session / "binaries.json").exists())

    def test_no_build_rejects_wrong_companion_modes_before_privileged_submission(self):
        for name, mode in (
            (xwayland.BINARY, 0o777),
            (xwayland.RECEIPT, 0o777),
            (xwayland.BINARY, 0o700),
            (xwayland.RECEIPT, 0o600),
        ):
            with self.subTest(name=name, mode=oct(mode)):
                companions(self.binaries)
                (self.binaries / name).chmod(mode)
                with (
                    patch.object(cli, "preflight"),
                    patch.object(cli, "privileged") as privileged,
                    self.assertRaisesRegex(RuntimeError, "must have mode"),
                ):
                    cli.start(self.args)
                privileged.assert_not_called()
                self.assertFalse((self.session / "binaries.json").exists())

    def test_compositor_override_requires_its_own_companions(self):
        other = self.root / "other"
        other.mkdir()
        compositor = other / "custom-compositor"
        compositor.write_bytes(b"selected compositor")
        compositor.chmod(0o755)
        self.args.compositor = compositor
        with self.assertRaisesRegex(RuntimeError, "prepare-xwayland"):
            build.build(self.args, self.session)
        companions(other, b"selected private Xwayland")
        build.build(self.args, self.session)
        records = json.loads((self.session / "binaries.json").read_text())
        self.assertEqual(records[xwayland.BINARY]["path"], str(other / xwayland.BINARY))
        self.assertEqual(
            records["hearthspace-compositor"]["sha256"],
            hashlib.sha256(compositor.read_bytes()).hexdigest(),
        )
