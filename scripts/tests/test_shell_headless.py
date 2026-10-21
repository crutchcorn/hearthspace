"""Exercise the compact shell through real input on an isolated headless display."""

import os
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib.shell_presentation import run as presentation
from agentlib.shell_smoke import run
from shell_headless import run_shell_session


@unittest.skipUnless(os.environ.get("HEARTHSPACE_E2E") == "1", "set HEARTHSPACE_E2E=1")
class ShellHeadlessTests(unittest.TestCase):
    def test_compact_popups_on_smaller_output(self):
        with tempfile.TemporaryDirectory(prefix="hs-shell-small-") as temporary:
            directory = Path(temporary)
            root = os.environ.get("HEARTHSPACE_SHELL_EVIDENCE")
            evidence = Path(root) / "small-output" if root else directory
            evidence.mkdir(parents=True, exist_ok=True)
            run_shell_session(
                self,
                directory,
                evidence,
                presentation,
                "HEARTHSPACE_SHELL_SMALL",
                size=(800, 600),
                scale=1,
            )

    def test_icons_meta_search_notifications_and_power_cancel(self):
        with tempfile.TemporaryDirectory(prefix="hs-shell-e2e-") as temporary:
            directory = Path(temporary)
            evidence = Path(os.environ.get("HEARTHSPACE_SHELL_EVIDENCE", directory))
            evidence.mkdir(parents=True, exist_ok=True)
            run_shell_session(
                self,
                directory,
                evidence,
                run,
                "HEARTHSPACE_SHELL_FIXTURE",
                # Includes startup, independent frame-control navigation, and
                # every popup's pixel proof before the search/app workflows.
                timeout=180,
            )


if __name__ == "__main__":
    unittest.main()
