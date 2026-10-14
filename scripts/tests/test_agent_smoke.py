import contextlib
import io
import sys
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib.control import ControlError
from agentlib.smoke import Smoke


class SmokeTests(unittest.TestCase):
    def test_recovery_failure_does_not_hide_original_interaction_error(self):
        with tempfile.TemporaryDirectory() as directory:
            session = Path(directory)
            (session / "session.json").write_text("{}")
            smoke = Smoke(SimpleNamespace(session=session, timeout=10, case="success"))

            def launcher(operation, *_):
                if operation == "stop":
                    raise ControlError("cleanup failed")

            smoke.launcher = launcher
            smoke.success = lambda: (_ for _ in ()).throw(ControlError("typing failed"))
            with (
                patch("agentlib.smoke.shutil.which", return_value="/usr/bin/foot"),
                contextlib.redirect_stdout(io.StringIO()),
                self.assertRaisesRegex(ControlError, "typing failed"),
            ):
                smoke.run()
            failures = [
                result for result in smoke.results if result["step"].endswith("failed")
            ]
            self.assertEqual(
                [result["error"] for result in failures],
                ["typing failed", "cleanup failed"],
            )

    def test_unmapped_toplevel_is_not_ready_for_input(self):
        with tempfile.TemporaryDirectory() as directory:
            smoke = Smoke(SimpleNamespace(session=Path(directory)))
            smoke.window = lambda: {"height": 30, "titlebar_height": 30}
            self.assertIsNone(smoke.mapped_terminal())
            smoke.window = lambda: {"height": 524, "titlebar_height": 30}
            self.assertIsNotNone(smoke.mapped_terminal())


if __name__ == "__main__":
    unittest.main()
