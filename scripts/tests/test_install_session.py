"""Staged installation coverage; never touch host sessions or require sudo."""

import json
import os
import stat
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from installlib import cli, layout, transaction


class InstallSessionTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.directory = Path(self.temporary.name)
        self.system_root = self.directory / "system"
        self.prefix = self.directory / "prefix with spaces"
        self.binaries = self.directory / "build"
        self.binaries.mkdir()
        self.counter = 0
        for name in layout.BINARIES:
            path = self.binaries / name
            path.write_text("#!/bin/sh\nexit 0\n")
            path.chmod(0o755)
        (self.binaries / "hearthspace-session").write_text(
            "#!/usr/bin/python3\nimport json, os, sys\n"
            "print(json.dumps({'argv': sys.argv, 'env': dict(os.environ)}))\n"
        )

    def bundle(self, prefix=None):
        self.counter += 1
        bundle = self.directory / f"bundle-{self.counter}"
        bundle.mkdir()
        layout.bundle(bundle, self.binaries, prefix or self.prefix)
        return bundle

    def install(self):
        return transaction.install(
            self.bundle(), self.prefix, self.prefix, system_root=self.system_root
        )

    def test_staged_layout_contains_complete_runtime_and_gdm_entry(self):
        prefix = Path("/usr/local")
        args = cli.parser().parse_args(
            [
                "install",
                "--no-build",
                "--binary-dir",
                str(self.binaries),
                "--destdir",
                str(self.directory / "stage"),
            ]
        )
        with patch.object(cli, "privileged", wraps=cli.privileged) as apply:
            cli.run(args)
        root = cli.root_path(args)
        release = (root / layout.CURRENT).resolve()
        self.assertEqual(apply.call_count, 1)
        self.assertTrue((release / "bin/hearthspace-session-bus").is_file())
        self.assertTrue((release / "bin/hearthspace-lock").is_file())
        policy = args.destdir / "etc/pam.d/hearthspace-lock"
        self.assertEqual(stat.S_IMODE(policy.stat().st_mode), 0o644)
        self.assertIn("auth include login\naccount include login\n", policy.read_text())
        self.assertFalse((root / "etc/pam.d/hearthspace-lock").exists())
        for name in (
            *layout.BINARIES,
            "hearthspace-login-guard",
            "hearthspace-share-picker",
        ):
            self.assertEqual(
                stat.S_IMODE((release / "bin" / name).stat().st_mode), 0o755
            )
        self.assertIn(
            "Icon=hearthspace-settings",
            (
                release / "share/applications/dev.hearthspace.Settings.desktop"
            ).read_text(),
        )
        self.assertTrue(
            (release / "share/icons/hicolor/scalable/apps/hearthspace.svg").is_file()
        )
        self.assertEqual(
            (
                release / "share/icons/hicolor/scalable/apps/hearthspace-settings.svg"
            ).read_bytes(),
            (layout.REPOSITORY / "assets/settings.svg").read_bytes(),
        )
        entry = (root / layout.SESSION).read_text()
        self.assertIn('Exec="/usr/local/bin/hearthspace-login"', entry)
        self.assertIn("TryExec=/usr/local/bin/hearthspace-login\n", entry)
        self.assertIn("X-GDM-SessionRegisters=false", entry)
        self.assertNotIn(str(self.directory), entry)
        self.assertEqual(transaction.installed(root, prefix)["prefix"], "/usr/local")

    def test_launcher_is_checkout_independent_and_pins_one_release(self):
        release = self.install()
        process = subprocess.run(
            [self.prefix / layout.LAUNCHER, "--print-env"],
            cwd="/",
            env={
                "PATH": "/usr/bin:/bin",
                "DISPLAY": ":42",
                "WAYLAND_DISPLAY": "parent",
                "HEARTHSPACE_DEV": "1",
                "HEARTHSPACE_REPO_ROOT": "/missing",
            },
            capture_output=True,
            text=True,
            check=True,
        )
        result = json.loads(process.stdout)
        self.assertEqual(result["argv"][0], str(release / "bin/hearthspace-session"))
        self.assertEqual(
            result["argv"][1:],
            [
                "--installed",
                "--backend",
                "native",
                "--login-session",
                "--no-watch-apps",
                "--binary-dir",
                str(release / "bin"),
                "--print-env",
            ],
        )
        self.assertTrue(result["env"]["PATH"].startswith(str(release / "bin") + ":"))
        self.assertTrue(
            result["env"]["XDG_DATA_DIRS"].startswith(str(release / "share") + ":")
        )
        self.assertEqual(result["env"]["DISPLAY"], ":42")
        self.assertEqual(result["env"]["WAYLAND_DISPLAY"], "parent")
        self.assertNotIn("HEARTHSPACE_DEV", result["env"])
        self.assertNotIn("HEARTHSPACE_REPO_ROOT", result["env"])

    def test_upgrade_retains_previous_release_and_reinstall_is_idempotent(self):
        old = self.install()
        self.assertEqual(self.install(), old)
        (self.binaries / "hearthspace-settings").write_text("#!/bin/sh\necho updated\n")
        new = self.install()
        self.assertNotEqual(new, old)
        self.assertTrue((old / "bin/hearthspace-settings").exists())
        self.assertEqual((self.prefix / layout.CURRENT).resolve(), new)
        manifest = transaction.installed(self.prefix, self.prefix)
        self.assertTrue(
            any(str(old.relative_to(self.prefix)) in path for path in manifest["files"])
        )

    def test_failed_upgrade_restores_previous_pointer_and_owned_files(self):
        old = self.install()
        manifest = (self.prefix / layout.MANIFEST).read_bytes()
        (self.binaries / "hearthspace-settings").write_text("#!/bin/sh\necho updated\n")
        bundle = self.bundle()
        original = transaction.atomic_write
        failed = False

        def write(path, content, mode):
            nonlocal failed
            if path == self.prefix / layout.SESSION and not failed:
                failed = True
                raise OSError("injected install failure")
            return original(path, content, mode)

        with (
            patch.object(transaction, "atomic_write", side_effect=write),
            self.assertRaisesRegex(OSError, "injected"),
        ):
            transaction.install(
                bundle, self.prefix, self.prefix, system_root=self.system_root
            )
        self.assertEqual((self.prefix / layout.CURRENT).resolve(), old)
        self.assertEqual((self.prefix / layout.MANIFEST).read_bytes(), manifest)
        self.assertEqual(list(old.parent.iterdir()), [old])
        transaction.installed(self.prefix, self.prefix)

    def test_failed_first_install_rolls_back_and_allows_a_clean_retry(self):
        for failure in ("copy", "entry", "manifest"):
            with self.subTest(failure=failure):
                self.prefix = self.directory / f"first-install-{failure}"
                self.system_root = self.directory / f"system-{failure}"
                bundle = self.bundle()
                original_write = transaction.atomic_write
                failed = False

                def write(
                    path,
                    content,
                    mode,
                    *,
                    failure=failure,
                    original_write=original_write,
                ):
                    nonlocal failed
                    target = layout.SESSION if failure == "entry" else layout.MANIFEST
                    if not failed and path == self.prefix / target:
                        failed = True
                        raise OSError("injected first install failure")
                    return original_write(path, content, mode)

                if failure == "copy":
                    fault = patch.object(
                        transaction.shutil,
                        "copyfile",
                        side_effect=OSError("injected first install failure"),
                    )
                else:
                    fault = patch.object(transaction, "atomic_write", side_effect=write)
                with fault, self.assertRaisesRegex(OSError, "first install failure"):
                    transaction.install(
                        bundle, self.prefix, self.prefix, system_root=self.system_root
                    )
                self.assertFalse((self.prefix / layout.CURRENT).is_symlink())
                self.assertFalse((self.prefix / layout.BASE / "releases").exists())
                release = transaction.install(
                    bundle, self.prefix, self.prefix, system_root=self.system_root
                )
                self.assertEqual((self.prefix / layout.CURRENT).resolve(), release)
                transaction.installed(self.prefix, self.prefix)

    def test_uninstall_removes_only_owned_files_and_preserves_foreign_content(self):
        release = self.install()
        unrelated = self.prefix / "share/wayland-sessions/ubuntu.desktop"
        unrelated.write_text("unrelated")
        foreign = release / "keep.txt"
        foreign.write_text("operator file")
        transaction.uninstall(self.prefix, self.prefix, system_root=self.system_root)
        self.assertEqual(unrelated.read_text(), "unrelated")
        self.assertEqual(foreign.read_text(), "operator file")
        self.assertFalse((self.prefix / layout.LAUNCHER).exists())
        self.assertFalse((self.prefix / layout.CURRENT).is_symlink())
        self.assertFalse((self.prefix / layout.MANIFEST).exists())

    def test_modified_files_or_active_processes_abort_before_any_uninstall(self):
        self.install()
        entry = self.prefix / layout.SESSION
        original = entry.read_bytes()
        entry.write_bytes(original + b"# operator change\n")
        with self.assertRaisesRegex(RuntimeError, "changed or missing"):
            transaction.uninstall(
                self.prefix, self.prefix, system_root=self.system_root
            )
        self.assertTrue((self.prefix / layout.LAUNCHER).exists())
        entry.write_bytes(original)
        with (
            patch.object(transaction, "running_processes", return_value=["123"]),
            self.assertRaisesRegex(RuntimeError, "log out"),
        ):
            transaction.uninstall(
                self.prefix, self.prefix, system_root=self.system_root
            )
        self.assertTrue(entry.exists())

    def test_foreign_launcher_and_symlinked_parent_are_never_overwritten(self):
        launcher = self.prefix / layout.LAUNCHER
        launcher.parent.mkdir(parents=True)
        launcher.write_text("operator launcher")
        with self.assertRaisesRegex(RuntimeError, "unowned file"):
            self.install()
        self.assertEqual(launcher.read_text(), "operator launcher")
        launcher.unlink()
        launcher.parent.rmdir()
        elsewhere = self.directory / "elsewhere"
        elsewhere.mkdir()
        launcher.parent.symlink_to(elsewhere)
        with self.assertRaisesRegex(RuntimeError, "symlink"):
            self.install()
        self.assertEqual(list(elsewhere.iterdir()), [])

    def test_missing_changed_or_incomplete_bundles_never_activate(self):
        missing = self.binaries / "hearthspace-settings"
        missing.unlink()
        with self.assertRaisesRegex(RuntimeError, "missing"):
            self.bundle()
        missing.write_text("#!/bin/sh\nexit 0\n")
        missing.chmod(0o755)
        bundle = self.bundle()
        (bundle / "release/bin/hearthspace-settings").write_text("changed")
        with self.assertRaisesRegex(RuntimeError, "changed or missing"):
            transaction.install(
                bundle, self.prefix, self.prefix, system_root=self.system_root
            )
        self.assertFalse((self.prefix / layout.CURRENT).exists())
        bundle = self.bundle()
        spec = json.loads((bundle / "bundle.json").read_text())
        spec["files"].pop("bin/hearthspace-settings")
        (bundle / "bundle.json").write_text(json.dumps(spec))
        with self.assertRaisesRegex(RuntimeError, "complete runtime"):
            transaction.install(
                bundle, self.prefix, self.prefix, system_root=self.system_root
            )

    def test_prefix_validation_and_manifest_path_traversal_rejection(self):
        for path in (
            "relative",
            "/",
            "/usr/../etc",
            "/tmp/a:b",
            "/tmp/a%f",
            "/tmp/a\nb",
        ):
            with self.subTest(path=path), self.assertRaises(ValueError):
                layout.prefix_path(path)
        self.install()
        manifest = self.prefix / layout.MANIFEST
        data = json.loads(manifest.read_text())
        data["files"]["../../outside"] = {"sha256": "unused", "mode": 0o644}
        manifest.write_text(json.dumps(data))
        with self.assertRaisesRegex(RuntimeError, "unsafe"):
            transaction.uninstall(
                self.prefix, self.prefix, system_root=self.system_root
            )
        self.assertTrue((self.prefix / layout.LAUNCHER).exists())

    def test_bundle_missing_private_bus_helper_never_activates(self):
        bundle = self.bundle()
        spec = json.loads((bundle / "bundle.json").read_text())
        spec["files"].pop("bin/hearthspace-session-bus", None)
        (bundle / "bundle.json").write_text(json.dumps(spec))
        with self.assertRaisesRegex(RuntimeError, "complete runtime"):
            transaction.install(
                bundle, self.prefix, self.prefix, system_root=self.system_root
            )
        self.assertFalse((self.prefix / layout.CURRENT).exists())

    def test_restrictive_umask_does_not_make_system_installation_inaccessible(self):
        old_mask = os.umask(0o077)
        try:
            release = self.install()
        finally:
            os.umask(old_mask)
        for path in (
            self.prefix,
            self.prefix / "bin",
            self.prefix / layout.BASE,
            release.parent,
            release,
            release / "bin",
            self.prefix / layout.SESSION.parent,
        ):
            self.assertEqual(stat.S_IMODE(path.stat().st_mode), 0o755, str(path))

    def test_build_is_unprivileged_and_precedes_elevation(self):
        with (
            patch.object(layout.os, "geteuid", return_value=0),
            self.assertRaisesRegex(RuntimeError, "normal user"),
        ):
            layout.build("release")
        with (
            patch.object(layout.os, "geteuid", return_value=1000),
            patch.object(layout.subprocess, "run") as run,
        ):
            layout.build("release")
        command = run.call_args.args[0]
        self.assertEqual(command[:3], ["cargo", "build", "--locked"])
        self.assertIn("hearthspace-compositor/udev", command)
        self.assertIn("waydriver-hearthspace", command)
        self.assertIn("hearthspace-session", command)
        self.assertIn("hearthspace-lock", command)
        self.assertIn("--bins", command)
        self.assertNotIn("hearthspace-session-bus", command)
        self.assertIn("--release", command)
        self.assertEqual(
            command[command.index("--target-dir") + 1],
            str(layout.REPOSITORY / "target"),
        )
        events = []
        args = cli.parser().parse_args(
            ["install", "--destdir", str(self.directory / "stage")]
        )
        with (
            patch.object(layout, "build", side_effect=lambda _: events.append("build")),
            patch.object(
                layout,
                "bundle",
                side_effect=lambda *_: events.append("bundle") or {"release": "a" * 64},
            ),
            patch.object(
                cli, "privileged", side_effect=lambda *_: events.append("apply")
            ),
        ):
            cli.run(args)
        self.assertEqual(events, ["build", "bundle", "apply"])


if __name__ == "__main__":
    unittest.main()
