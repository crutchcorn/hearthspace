"""Explicit preparation is pinned, private, serial and separate from startup."""

import hashlib
import io
import json
import os
import sys
import tarfile
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import prepare_xwayland as prepare
import xwayland_build as build
import xwayland_stage as staging
from installlib import xwayland
from xwayland_fixture import companions
from xwayland_provenance import BUILD_CONFIGURATION, SOURCE_INPUTS, verify_prepared


class PrepareXwaylandTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.repository = self.root / "repository"
        (self.repository / "data").mkdir(parents=True)
        (self.repository / "patches").mkdir()
        self.notice = b"Preserved upstream notice\n"
        self.patch = self.repository / "patches/xwayland-surface-input-targets.patch"
        self.patch.write_bytes(b"test patch; not executed\n")
        (self.repository / "patches/xwayland-COPYING").write_bytes(self.notice)
        self.archive = self.root / "upstream.tar.xz"
        with tarfile.open(self.archive, "w:xz") as archive:
            for name in (*SOURCE_INPUTS, "COPYING"):
                content = self.notice if name == "COPYING" else b"upstream source\n"
                entry = tarfile.TarInfo(f"xwayland-24.1.10/{name}")
                entry.size = len(content)
                archive.addfile(entry, io.BytesIO(content))
        self.requirements = {
            "format_version": 1,
            "version": "24.1.10",
            "source_archive_sha256": hashlib.sha256(
                self.archive.read_bytes()
            ).hexdigest(),
            "surface_input_targets_sha256": hashlib.sha256(
                self.patch.read_bytes()
            ).hexdigest(),
        }
        (self.repository / "data/xwayland-requirements.json").write_text(
            json.dumps(self.requirements)
        )
        self.compositor = self.root / "custom-target/profile/hearthspace-compositor"
        self.compositor.parent.mkdir(parents=True)
        self.compositor.write_bytes(b"prebuilt compositor")
        self.compositor.chmod(0o755)
        self.args = SimpleNamespace(
            archive=self.archive,
            build_dir=self.root / "fresh",
            compositor=self.compositor,
        )
        self.enterContext(patch.object(prepare, "REPOSITORY", self.repository))
        self.enterContext(patch.object(xwayland, "REPOSITORY", self.repository))
        self.enterContext(patch.object(prepare.os, "geteuid", return_value=1000))
        self.enterContext(patch.object(prepare, "available_tools", return_value={}))

    def fake_compile(self, root, source, patch_path, tools):
        self.assertEqual(patch_path.read_bytes(), self.patch.read_bytes())
        self.assertEqual(stat_mode(root), 0o700)
        (source / "hw/xwayland/xwayland-input.c").write_bytes(b"patched source")
        for name in (*BUILD_CONFIGURATION, "include/xorg-config.h"):
            path = root / "build" / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(b"build input")
        (root / "compiler-inputs.log").write_text(
            f"object.o: deps 1\n    {source / 'include/inpututils.h'}\n"
        )
        binary = root / "build/hw/xwayland/Xwayland"
        binary.parent.mkdir(parents=True)
        binary.write_bytes(b"built pinned Xwayland; never executed")
        binary.chmod(0o755)
        return binary

    def test_preparation_stages_beside_explicit_compositor_with_exact_receipt(self):
        with patch.object(prepare, "compile_source", side_effect=self.fake_compile):
            result = prepare.prepare(self.args)
        self.assertEqual(
            result["binary"], str(self.compositor.with_name(xwayland.BINARY))
        )
        self.assertEqual(
            result["build_receipt"], str(self.args.build_dir / "build-receipt.json")
        )
        receipt = json.loads(Path(result["receipt"]).read_text())
        self.assertEqual(set(receipt), {*xwayland.FIELDS, "sha256"})
        self.assertEqual(
            receipt["sha256"],
            hashlib.sha256(Path(result["binary"]).read_bytes()).hexdigest(),
        )
        self.assertEqual(stat_mode(Path(result["binary"])), 0o755)
        self.assertEqual(stat_mode(Path(result["receipt"])), 0o644)
        self.assertEqual(
            (self.args.build_dir / "Xwayland-COPYING").read_bytes(), self.notice
        )
        verify_prepared(
            Path(result["binary"]),
            Path(result["source"]),
            Path(result["compile_commands"]),
        )

    def test_published_pair_has_exact_modes_under_restrictive_umask(self):
        old_mask = os.umask(0o077)
        try:
            with patch.object(prepare, "compile_source", side_effect=self.fake_compile):
                result = prepare.prepare(self.args)
        finally:
            os.umask(old_mask)
        records = xwayland.verify_companions(self.compositor)
        self.assertEqual(records[xwayland.BINARY]["mode"], 0o755)
        self.assertEqual(records[xwayland.RECEIPT]["mode"], 0o644)
        self.assertEqual(stat_mode(Path(result["binary"])), 0o755)
        self.assertEqual(stat_mode(Path(result["receipt"])), 0o644)

    def test_wrong_archive_or_patch_digest_fails_before_creating_build_directory(self):
        for path in (self.archive, self.patch):
            with self.subTest(path=path):
                original = path.read_bytes()
                path.write_bytes(b"changed input")
                with (
                    patch.object(prepare, "compile_source") as compile_source,
                    self.assertRaisesRegex(RuntimeError, "trusted requirements"),
                ):
                    prepare.prepare(self.args)
                compile_source.assert_not_called()
                self.assertFalse(self.args.build_dir.exists())
                path.write_bytes(original)

    def test_root_existing_build_and_symlink_destinations_are_rejected(self):
        with (
            patch.object(prepare.os, "geteuid", return_value=0),
            self.assertRaisesRegex(RuntimeError, "normal user"),
        ):
            prepare.prepare(self.args)
        self.args.build_dir.mkdir()
        with self.assertRaisesRegex(RuntimeError, "fresh"):
            prepare.prepare(self.args)
        self.args.build_dir.rmdir()
        self.compositor.with_name(xwayland.BINARY).symlink_to(self.compositor)
        with self.assertRaisesRegex(RuntimeError, "symlink"):
            prepare.prepare(self.args)
        self.assertFalse(self.args.build_dir.exists())

    def test_compile_failure_keeps_prior_companions_and_preserves_build_evidence(self):
        binary = companions(self.compositor.parent)
        original = binary.read_bytes()
        with (
            patch.object(
                prepare, "compile_source", side_effect=RuntimeError("compiler failed")
            ),
            self.assertRaisesRegex(RuntimeError, "compiler failed"),
        ):
            prepare.prepare(self.args)
        self.assertEqual(binary.read_bytes(), original)
        self.assertTrue((self.args.build_dir / "surface-input-targets.patch").is_file())
        xwayland.verify_companions(self.compositor)

    def test_mismatched_notice_aborts_without_compiling(self):
        (self.repository / "patches/xwayland-COPYING").write_bytes(b"wrong notice")
        with (
            patch.object(prepare, "compile_source") as compile_source,
            self.assertRaisesRegex(RuntimeError, "COPYING"),
        ):
            prepare.prepare(self.args)
        compile_source.assert_not_called()

    def test_missing_tools_are_actionable_without_starting_processes(self):
        with (
            patch.object(build.shutil, "which", return_value=None),
            self.assertRaisesRegex(
                RuntimeError, "missing Xwayland preparation tools.*meson.*ninja"
            ),
        ):
            build.available_tools()

    def test_cli_requires_explicit_paths_and_rejects_relative_compositor(self):
        with self.assertRaises(SystemExit):
            prepare.parser().parse_args(
                [
                    "--archive",
                    str(self.archive),
                    "--build-dir",
                    str(self.args.build_dir),
                    "--compositor",
                    "target/debug/hearthspace-compositor",
                ]
            )

    def test_atomic_staging_failure_restores_previous_binary_and_receipt(self):
        binary = companions(self.compositor.parent)
        receipt_path = binary.with_name(xwayland.RECEIPT)
        original_binary = binary.read_bytes()
        original_receipt = receipt_path.read_bytes()
        built = self.root / "built"
        built.write_bytes(b"replacement build")
        built.chmod(0o755)
        receipt = {
            **self.requirements,
            "sha256": hashlib.sha256(built.read_bytes()).hexdigest(),
        }
        original_replace = staging.os.replace
        failed = False

        def replace(source, target):
            nonlocal failed
            if target == receipt_path and not failed:
                failed = True
                raise OSError("injected receipt replacement failure")
            return original_replace(source, target)

        with (
            patch.object(staging.os, "replace", side_effect=replace),
            self.assertRaisesRegex(OSError, "injected receipt"),
        ):
            staging.stage(built, self.compositor, receipt)
        self.assertEqual(binary.read_bytes(), original_binary)
        self.assertEqual(receipt_path.read_bytes(), original_receipt)
        xwayland.verify_companions(self.compositor)


