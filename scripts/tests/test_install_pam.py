"""Fixed PAM policy transactions use temporary staging roots, never host PAM."""

import json
import os
import stat
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from installlib import layout, pam, transaction


class InstallPamTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.directory = Path(temporary.name)
        self.prefix = Path("/opt/hearthspace")
        self.root = self.directory / "opt/hearthspace"
        self.policy = self.directory / pam.POLICY
        self.binaries = self.directory / "binaries"
        self.binaries.mkdir()
        for name in layout.BINARIES:
            binary = self.binaries / name
            binary.write_bytes(b"never executed")
            binary.chmod(0o755)
        self.bundle = self.directory / "bundle"
        self.bundle.mkdir()
        layout.bundle(self.bundle, self.binaries, self.prefix)

    def install(self):
        return transaction.install(
            self.bundle, self.root, self.prefix, system_root=self.directory
        )

    def uninstall(self):
        transaction.uninstall(self.root, self.prefix, system_root=self.directory)

    def custom(self, content=b"auth required local_admin_policy.so\n"):
        self.policy.parent.mkdir(parents=True, exist_ok=True)
        self.policy.write_bytes(content)
        self.policy.chmod(0o644)

    def manifest(self):
        return json.loads((self.root / layout.MANIFEST).read_text())

    def test_policy_is_tracked_reinstalled_and_removed_with_its_prefix(self):
        self.install()
        original = self.policy.read_bytes()
        self.assertEqual(stat.S_IMODE(self.policy.stat().st_mode), 0o644)
        self.assertEqual(self.policy.stat().st_uid, os.geteuid())
        self.assertEqual(self.manifest()["pam"]["path"], str(pam.POLICY))
        self.assertEqual(self.manifest()["pam"]["sha256"], layout.digest(self.policy))
        self.assertIn(b"auth include login\naccount include login\n", original)
        self.install()
        self.assertEqual(self.policy.read_bytes(), original)
        self.uninstall()
        self.assertFalse(self.policy.exists())

    def test_existing_custom_or_identical_policy_is_never_claimed_or_removed(self):
        for content in (
            b"auth required local_admin_policy.so\n",
            (layout.REPOSITORY / "data/pam.d/hearthspace-lock").read_bytes(),
        ):
            with self.subTest(content=content):
                self.custom(content)
                inode = self.policy.stat().st_ino
                self.install()
                self.assertIsNone(self.manifest()["pam"])
                self.install()
                self.uninstall()
                self.assertEqual(self.policy.read_bytes(), content)
                self.assertEqual(self.policy.stat().st_ino, inode)

    def test_changed_managed_policy_aborts_install_and_uninstall_before_mutation(self):
        self.install()
        original = self.policy.read_bytes()
        saved = (self.root / layout.MANIFEST).read_bytes()
        for change in ("contents", "mode"):
            with self.subTest(change=change):
                self.policy.write_bytes(
                    original + b"# admin edit\n" if change == "contents" else original
                )
                self.policy.chmod(0o600 if change == "mode" else 0o644)
                for operation in (self.install, self.uninstall):
                    with self.assertRaises(RuntimeError):
                        operation()
                    self.assertEqual((self.root / layout.MANIFEST).read_bytes(), saved)
                    self.assertTrue((self.root / layout.LAUNCHER).exists())

    def test_invalid_existing_policy_permissions_and_owner_abort(self):
        self.custom()
        self.policy.chmod(0o666)
        with self.assertRaisesRegex(RuntimeError, "0644"):
            self.install()
        self.policy.chmod(0o644)
        with (
            patch.object(pam.os, "geteuid", return_value=os.geteuid() + 1),
            self.assertRaisesRegex(RuntimeError, "owned by"),
        ):
            self.install()
        self.assertFalse((self.root / layout.CURRENT).exists())

    def test_symlink_policy_and_parent_never_redirect_writes_or_deletes(self):
        other = self.directory / "other"
        other.mkdir()
        target = other / "hearthspace-lock"
        target.write_bytes(b"administrator policy")
        self.policy.parent.mkdir(parents=True)
        self.policy.symlink_to(target)
        with self.assertRaisesRegex(RuntimeError, "symlink"):
            self.install()
        self.policy.unlink()
        self.policy.parent.rmdir()
        self.policy.parent.symlink_to(other, target_is_directory=True)
        with self.assertRaisesRegex(RuntimeError, "symlink"):
            self.install()
        self.assertEqual(target.read_bytes(), b"administrator policy")

    def test_failed_first_install_removes_only_the_new_policy(self):
        write = transaction.atomic_write

        def fail_manifest(path, content, mode):
            if path == self.root / layout.MANIFEST:
                raise OSError("manifest failure")
            write(path, content, mode)

        with (
            patch.object(transaction, "atomic_write", side_effect=fail_manifest),
            self.assertRaisesRegex(OSError, "manifest failure"),
        ):
            self.install()
        self.assertFalse(self.policy.exists())
        self.assertFalse((self.root / layout.CURRENT).exists())
        self.install()

    def test_failed_policy_upgrade_restores_previous_bytes_and_manifest(self):
        self.install()
        before = self.policy.read_bytes()
        manifest = (self.root / layout.MANIFEST).read_bytes()
        install_policy = pam.Policy.install

        def update(policy, content):
            install_policy(policy, content + b"# new packaged policy\n")

        write = transaction.atomic_write

        def fail_manifest(path, content, mode):
            if path == self.root / layout.MANIFEST:
                raise OSError("upgrade failure")
            write(path, content, mode)

        with (
            patch.object(pam.Policy, "install", autospec=True, side_effect=update),
            patch.object(transaction, "atomic_write", side_effect=fail_manifest),
            self.assertRaisesRegex(OSError, "upgrade failure"),
        ):
            self.install()
        self.assertEqual(self.policy.read_bytes(), before)
        self.assertEqual((self.root / layout.MANIFEST).read_bytes(), manifest)

    def test_concurrent_creation_is_not_replaced_or_unlinked_during_rollback(self):
        link = pam.os.link

        def race(source, target):
            target.write_bytes(b"concurrent administrator policy")
            return link(source, target)

        with (
            patch.object(pam.os, "link", side_effect=race),
            self.assertRaises(FileExistsError),
        ):
            self.install()
        self.assertEqual(self.policy.read_bytes(), b"concurrent administrator policy")
        self.assertFalse((self.root / layout.CURRENT).exists())

    def test_policy_owned_by_another_prefix_cannot_be_borrowed(self):
        self.custom(pam.OWNER_HEADER + b"/some/other/prefix\nauth include login\n")
        with self.assertRaisesRegex(RuntimeError, "another installation"):
            self.install()
        self.assertFalse((self.root / layout.CURRENT).exists())

    def test_manifest_cannot_redirect_policy_removal(self):
        self.install()
        manifest = self.manifest()
        manifest["pam"]["path"] = "etc/pam.d/login"
        (self.root / layout.MANIFEST).write_text(json.dumps(manifest))
        with self.assertRaisesRegex(RuntimeError, "unknown managed PAM"):
            self.uninstall()
        self.assertTrue(self.policy.exists())
        self.assertTrue((self.root / layout.LAUNCHER).exists())

    def test_live_policy_owner_is_always_root_even_for_a_nonroot_caller(self):
        with patch.object(pam.os, "geteuid", return_value=1000):
            policy = pam.Policy(Path("/"), self.prefix, None)
        self.assertEqual((policy.uid, policy.gid), (0, 0))

    def test_locker_and_packaged_policy_are_required_bundle_members(self):
        bundle_manifest = self.bundle / "bundle.json"
        original = bundle_manifest.read_text()
        for missing in ("bin/hearthspace-lock", str(layout.PAM_SOURCE)):
            manifest = json.loads(original)
            del manifest["files"][missing]
            bundle_manifest.write_text(json.dumps(manifest))
            with (
                self.subTest(missing=missing),
                self.assertRaisesRegex(RuntimeError, "complete runtime"),
            ):
                self.install()
            self.assertFalse(self.policy.exists())
            self.assertFalse((self.root / layout.CURRENT).exists())


if __name__ == "__main__":
    unittest.main()
