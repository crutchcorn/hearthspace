"""Exercise the compact shell through real input on an isolated headless display."""

import os
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib.shell_smoke import run
from shell_headless import run_shell_session


@unittest.skipUnless(os.environ.get("HEARTHSPACE_E2E") == "1", "set HEARTHSPACE_E2E=1")
class ShellHeadlessTests(unittest.TestCase):
    def test_icons_meta_search_notifications_and_power_cancel(self):
        with tempfile.TemporaryDirectory(prefix="hs-shell-e2e-") as temporary:
            directory = Path(temporary)
            evidence = Path(os.environ.get("HEARTHSPACE_SHELL_EVIDENCE", directory))
            evidence.mkdir(parents=True, exist_ok=True)
            run_shell_session(
                self, directory, evidence, run, "HEARTHSPACE_SHELL_FIXTURE"
            )


if __name__ == "__main__":
    unittest.main()
