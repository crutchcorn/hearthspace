"""A developer's live session must not select test services or graphics devices."""

import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from e2e_environment import LAVAPIPE, MESA_EGL, headless_binary, headless_environment


class HeadlessEnvironmentTests(unittest.TestCase):
    def test_child_ignores_hostile_desktop_and_graphics_overrides(self):
        inherited = dict(os.environ) | {
            "HEARTHSPACE_COMMAND_SOCKET": "",
            "HEARTHSPACE_SETTINGS_SOCKET": "/live/settings.sock",
            "HEARTHSPACE_HOST_XDG_RUNTIME_DIR": "/live/runtime",
            "HEARTHSPACE_LAYOUT_PATH": "/live/layout.json",
            "WAYLAND_SOCKET": "42",
            "DISPLAY": ":99",
            "AT_SPI_BUS_ADDRESS": "unix:path=/live/a11y",
            "DBUS_SYSTEM_BUS_ADDRESS": "unix:path=/live/system-bus",
            "VK_DRIVER_FILES": "/hardware/icd.json",
            "__EGL_VENDOR_LIBRARY_FILENAMES": "/hardware/vendor.json",
            "GSK_RENDERER": "vulkan",
            "XWAYLAND_NO_GLAMOR": "0",
            "HEARTHSPACE_BINARY": "/build/debug/hearthspace",
        }
        before = dict(inherited)
        with tempfile.TemporaryDirectory() as temporary:
            runtime = Path(temporary)
            env = headless_environment(runtime, inherited=inherited)
            keys = [
                "HEARTHSPACE_SETTINGS_SOCKET",
                "HEARTHSPACE_COMMAND_SOCKET",
                "HEARTHSPACE_HOST_XDG_RUNTIME_DIR",
                "HEARTHSPACE_LAYOUT_PATH",
                "XDG_CONFIG_HOME",
                "XDG_DATA_HOME",
                "XDG_CACHE_HOME",
                "XDG_STATE_HOME",
                "WAYLAND_SOCKET",
                "DISPLAY",
                "AT_SPI_BUS_ADDRESS",
                "DBUS_SYSTEM_BUS_ADDRESS",
                "VK_DRIVER_FILES",
                "__EGL_VENDOR_LIBRARY_FILENAMES",
                "GSK_RENDERER",
                "XWAYLAND_NO_GLAMOR",
                "HEARTHSPACE_BINARY",
            ]
            script = "import json,os,sys;print(json.dumps({k:os.environ.get(k) for k in sys.argv[1:]}))"
            child = subprocess.run(
                [sys.executable, "-c", script, *keys],
                env=env,
                capture_output=True,
                text=True,
                check=True,
            )
            actual = json.loads(child.stdout)
            self.assertIsNone(actual["HEARTHSPACE_COMMAND_SOCKET"])
            self.assertIsNone(actual["HEARTHSPACE_HOST_XDG_RUNTIME_DIR"])
            for key in ("WAYLAND_SOCKET", "DISPLAY", "AT_SPI_BUS_ADDRESS"):
                self.assertIsNone(actual[key])
            self.assertEqual(
                actual["HEARTHSPACE_SETTINGS_SOCKET"], str(runtime / "settings.sock")
            )
            self.assertEqual(
                actual["HEARTHSPACE_LAYOUT_PATH"], str(runtime / "layout.json")
            )
            for kind in ("CONFIG", "DATA", "CACHE", "STATE"):
                self.assertEqual(
                    actual[f"XDG_{kind}_HOME"], str(runtime / kind.lower())
                )
            self.assertNotIn("/live", actual["DBUS_SYSTEM_BUS_ADDRESS"])
            self.assertEqual(actual["VK_DRIVER_FILES"], str(LAVAPIPE))
            self.assertEqual(actual["__EGL_VENDOR_LIBRARY_FILENAMES"], str(MESA_EGL))
            self.assertEqual(actual["GSK_RENDERER"], "cairo")
            self.assertEqual(actual["XWAYLAND_NO_GLAMOR"], "1")
            self.assertEqual(actual["HEARTHSPACE_BINARY"], "/build/debug/hearthspace")
        self.assertEqual(inherited, before)

    def test_explicit_build_directory_selects_all_companions(self):
        with patch.dict(
            os.environ, {"HEARTHSPACE_BINARY": "/build/debug/selected-compositor"}
        ):
            self.assertEqual(
                headless_binary(), Path("/build/debug/selected-compositor")
            )
            self.assertEqual(
                headless_binary("hearthspace-shell"),
                Path("/build/debug/hearthspace-shell"),
            )
