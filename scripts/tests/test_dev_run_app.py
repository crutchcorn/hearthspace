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
            "import json, os, pathlib, sys\n"
            "pathlib.Path(os.environ['CARGO_LOG']).write_text(json.dumps(sys.argv[1:]))\n"
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
            ["build", "-p", "hearthspace-settings"],
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


if __name__ == "__main__":
    unittest.main()
