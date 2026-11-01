"""Private Xwayland verification fails closed without launching any executable."""

import hashlib
import json
import os
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from installlib import xwayland
from xwayland_fixture import companions


class XwaylandCompanionTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.compositor = self.root / "hearthspace-compositor"
        self.compositor.write_bytes(b"compositor; never execute")
        self.compositor.chmod(0o755)
        self.binary = companions(self.root)
        self.receipt = self.root / xwayland.RECEIPT
        self.original = self.receipt.read_bytes()

    def verify(self):
        return xwayland.verify_companions(self.compositor)

    def test_valid_pair_hashes_both_companions_without_execution(self):
        with patch("subprocess.Popen") as execute:
            result = self.verify()
        execute.assert_not_called()
        for path in (self.binary, self.receipt):
            self.assertEqual(result[path.name]["path"], str(path))
            self.assertEqual(
                result[path.name]["sha256"],
                hashlib.sha256(path.read_bytes()).hexdigest(),
            )

    def test_published_companions_require_exact_modes_from_opened_file_records(self):
        for path, modes in (
            (self.binary, (0o777, 0o700, 0o4755)),
            (self.receipt, (0o777, 0o600, 0o664)),
        ):
            for mode in modes:
                with self.subTest(path=path, mode=oct(mode)):
                    companions(self.root)
                    path.chmod(mode)
                    with self.assertRaisesRegex(RuntimeError, "must have mode"):
                        self.verify()

    def test_raw_build_verification_accepts_private_modes_and_records_them(self):
        self.binary.chmod(0o700)
        self.receipt.chmod(0o600)
        result = xwayland.verify_binary(self.binary)
        self.assertEqual(result["binary"]["mode"], 0o700)
        self.assertEqual(result["receipt"]["mode"], 0o600)

    def test_missing_binary_or_receipt_has_explicit_preparation_hint(self):
        for path in (self.binary, self.receipt):
            with self.subTest(path=path):
                path.unlink()
                with self.assertRaisesRegex(RuntimeError, "scripts/prepare-xwayland"):
                    self.verify()
                companions(self.root)

    def test_exact_requirements_reject_newer_versions_and_wrong_source_or_patch(self):
        for key, value in (
            ("version", "24.1.11"),
            ("version", "24.1.10-custom"),
            ("source_archive_sha256", "0" * 64),
            ("surface_input_targets_sha256", "f" * 64),
            ("format_version", 2),
            ("format_version", True),
        ):
            with self.subTest(key=key, value=value):
                receipt = json.loads(self.original)
                receipt[key] = value
                self.receipt.write_text(json.dumps(receipt))
                with self.assertRaisesRegex(RuntimeError, "trusted requirements"):
                    self.verify()

    def test_changed_binary_and_wrong_binary_digest_are_rejected(self):
        self.binary.write_bytes(b"replacement")
        with self.assertRaisesRegex(RuntimeError, "binary sha256"):
            self.verify()
        companions(self.root)
        receipt = json.loads(self.original)
        receipt["sha256"] = "0" * 64
        self.receipt.write_text(json.dumps(receipt))
        with self.assertRaisesRegex(RuntimeError, "binary sha256"):
            self.verify()

    def test_malformed_receipts_and_missing_fields_are_rejected(self):
        invalid = [b"{", b"[]", b"null", b"{}", b"\xff", b'{"sha256":0,"sha256":1}']
        for key in (*xwayland.FIELDS, "sha256"):
            receipt = json.loads(self.original)
            del receipt[key]
            invalid.append(json.dumps(receipt).encode())
        for content in invalid:
            with self.subTest(content=content):
                self.receipt.write_bytes(content)
                with self.assertRaises(RuntimeError):
                    self.verify()

    def test_symlinks_directories_and_fifos_are_never_read_as_companions(self):
        for path in (self.binary, self.receipt):
            for kind in ("symlink", "directory", "fifo"):
                with self.subTest(path=path, kind=kind):
                    path.unlink()
                    if kind == "symlink":
                        path.symlink_to(self.compositor)
                    elif kind == "directory":
                        path.mkdir()
                    else:
                        os.mkfifo(path)
                    with self.assertRaisesRegex(RuntimeError, "regular nonsymlink"):
                        self.verify()
                    if kind == "directory":
                        path.rmdir()
                    else:
                        path.unlink()
                    companions(self.root)

    def test_nonexecutable_private_binary_cannot_fall_back_to_path(self):
        system = self.root / "Xwayland"
        system.write_bytes(b"system fixture")
        system.chmod(0o755)
        self.binary.chmod(0o644)
        with (
            patch.dict(os.environ, {"PATH": str(self.root)}),
            self.assertRaisesRegex(RuntimeError, "not executable"),
        ):
            self.verify()

    def test_companions_follow_actual_compositor_location(self):
        alias_dir = self.root / "alias"
        alias_dir.mkdir()
        alias = alias_dir / self.compositor.name
        alias.symlink_to(self.compositor)
        result = xwayland.verify_companions(alias)
        self.assertEqual(result[xwayland.BINARY]["path"], str(self.binary))
