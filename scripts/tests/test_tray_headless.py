"""Exercise an independent tray app against the real shell on a private display."""

import os
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib.tray_smoke import run
from shell_headless import run_shell_session


@unittest.skipUnless(os.environ.get("HEARTHSPACE_E2E") == "1", "set HEARTHSPACE_E2E=1")
class TrayHeadlessTests(unittest.TestCase):
    def test_app_icons_menus_and_owner_lifecycle(self):
        with tempfile.TemporaryDirectory(prefix="hs-tray-e2e-") as temporary:
            directory = Path(temporary)
            evidence = Path(os.environ.get("HEARTHSPACE_TRAY_EVIDENCE", directory))
            evidence.mkdir(parents=True, exist_ok=True)
            run_shell_session(
                self, directory, evidence, run, "HEARTHSPACE_TRAY_FIXTURE"
            )


if __name__ == "__main__":
    unittest.main()
