"""Run the native map workflow headlessly with a private accessibility bus."""

import os
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib.map_smoke import run
from shell_headless import run_shell_session


@unittest.skipUnless(os.environ.get("HEARTHSPACE_E2E") == "1", "set HEARTHSPACE_E2E=1")
class CanvasMapHeadlessTests(unittest.TestCase):
    def test_live_map_pointer_zoom_and_dismissal(self):
        with tempfile.TemporaryDirectory(prefix="hs-map-e2e-") as temporary:
            directory = Path(temporary)
            evidence = Path(os.environ.get("HEARTHSPACE_MAP_EVIDENCE", directory))
            evidence.mkdir(parents=True, exist_ok=True)
            run_shell_session(self, directory, evidence, run, "HEARTHSPACE_MAP_FIXTURE")


if __name__ == "__main__":
    unittest.main()
