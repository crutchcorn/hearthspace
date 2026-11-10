"""Private X11 companions are bundled and checked before privileged launch."""

import hashlib
import json
import os
import sys
import tarfile
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from installlib import layout, satellite, transaction, xwayland
from nativelib import build, cli
from xwayland_fixture import runtime


class SatelliteRuntimeTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.binaries = self.root / "bin"
        runtime(self.binaries)

    def test_release_contains_verified_private_server_notices_and_modified_source(self):
        prefix = Path("/opt/hearthspace")
        first = self.root / "first"
        first.mkdir()
        spec = layout.bundle(first, self.binaries, prefix)
        self.assertEqual(transaction.verify_bundle(first, prefix), spec)
        release = first / "release"
        self.assertEqual(spec["files"][f"bin/{xwayland.RECEIPT}"]["mode"], 0o644)
        self.assertEqual(spec["files"][f"bin/{xwayland.BINARY}"]["mode"], 0o755)
        self.assertEqual(
            (release / satellite.NOTICE).read_bytes(),
            (layout.REPOSITORY / "vendor/xwayland-satellite/LICENSE").read_bytes(),
        )
        with tarfile.open(release / satellite.SOURCE, "r:xz") as source:
            for name in (
                "src/lib.rs",
                "src/server/event.rs",
                "PROVENANCE.md",
                "LICENSE",
            ):
                self.assertEqual(
                    source.extractfile(f"xwayland-satellite/{name}").read(),
                    (
                        layout.REPOSITORY / "vendor/xwayland-satellite" / name
                    ).read_bytes(),
                )
            self.assertTrue(
                all(member.uid == member.gid == member.mtime == 0 for member in source)
            )
        second = self.root / "second"
        second.mkdir()
        self.assertEqual(
            layout.bundle(second, self.binaries, prefix)["release"], spec["release"]
        )

    def test_missing_or_mismatched_server_prevents_vt_acquisition(self):
        session = self.root / "session"
        args = cli.parser().parse_args(
            [
                "start",
                "--session",
                str(session),
                "--binary-dir",
                str(self.binaries),
                "--no-build",
            ]
        )
        (self.binaries / xwayland.BINARY).write_bytes(b"changed binary")
        with (
            patch.object(cli, "preflight"),
            patch.object(cli, "privileged") as privileged,
            self.assertRaisesRegex(RuntimeError, "sha256 does not match"),
        ):
            cli.start(args)
        privileged.assert_not_called()

    def test_native_provenance_uses_companions_of_actual_compositor_override(self):
        custom = self.root / "custom"
        runtime(custom)
        (custom / xwayland.BINARY).write_bytes(b"custom prepared server")
        receipt = {
            **xwayland.requirements(),
            "sha256": hashlib.sha256(b"custom prepared server").hexdigest(),
        }
        (custom / xwayland.RECEIPT).write_text(json.dumps(receipt))
        args = cli.parser().parse_args(
            [
                "start",
                "--session",
                str(self.root / "session"),
                "--binary-dir",
                str(self.binaries),
                "--compositor",
                str(custom / "hearthspace-compositor"),
                "--no-build",
            ]
        )
        args.session.mkdir()
        with patch.object(build, "ROOT", self.root):
            build.build(args, args.session)
        records = json.loads((args.session / "binaries.json").read_text())
        for name in (
            "hearthspace-compositor",
            "hearthspace-satellite",
            xwayland.BINARY,
            xwayland.RECEIPT,
        ):
            self.assertEqual(records[name]["path"], str(custom / name))
        self.assertEqual(records[xwayland.BINARY]["sha256"], receipt["sha256"])
        self.assertNotIn("Xwayland", os.listdir(self.binaries))
