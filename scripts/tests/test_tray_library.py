"""CI requires deployed tray libraries; local acceptance records missing ones."""

import os
import sys
import unittest
from pathlib import Path
from unittest.mock import Mock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib import tray_library
from agentlib.control import ControlError


class TrayLibraryRequirementTests(unittest.TestCase):
    def test_missing_optional_libraries_are_recorded_without_launching(self):
        for required in (None, "0"):
            smoke = Mock()
            environment = (
                {}
                if required is None
                else {"HEARTHSPACE_REQUIRE_TRAY_LIBRARY": required}
            )
            with (
                self.subTest(required=required),
                patch.dict(os.environ, environment, clear=True),
                patch.object(
                    tray_library.ctypes.util, "find_library", return_value=None
                ),
                patch.object(tray_library.subprocess, "Popen") as launch,
            ):
                tray_library.run(smoke)
                smoke.record.assert_called_once_with(
                    "tray-library-unavailable",
                    missing=["gtk-3", "ayatana-appindicator3"],
                )
                launch.assert_not_called()

    def test_required_libraries_fail_with_all_missing_names(self):
        smoke = Mock()
        with (
            patch.dict(os.environ, {"HEARTHSPACE_REQUIRE_TRAY_LIBRARY": "1"}),
            patch.object(tray_library.ctypes.util, "find_library", return_value=None),
            patch.object(tray_library.subprocess, "Popen") as launch,
            self.assertRaisesRegex(
                ControlError,
                "HEARTHSPACE_REQUIRE_TRAY_LIBRARY=1.*gtk-3, ayatana-appindicator3",
            ),
        ):
            tray_library.run(smoke)
        smoke.record.assert_called_once_with(
            "tray-library-unavailable", missing=["gtk-3", "ayatana-appindicator3"]
        )
        launch.assert_not_called()

    def test_required_libraries_report_only_the_missing_dependency(self):
        for missing in ("gtk-3", "ayatana-appindicator3"):
            smoke = Mock()
            with (
                self.subTest(missing=missing),
                patch.dict(os.environ, {"HEARTHSPACE_REQUIRE_TRAY_LIBRARY": "1"}),
                patch.object(
                    tray_library.ctypes.util,
                    "find_library",
                    side_effect=lambda name, missing=missing: (
                        None if name == missing else f"lib{name}.so"
                    ),
                ),
                self.assertRaisesRegex(
                    ControlError, f"missing shared libraries: {missing}\\."
                ),
            ):
                tray_library.run(smoke)
            smoke.record.assert_called_once_with(
                "tray-library-unavailable", missing=[missing]
            )


if __name__ == "__main__":
    unittest.main()
