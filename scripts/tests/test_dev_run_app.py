"""Development app launchers preserve arguments used for Settings deep links."""

import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).resolve().parents[1] / "dev-run-app"


class DevRunAppTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="hearthspace dev launcher ")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.commands = self.root / "commands"
        self.commands.mkdir()
        (self.root / "target/debug").mkdir(parents=True)
        self.cargo_log = self.root / "cargo.json"
        self.executable(
            self.commands / "cargo",
            "import json, os, pathlib, sys, tomllib\n"
            "pathlib.Path(os.environ['CARGO_LOG']).write_text(json.dumps(sys.argv[1:]))\n"
            "binary = sys.argv[sys.argv.index('--bin') + 1]\n"
            "config = pathlib.Path('.cargo/config.toml')\n"
            "build = tomllib.loads(config.read_text()).get('build', {}) if config.exists() else {}\n"
            "target = pathlib.Path(os.environ.get('CARGO_TARGET_DIR', build.get('target-dir', 'target')))\n"
            "if build.get('target'): target = target / build['target']\n"
            "artifact = (target / 'debug' / binary).resolve()\n"
            "print(json.dumps({'reason':'compiler-artifact', 'target':{'name':'other', 'kind':['bin']}, 'executable':'/wrong/artifact'}))\n"
            "if not os.environ.get('SKIP_ARTIFACT'):\n"
            " print(json.dumps({'reason':'compiler-artifact', 'target':{'name':binary, 'kind':['bin']}, 'profile':{'test':False}, 'executable':str(artifact), 'fresh':True}))\n"
            "sys.exit(int(os.environ.get('CARGO_STATUS', '0')))\n",
        )
        self.executable(
            self.root / "target/debug/hearthspace-settings",
            "import json, os, sys\n"
            "print(json.dumps({'arguments': sys.argv[1:], 'cwd': os.getcwd()}))\n",
        )
        self.environment = {
            **os.environ,
            "HEARTHSPACE_REPO_ROOT": str(self.root),
            "CARGO_LOG": str(self.cargo_log),
            "PATH": f"{self.commands}:{os.environ['PATH']}",
        }
        self.environment.pop("CARGO_TARGET_DIR", None)

    def executable(self, path, code):
        path.write_text(f"#!{sys.executable}\n{code}")
        path.chmod(0o755)

    def launch(self, *arguments):
        return subprocess.run(
            [str(SCRIPT), "hearthspace-settings", *arguments],
            cwd=self.root,
            env=self.environment,
            capture_output=True,
            text=True,
            check=False,
            timeout=10,
        )

    def test_launch_forwards_literal_arguments_and_uses_the_project_directory(self):
        arguments = ["--page", "search", "two words", "$(touch unexpected)", "*", ""]
        result = self.launch(*arguments)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(
            json.loads(result.stdout),
            {"arguments": arguments, "cwd": str(self.root)},
        )
        self.assertEqual(
            json.loads(self.cargo_log.read_text()),
            [
                "build",
                "--locked",
                "--message-format=json-render-diagnostics",
                "-p",
                "hearthspace-settings",
                "--bin",
                "hearthspace-settings",
            ],
        )
        self.assertFalse((self.root / "unexpected").exists())

    def test_ordinary_launch_has_no_extra_arguments(self):
        result = self.launch()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout)["arguments"], [])

    def test_failed_build_does_not_start_the_app(self):
        self.environment["CARGO_STATUS"] = "42"
        result = self.launch("--page", "search")
        self.assertEqual(result.returncode, 42)
        self.assertEqual(result.stdout, "")

    def test_environment_target_directory_uses_cargos_reported_executable(self):
        target = self.root / "custom target"
        (target / "debug").mkdir(parents=True)
        self.executable(
            target / "debug/hearthspace-settings",
            "import json, sys\nprint(json.dumps({'custom': True, 'arguments': sys.argv[1:]}))\n",
        )
        self.environment["CARGO_TARGET_DIR"] = str(target)
        result = self.launch("--page", "search")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(
            json.loads(result.stdout),
            {"custom": True, "arguments": ["--page", "search"]},
        )
        self.assertNotIn("--target-dir", json.loads(self.cargo_log.read_text()))

    def test_cargo_config_target_directory_and_triple_do_not_use_stale_default(self):
        config = self.root / ".cargo"
        config.mkdir()
        (config / "config.toml").write_text(
            '[build]\ntarget-dir = "configured output"\ntarget = "test-triple"\n'
        )
        artifact = (
            self.root / "configured output/test-triple/debug/hearthspace-settings"
        )
        artifact.parent.mkdir(parents=True)
        self.executable(artifact, "print('configured artifact')\n")
        result = self.launch()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, "configured artifact\n")

    def test_success_without_artifact_does_not_fall_back_to_stale_binary(self):
        self.environment["SKIP_ARTIFACT"] = "1"
        result = self.launch()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("did not report one executable", result.stderr)
        self.assertEqual(result.stdout, "")

    def test_missing_reported_artifact_does_not_fall_back_to_stale_binary(self):
        self.environment["CARGO_TARGET_DIR"] = str(self.root / "missing target")
        result = self.launch()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("unavailable executable", result.stderr)
        self.assertEqual(result.stdout, "")

    def test_development_storybook_remains_launchable(self):
        self.executable(
            self.root / "target/debug/hearthspace-ui-storybook",
            "print('storybook')\n",
        )
        result = subprocess.run(
            [str(SCRIPT), "hearthspace-ui-storybook"],
            check=False,
            cwd=self.root,
            env=self.environment,
            capture_output=True,
            text=True,
            timeout=10,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, "storybook\n")


if __name__ == "__main__":
    unittest.main()
