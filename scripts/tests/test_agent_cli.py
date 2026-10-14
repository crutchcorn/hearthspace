import json
import sys
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib.cli import run_launcher


class LauncherWrapperTests(unittest.TestCase):
    def test_start_preserves_separate_bounded_build_and_startup_budgets(self):
        for no_build, budget in ((False, 1850), (True, 50)):
            args = SimpleNamespace(
                session=Path("/tmp/test-session"),
                command="start",
                timeout=20,
                watchdog=300,
                no_build=no_build,
                log_level="info",
            )
            with (
                self.subTest(no_build=no_build),
                patch("agentlib.cli.subprocess.run") as run,
            ):
                run.return_value = SimpleNamespace(
                    returncode=0, stdout=json.dumps({"ok": True})
                )
                self.assertTrue(run_launcher(args)["ok"])
                self.assertEqual(run.call_args.kwargs["timeout"], budget)


if __name__ == "__main__":
    unittest.main()
