"""The declared runtime set must actually be built before native ownership."""

import hashlib
import json
import os
import subprocess
import sys
import tempfile
import tomllib
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import runtime_artifacts as artifacts
from installlib import layout
from nativelib import build, cli, common


class RuntimeArtifactTests(unittest.TestCase):
    def test_inventory_names_existing_package_binary_targets_without_duplicates(self):
        packages = []
        binaries = []
        for item in artifacts.INVENTORY:
            package = item["package"]
            directory = common.ROOT / "crates" / package
            manifest = tomllib.loads((directory / "Cargo.toml").read_text())
            self.assertEqual(manifest["package"]["name"], package)
            targets = {target["name"] for target in manifest.get("bin", [])}
            if (directory / "src/main.rs").is_file():
                targets.add(package)
            targets.update(path.stem for path in (directory / "src/bin").glob("*.rs"))
            for binary in item["binaries"]:
                self.assertIn(binary, targets)
                binaries.append(binary)
            for feature in item["native_features"]:
                self.assertIn(feature, manifest.get("features", {}))
            packages.append(package)
        self.assertEqual(len(packages), len(set(packages)))
        self.assertEqual(len(binaries), len(set(binaries)))
        self.assertEqual(layout.BINARIES, common.BINARIES)
        self.assertNotIn("hearthspace-ui-storybook", layout.BINARIES)

    def test_isolated_native_builds_prepare_every_required_package_and_helper_bin(self):
        commands = build.development_commands()
        built = [command[command.index("-p") + 1] for command in commands]
        self.assertCountEqual(built, artifacts.PACKAGES)
        for command in commands:
            self.assertEqual(command.count("-p"), 1)
            self.assertIn("--bins", command)
            self.assertIn("--locked", command)
            self.assertEqual(
                "--features" in command, "hearthspace-compositor" in command
            )
        self.assertIn("hearthspace-app-scope-guard", artifacts.BINARIES)
        self.assertIn("hearthspace-session-bus", artifacts.BINARIES)

    def test_failed_artifact_build_cannot_reach_privileged_vt_acquisition(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            args = cli.parser().parse_args(
                ["start", "--session", str(root / "run"), "--dev"]
            )
            with (
                patch.object(cli, "preflight"),
                patch.object(cli, "privileged") as privileged,
                patch.object(
                    build.subprocess,
                    "run",
                    return_value=subprocess.CompletedProcess([], 42),
                ),
                self.assertRaisesRegex(RuntimeError, "native build failed"),
            ):
                cli.start(args)
            privileged.assert_not_called()

    def test_pre_vt_build_refreshes_stale_helper_before_recording_provenance(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binaries = root / "target/debug"
            binaries.mkdir(parents=True)
            picker = root / "scripts/runtime/hearthspace-share-picker"
            picker.parent.mkdir(parents=True)
            picker.write_text("#!/usr/bin/python3\n")
            picker.chmod(0o755)
            session = root / "run"
            session.mkdir()
            for name in artifacts.BINARIES:
                binary = binaries / name
                binary.write_text("stale")
                binary.chmod(0o700)
            commands = root / "commands"
            commands.mkdir()
            cargo = commands / "cargo"
            cargo.write_text(
                f"#!{sys.executable}\nimport pathlib, sys\n"
                "root = pathlib.Path.cwd()\n"
                "assert '--bins' in sys.argv\n"
                "assert sys.argv.count('-p') == 1\n"
                "if sys.argv[sys.argv.index('-p') + 1] == 'hearthspace-session':\n"
                " (root / 'target/debug/hearthspace-app-scope-guard').write_text('fresh helper')\n"
            )
            cargo.chmod(0o700)
            args = cli.parser().parse_args(
                ["start", "--session", str(session), "--dev"]
            )
            args.binary_dir = binaries
            with (
                patch.object(build, "ROOT", root),
                patch.dict(os.environ, {"PATH": str(commands)}),
            ):
                build.build(args, session)
            provenance = json.loads((session / "binaries.json").read_text())
            self.assertEqual(
                provenance["hearthspace-app-scope-guard"]["sha256"],
                hashlib.sha256(b"fresh helper").hexdigest(),
            )
