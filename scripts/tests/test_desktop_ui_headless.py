"""Real desktop selection, canvas movement, native focus and app activation."""

import contextlib
import json
import os
import sys
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib.desktop_checks import desktop as desktop_status
from agentlib.desktop_checks import require, run
from shell_headless import run_shell_session


@contextlib.contextmanager
def services(directory, _evidence, env, _deadline):
    config, data = directory / "config", directory / "data"
    desktop = directory / "Desktop"
    applications = data / "applications"
    applications.mkdir(parents=True)
    (data / "mime").symlink_to("/usr/share/mime", target_is_directory=True)
    config.mkdir()
    desktop.mkdir()
    for folder in ["Pictures", "Projects"]:
        (desktop / folder).mkdir()
    (desktop / "Notes.txt").write_text("Desktop file fixture\n")
    (config / "user-dirs.dirs").write_text(f'XDG_DESKTOP_DIR="{desktop}"\n')
    (config / "mimeapps.list").write_text(
        "[Default Applications]\ntext/plain=file-handler.desktop\ninode/directory=file-handler.desktop\n"
    )
    opened_paths = directory / "opened-paths.jsonl"
    handler = directory / "open-handler"
    handler.write_text(
        "#!/usr/bin/python3\nimport json, os, sys\n"
        f"with open({str(opened_paths)!r}, 'a') as log:\n"
        "    log.write(json.dumps(sys.argv[1:]) + '\\n')\n"
        "os.execvp('foot', ['foot', '--title=DESKTOP_FILE'])\n"
    )
    handler.chmod(0o700)
    (applications / "file-handler.desktop").write_text(
        "[Desktop Entry]\nType=Application\nName=File handler\nNoDisplay=true\n"
        f"Exec={handler} %f\nMimeType=text/plain;inode/directory;\n"
    )
    for app_id, name in [
        ("a-terminal.desktop", "Terminal"),
        ("b-editor.desktop", "Editor"),
    ]:
        (applications / app_id).write_text(
            "[Desktop Entry]\nType=Application\n"
            f"Name={name}\nExec=foot --title=DESKTOP_{name.upper()}\n"
            "Icon=/usr/share/icons/hicolor/scalable/apps/foot.svg\nOnlyShowIn=Hearthspace;\n"
        )
    env.update(
        XDG_CONFIG_HOME=str(config),
        XDG_DATA_HOME=str(data),
        XDG_DATA_DIRS=str(data),
        HEARTHSPACE_LAYOUT_PATH=str(config / "desktop-layout.json"),
        HEARTHSPACE_DEV="0",
    )
    yield SimpleNamespace(
        desktop=desktop,
        config=config,
        favorites=config / "hearthspace" / "favorites.json",
        opened_paths=opened_paths,
    )


@unittest.skipUnless(os.environ.get("HEARTHSPACE_E2E") == "1", "set HEARTHSPACE_E2E=1")
class DesktopUiHeadlessTests(unittest.TestCase):
    def test_shortcuts_select_move_open_and_survive_favoriting(self):
        with tempfile.TemporaryDirectory(prefix="hs-desktop-ui-") as temporary:
            directory = Path(temporary)
            root = os.environ.get("HEARTHSPACE_SHELL_EVIDENCE")
            evidence = Path(root) / "desktop-items" if root else directory
            evidence.mkdir(parents=True, exist_ok=True)
            run_shell_session(
                self,
                directory,
                evidence,
                run,
                "HEARTHSPACE_DESKTOP_FIXTURE",
                size=(1280, 800),
                scale=1,
                services=services,
            )
            results = json.loads((evidence / "artifacts/results.json").read_text())
            self.assertTrue(
                any(result["step"] == "desktop-verified" for result in results)
            )
            saved = (directory / "config/desktop-layout.json").read_text()
            positions = json.loads(saved)["desktop"]
            restarted = directory / "restarted"
            restarted.mkdir()
            restored_evidence = evidence / "restarted"
            restored_evidence.mkdir()

            @contextlib.contextmanager
            def restored_services(*args):
                with services(*args) as fixture:
                    (fixture.config / "desktop-layout.json").write_text(saved)
                    yield fixture

            def restored(smoke):
                smoke.until(
                    "desktop icons restored",
                    lambda: len(desktop_status(smoke)["items"]) == 5,
                )
                for entry in desktop_status(smoke)["items"]:
                    if entry["id"] in positions:
                        require(
                            {"x": entry["x"], "y": entry["y"]}
                            == positions[entry["id"]],
                            f"restart lost placement for {entry['id']}",
                        )
                smoke.capture("desktop-restored")

            run_shell_session(
                self,
                restarted,
                restored_evidence,
                restored,
                "HEARTHSPACE_DESKTOP_RESTORE_FIXTURE",
                size=(1280, 800),
                scale=1,
                services=restored_services,
            )


if __name__ == "__main__":
    unittest.main()
