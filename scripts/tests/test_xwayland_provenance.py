"""Build receipts bind the compiler/source view to a verified private binary."""

import json
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import xwayland_provenance as provenance
from xwayland_fixture import companions


class XwaylandProvenanceTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.source = self.root / "source/xwayland-24.1.10"
        self.build = self.root / "build"
        for name in (*provenance.SOURCE_INPUTS, "COPYING", "extra/source.c"):
            path = self.source / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(f"source: {name}\n")
        for name in (
            *provenance.BUILD_CONFIGURATION,
            "include/xorg-config.h",
            "generated.c",
        ):
            path = self.build / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(f"build: {name}\n")
        self.commands = self.build / "compile_commands.json"
        self.header = self.root / "system/header.h"
        self.header.parent.mkdir()
        self.header.write_text("compiler header\n")
        self.binary = companions(self.root)
        self.receipt = self.root / "build-receipt.json"
        data = provenance.build_receipt(
            self.binary,
            self.source,
            self.commands,
            f"object.o: deps 1\n    {self.header}\n",
        )
        self.receipt.write_text(json.dumps(data))

    def verify(self):
        return provenance.verify_prepared(self.binary, self.source, self.commands)

    def test_valid_build_identity_is_verified_without_running_any_program(self):
        with patch("subprocess.Popen") as launch:
            result = self.verify()
        launch.assert_not_called()
        self.assertEqual(result["source_directory"], str(self.source))
        self.assertEqual(result["compile_commands"]["path"], str(self.commands))

    def test_changed_any_source_or_added_source_is_detected(self):
        path = self.source / "extra/source.c"
        original = path.read_bytes()
        path.write_bytes(b"new source")
        with self.assertRaisesRegex(RuntimeError, "source inventory"):
            self.verify()
        path.write_bytes(original)
        (self.source / "extra/new.c").write_bytes(b"unrecorded")
        with self.assertRaisesRegex(RuntimeError, "source inventory"):
            self.verify()

    def test_removed_required_axis_input_is_detected(self):
        (self.source / "dix/inpututils.c").unlink()
        with self.assertRaisesRegex(RuntimeError, "missing dix/inpututils.c"):
            self.verify()

    def test_changed_commands_generated_headers_and_system_headers_are_detected(self):
        for path, message in (
            (self.commands, "compile_commands"),
            (self.build / "include/xorg-config.h", "build inventory"),
            (self.header, "compiler input"),
        ):
            with self.subTest(path=path):
                original = path.read_bytes()
                path.write_bytes(b"changed input")
                with self.assertRaisesRegex(RuntimeError, message):
                    self.verify()
                path.write_bytes(original)

    def test_retargeted_include_directory_is_rejected_even_for_identical_headers(self):
        first = self.root / "external/v1"
        second = self.root / "external/v2"
        first.mkdir(parents=True)
        second.mkdir()
        content = b"unchanged header bytes\n"
        (first / "header.h").write_bytes(content)
        (second / "header.h").write_bytes(content)
        current = self.root / "external/current"
        current.symlink_to(first, target_is_directory=True)
        dependency = current / "header.h"
        data = provenance.build_receipt(
            self.binary,
            self.source,
            self.commands,
            f"object.o: deps 1\n    {dependency}\n",
        )
        self.receipt.write_text(json.dumps(data))
        record = self.verify()["compiler_inputs"][str(dependency)]
        self.assertEqual(record["resolved_path"], str(first / "header.h"))
        current.unlink()
        current.symlink_to(second, target_is_directory=True)
        self.assertEqual((first / "header.h").read_bytes(), content)
        with self.assertRaisesRegex(RuntimeError, "compiler input resolution"):
            self.verify()

    def test_original_dependency_aliases_remain_separate_inventory_entries(self):
        alias = self.header.with_name("alias.h")
        alias.symlink_to(self.header)
        dependencies = (
            f"object.o: deps 3\n    {self.header}\n    {alias}\n    {self.header}\n"
        )
        inventory = provenance.compiler_inventory(dependencies, self.build)
        self.assertEqual(set(inventory), {str(self.header), str(alias)})
        self.assertEqual(inventory[str(alias)], inventory[str(self.header)])

    def test_old_target_only_inventory_is_rejected(self):
        data = json.loads(self.receipt.read_text())
        record = data["compiler_inputs"][str(self.header)]
        data["compiler_inputs"][str(self.header)] = record["sha256"]
        self.receipt.write_text(json.dumps(data))
        with self.assertRaisesRegex(RuntimeError, "invalid compiler input record"):
            self.verify()

    def test_new_valid_companion_cannot_borrow_another_binarys_source_receipt(self):
        companions(self.root, b"different but valid companion")
        with self.assertRaisesRegex(RuntimeError, "does not match Xwayland binary"):
            self.verify()

    def test_receipt_cannot_change_pinned_requirements_or_selected_source_path(self):
        original = self.receipt.read_bytes()
        for key, value, message in (
            ("version", "24.1.11", "trusted requirements"),
            ("source_directory", str(self.root), "source directory"),
            ("compiler_inputs", {}, "no compiler input"),
        ):
            with self.subTest(key=key):
                data = json.loads(original)
                data[key] = value
                self.receipt.write_text(json.dumps(data))
                with self.assertRaisesRegex(RuntimeError, message):
                    self.verify()

    def test_symlinked_inputs_and_nonobject_receipt_are_rejected(self):
        source = self.source / "extra/source.c"
        source.unlink()
        source.symlink_to(self.header)
        with self.assertRaisesRegex(RuntimeError, "symlink"):
            self.verify()
        self.receipt.write_text("[]")
        with self.assertRaisesRegex(RuntimeError, "JSON object"):
            self.verify()
