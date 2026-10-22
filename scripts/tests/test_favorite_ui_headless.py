"""Real Settings assignment, persisted favorites and explicit search drawing."""

import os
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib.favorite_ui import APP_ID, APP_NAME, run, services
from shell_headless import run_shell_session


class FavoriteFixtureTests(unittest.TestCase):
    def test_fixture_isolates_favorite_and_catalog_writes(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            env = {
                "XDG_CONFIG_HOME": "/unrelated/config",
                "XDG_DATA_HOME": "/unrelated/data",
            }
            with services(directory, directory, env, None) as fixture:
                self.assertEqual(
                    fixture.favorites, directory / "config/hearthspace/favorites.json"
                )
                self.assertEqual(env["XDG_CONFIG_HOME"], str(directory / "config"))
                self.assertEqual(env["XDG_DATA_HOME"], str(directory / "data"))
                desktop = (directory / "data/applications" / APP_ID).read_text()
                self.assertIn(f"Name={APP_NAME}", desktop)
                self.assertIn("Exec=foot --title=HEARTHSPACE_FAVORITE_LAUNCH", desktop)
                self.assertFalse(fixture.favorites.exists())


@unittest.skipUnless(os.environ.get("HEARTHSPACE_E2E") == "1", "set HEARTHSPACE_E2E=1")
class FavoriteUiHeadlessTests(unittest.TestCase):
    def test_settings_assignment_and_tolerant_immediate_launch(self):
        with tempfile.TemporaryDirectory(prefix="hs-favorite-ui-") as temporary:
            directory = Path(temporary)
            root = os.environ.get("HEARTHSPACE_SHELL_EVIDENCE")
            evidence = Path(root) / "favorite-apps" if root else directory
            evidence.mkdir(parents=True, exist_ok=True)
            run_shell_session(
                self,
                directory,
                evidence,
                run,
                "HEARTHSPACE_FAVORITE_FIXTURE",
                services=services,
                # The complete software-rendered workflow takes about 129s,
                # including cold renderer startup and private AT-SPI queries.
                timeout=180,
            )


if __name__ == "__main__":
    unittest.main()