def stat_mode(path):
    return path.stat().st_mode & 0o777


class ExtractXwaylandTests(unittest.TestCase):
    def test_unsafe_archive_members_are_rejected_before_any_extraction(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name, kind in (
                ("../escape", tarfile.REGTYPE),
                ("/absolute", tarfile.REGTYPE),
                ("xwayland-24.1.10/link", tarfile.SYMTYPE),
                ("xwayland-24.1.10/hardlink", tarfile.LNKTYPE),
                ("xwayland-24.1.10/fifo", tarfile.FIFOTYPE),
            ):
                with self.subTest(name=name):
                    archive_path = root / "bad.tar.xz"
                    with tarfile.open(archive_path, "w:xz") as archive:
                        safe = tarfile.TarInfo("xwayland-24.1.10/safe")
                        archive.addfile(safe, io.BytesIO())
                        bad = tarfile.TarInfo(name)
                        bad.type = kind
                        archive.addfile(bad)
                    with self.assertRaisesRegex(RuntimeError, "unsafe"):
                        build.extract(archive_path, root / "source", "24.1.10")
                    self.assertFalse((root / "source").exists())

    def test_build_commands_match_tested_configuration_and_use_one_job(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "source"
            source.mkdir()
            tools = {name: f"/usr/bin/{name}" for name in build.TOOLS}
            with (
                patch.dict(os.environ, {"CFLAGS": "unsafe", "LD_PRELOAD": "unsafe"}),
                patch.object(build, "run") as run,
            ):
                build.compile_source(root, source, root / "patch", tools)
            commands = [call.args[0] for call in run.call_args_list]
            self.assertEqual(len(commands), 4)
            self.assertIn("--fuzz=0", commands[0])
            self.assertIn("--buildtype=debug", commands[1])
            self.assertIn("--wrap-mode=nofallback", commands[1])
            self.assertIn("-Dsha1=libmd", commands[1])
            self.assertIn("-j1", commands[2])
            self.assertIn("-l1", commands[2])
            self.assertNotIn("install", commands[2])
            for call in run.call_args_list:
                self.assertNotIn("CFLAGS", call.kwargs["env"])
                self.assertNotIn("LD_PRELOAD", call.kwargs["env"])
                self.assertEqual(
                    call.kwargs["deadline"], run.call_args.kwargs["deadline"]
                )
