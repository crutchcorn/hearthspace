"""Sound/Brightness dropdown and OSD acceptance with private fake pactl routing."""

import contextlib
import os
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib.control_ui import run
from agentlib.control_ui_fixture import services
from shell_headless import ROOT, run_shell_session


@contextlib.contextmanager
def isolated_services(directory, evidence, env, deadline):
    # The private bus is already running with system schema lookup intact.
    # Only these clients need a catalog independent of installed host apps.
    env["XDG_DATA_DIRS"] = str(ROOT / "data")
    with services(directory, evidence, env, deadline) as fixture:
        yield fixture


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
                services=isolated_services,
            )


if __name__ == "__main__":
    unittest.main()
