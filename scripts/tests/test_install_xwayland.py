"""Private Xwayland is owned, pinned and verified in every installed release."""

import hashlib
import json
import stat
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from installlib import cli, layout, transaction, xwayland
from xwayland_fixture import companions, runtime


class InstallXwaylandTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.binaries = self.root / "binaries"
        runtime(self.binaries)
        self.prefix = self.root / "prefix"
        self.system = self.root / "system"
        self.number = 0

    def bundle(self):
        self.number += 1
        bundle = self.root / f"bundle-{self.number}"
        bundle.mkdir()
        layout.bundle(bundle, self.binaries, self.prefix)
        return bundle

    def install(self, bundle=None):
        return transaction.install(
            bundle or self.bundle(), self.prefix, self.prefix, system_root=self.system
        )

    def rewrite_manifest(self, bundle):
        path = bundle / "bundle.json"
        spec = json.loads(path.read_text())
        for name, value in spec["files"].items():
            value["sha256"] = layout.digest(bundle / "release" / name)
        spec["release"] = hashlib.sha256(
            json.dumps(spec["files"], sort_keys=True).encode()
        ).hexdigest()
        path.write_text(json.dumps(spec))

    def test_complete_inventory_tracks_companions_and_notice_with_exact_modes(self):
        bundle = self.bundle()
        spec = transaction.verify_bundle(bundle, self.prefix)
        release = self.install(bundle)
        installed = transaction.installed(self.prefix, self.prefix)
        for name, mode in xwayland.RELEASE_MODES.items():
            path = release / name
            self.assertEqual(spec["files"][name]["mode"], mode)
            self.assertEqual(stat.S_IMODE(path.stat().st_mode), mode)
            self.assertEqual(
                installed["files"][str(path.relative_to(self.prefix))]["sha256"],
                layout.digest(path),
            )
        self.assertEqual(
            (release / xwayland.NOTICE).read_bytes(),
            (layout.REPOSITORY / "patches/xwayland-COPYING").read_bytes(),
        )
        xwayland.verify_companions(release / "bin/hearthspace-compositor")

    def test_missing_companion_fails_before_privileges_even_with_destdir(self):
        (self.binaries / xwayland.RECEIPT).unlink()
        for staging in (False, True):
            command = ["install", "--no-build", "--binary-dir", str(self.binaries)]
            if staging:
                command.extend(["--destdir", str(self.root / "destdir")])
            with (
                self.subTest(staging=staging),
                patch.object(layout, "dependencies"),
                patch.object(cli, "privileged") as privileged,
                self.assertRaisesRegex(RuntimeError, "prepare-xwayland"),
            ):
                cli.run(cli.parser().parse_args(command))
            privileged.assert_not_called()
            self.assertFalse((self.root / "destdir").exists())

    def test_system_xwayland_is_not_a_session_dependency_equivalent(self):
        with (
            patch.object(
                layout.shutil,
                "which",
                side_effect=lambda name: (
                    None if name == "Xwayland" else "/fixture/tool"
                ),
            ),
            patch.object(layout.os, "access", return_value=True),
            patch.object(layout, "validate_backend"),
        ):
            layout.dependencies()

    def test_every_new_artifact_is_required_in_the_bundle_inventory(self):
        for name in xwayland.RELEASE_MODES:
            with self.subTest(name=name):
                bundle = self.bundle()
                path = bundle / "bundle.json"
                spec = json.loads(path.read_text())
                del spec["files"][name]
                path.write_text(json.dumps(spec))
                with self.assertRaisesRegex(RuntimeError, "complete runtime"):
                    self.install(bundle)
                self.assertFalse((self.prefix / layout.CURRENT).exists())

    def test_bundle_hashes_cannot_authorize_a_different_requirement_receipt(self):
        bundle = self.bundle()
        receipt = bundle / "release/bin" / xwayland.RECEIPT
        data = json.loads(receipt.read_text())
        data["version"] = "24.1.11"
        receipt.write_text(json.dumps(data))
        self.rewrite_manifest(bundle)
        with self.assertRaisesRegex(RuntimeError, "trusted requirements"):
            self.install(bundle)
        self.assertFalse((self.prefix / layout.CURRENT).exists())

    def test_tamper_or_wrong_modes_are_rejected_before_activation(self):
        for name in xwayland.RELEASE_MODES:
            with self.subTest(name=name):
                bundle = self.bundle()
                path = bundle / "release" / name
                path.write_bytes(path.read_bytes() + b"tampered")
                with self.assertRaisesRegex(RuntimeError, "changed or missing"):
                    self.install(bundle)
        bundle = self.bundle()
        path = bundle / "release/bin" / xwayland.RECEIPT
        path.chmod(0o755)
        spec_path = bundle / "bundle.json"
        spec = json.loads(spec_path.read_text())
        spec["files"][f"bin/{xwayland.RECEIPT}"]["mode"] = 0o755
        spec_path.write_text(json.dumps(spec))
        self.rewrite_manifest(bundle)
        with self.assertRaisesRegex(RuntimeError, "invalid file mode"):
            self.install(bundle)

    def test_staged_pair_is_reverified_after_all_copy_operations(self):
        bundle = self.bundle()
        original = transaction.shutil.copyfile

        def copy(source, target):
            result = original(source, target)
            if Path(target).name == "Xwayland-COPYING":
                staged_binary = Path(target).parents[3] / "bin" / xwayland.BINARY
                staged_binary.write_bytes(b"changed after its own copy check")
            return result

        with (
            patch.object(transaction.shutil, "copyfile", side_effect=copy),
            self.assertRaisesRegex(RuntimeError, "binary sha256"),
        ):
            self.install(bundle)
        self.assertFalse((self.prefix / layout.CURRENT).exists())
        self.assertFalse((self.prefix / layout.BASE / "releases").exists())

    def test_failed_upgrade_restores_previous_pair_and_release_pin(self):
        old = self.install()
        old_receipt = (old / "bin" / xwayland.RECEIPT).read_bytes()
        companions(self.binaries, b"new private Xwayland fixture")
        bundle = self.bundle()
        original = transaction.atomic_write
        failed = False

        def write(path, content, mode):
            nonlocal failed
            if path == self.prefix / layout.MANIFEST and not failed:
                failed = True
                raise OSError("injected commit failure")
            return original(path, content, mode)

        with (
            patch.object(transaction, "atomic_write", side_effect=write),
            self.assertRaisesRegex(OSError, "injected commit"),
        ):
            self.install(bundle)
        self.assertEqual((self.prefix / layout.CURRENT).resolve(), old)
        self.assertEqual((old / "bin" / xwayland.RECEIPT).read_bytes(), old_receipt)
        xwayland.verify_companions(old / "bin/hearthspace-compositor")
        self.assertEqual(list(old.parent.iterdir()), [old])

    def test_upgrade_keeps_old_pair_and_uninstall_removes_both_owned_pairs(self):
        old = self.install()
        companions(self.binaries, b"new private Xwayland fixture")
        new = self.install()
        self.assertNotEqual(new, old)
        self.assertEqual((self.prefix / layout.CURRENT).resolve(), new)
        for release in (old, new):
            xwayland.verify_companions(release / "bin/hearthspace-compositor")
        foreign = self.prefix / "bin/Xwayland"
        foreign.write_bytes(b"operator file")
        transaction.uninstall(self.prefix, self.prefix, system_root=self.system)
        self.assertEqual(foreign.read_bytes(), b"operator file")
        for release in (old, new):
            for name in xwayland.RELEASE_MODES:
                self.assertFalse((release / name).exists())

    def test_tampered_owned_companion_blocks_uninstall_without_removing_files(self):
        release = self.install()
        (release / "bin" / xwayland.RECEIPT).write_text("tampered")
        with self.assertRaisesRegex(RuntimeError, "changed or missing"):
            transaction.uninstall(self.prefix, self.prefix, system_root=self.system)
        self.assertTrue((release / "bin" / xwayland.BINARY).exists())
        self.assertTrue((self.prefix / layout.LAUNCHER).exists())
