"""Full shell battery/notification/menu acceptance with isolated power peers."""

import os
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib.power_ui import run
from agentlib.power_ui_fixture import services
from shell_headless import run_shell_session


@unittest.skipUnless(os.environ.get("HEARTHSPACE_E2E") == "1", "set HEARTHSPACE_E2E=1")
class PowerUiHeadlessTests(unittest.TestCase):
    def test_battery_notifications_and_battery_free_suspend(self):
        with tempfile.TemporaryDirectory(prefix="hs-power-ui-") as temporary:
            directory = Path(temporary)
            root = os.environ.get("HEARTHSPACE_SHELL_EVIDENCE")
            evidence = Path(root) / "power-controls" if root else directory
            evidence.mkdir(parents=True, exist_ok=True)
            run_shell_session(
                self,
                directory,
                evidence,
                run,
                "HEARTHSPACE_POWER_FIXTURE",
                services=services,
            )


if __name__ == "__main__":
    unittest.main()
