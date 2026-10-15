import sys
import unittest
from pathlib import Path
from unittest.mock import Mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib.cli import action, parser


class OverviewCliTests(unittest.TestCase):
    def test_overview_modes_and_selection_send_precise_commands(self):
        for argv, wire in [
            (["overview"], "overview-toggle"),
            (["overview", "on"], "overview-on"),
            (["overview", "off"], "overview-off"),
            (["overview-select", "42"], "overview-select 42"),
        ]:
            with self.subTest(argv=argv):
                args = parser().parse_args(
                    ["--socket", "/private/control", *argv, "--wait", "none"]
                )
                control = Mock()
                result = action(args, control)
                self.assertTrue(result["accepted"])
                control.request.assert_called_once_with(wire)
                control.capture.assert_not_called()


if __name__ == "__main__":
    unittest.main()
