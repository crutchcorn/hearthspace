"""Sound/Brightness dropdown and OSD acceptance with private fake pactl routing."""

import os
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib.control_ui import run
from agentlib.control_ui_fixture import services
from shell_headless import run_shell_session


@unittest.skipUnless(os.environ.get("HEARTHSPACE_E2E") == "1", "set HEARTHSPACE_E2E=1")
class ControlUiHeadlessTests(unittest.TestCase):
    def test_volume_slider_media_keys_and_passive_osd(self):
        with tempfile.TemporaryDirectory(prefix="hs-control-ui-") as temporary:
            directory = Path(temporary)
            root = os.environ.get("HEARTHSPACE_SHELL_EVIDENCE")
            evidence = Path(root) / "audio-and-brightness" if root else directory
            evidence.mkdir(parents=True, exist_ok=True)
            run_shell_session(
                self,
                directory,
                evidence,
                run,
                "HEARTHSPACE_CONTROL_FIXTURE",
                services=services,
            )


if __name__ == "__main__":
    unittest.main()
