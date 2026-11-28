"""Full shell/compositor acceptance of widget creation, placement, and removal."""

import contextlib
import os
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib.widget_checks import empty_frame_recovery, keyboard_add, run
from shell_headless import run_shell_session


@contextlib.contextmanager
def services(directory, _evidence, env, _deadline):
    config, data, desktop = (directory / name for name in ("config", "data", "Desktop"))
    config.mkdir()
    desktop.mkdir()
    applications = data / "applications"
    applications.mkdir(parents=True)
    (config / "user-dirs.dirs").write_text(f'XDG_DESKTOP_DIR="{desktop}"\n')
    (applications / "widget-terminal.desktop").write_text(
        "[Desktop Entry]\nType=Application\nName=Widget Terminal\n"
        "Exec=foot --title=WIDGET_TERMINAL\n"
    )
    env.update(
        XDG_CONFIG_HOME=str(config),
        XDG_DATA_HOME=str(data),
        XDG_DATA_DIRS=str(data),
        HEARTHSPACE_DEV="0",
        HEARTHSPACE_LAYOUT_PATH=str(config / "widget-layout.json"),
    )
    yield None


@unittest.skipUnless(os.environ.get("HEARTHSPACE_E2E") == "1", "set HEARTHSPACE_E2E=1")
class WidgetsHeadlessTests(unittest.TestCase):
    def test_keyboard_add_on_a_compact_output(self):
        def workflow(smoke):
            empty_frame_recovery(smoke)
            keyboard_add(smoke)
            smoke.capture("compact-keyboard-widget")
            smoke.control.request("quit")

        with tempfile.TemporaryDirectory(prefix="hs-widget-keys-") as temporary:
            directory = Path(temporary)
            root = os.environ.get("HEARTHSPACE_SHELL_EVIDENCE")
            evidence = Path(root) / "widgets-compact" if root else directory
            evidence.mkdir(parents=True, exist_ok=True)
            run_shell_session(
                self,
                directory,
                evidence,
                workflow,
                "HEARTHSPACE_WIDGET_KEYS",
                size=(800, 600),
                scale=1,
                services=services,
            )

    def test_canvas_search_adds_independent_widgets_that_drag_to_the_frame_and_launch_apps(
        self,
    ):
        with tempfile.TemporaryDirectory(prefix="hs-widgets-") as temporary:
            directory = Path(temporary)
            root = os.environ.get("HEARTHSPACE_SHELL_EVIDENCE")
            evidence = Path(root) / "widgets" if root else directory
            evidence.mkdir(parents=True, exist_ok=True)
            run_shell_session(
                self,
                directory,
                evidence,
                run,
                "HEARTHSPACE_WIDGET_FIXTURE",
                size=(1280, 800),
                scale=1,
                services=services,
                timeout=180,
            )
