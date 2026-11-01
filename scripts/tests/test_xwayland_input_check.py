"""The maintained runner must never accept skipped or mismatched input checks."""

import json
import sys
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from xwayland_checklib import runner
from xwayland_checklib.process import fresh_output, invoke


def accepted_axis():
    return {
        "suite": "extended",
        "expectation": "pass",
        "accepted": True,
        "enabled_oracle_passed": True,
        "cases": 56,
        "total_cases": 98,
        "legacy_compared": True,
        "legacy_captured": False,
        "enabled_failures": [],
        "shape_failures": [],
        "legacy_failures": [],
        "initial_suite": {
            "expectation": "pass",
            "cases": 42,
            "positive_oracle_failing_cases": [],
            "legacy_compared": True,
        },
    }


class XwaylandInputRunnerTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.out = Path(self.directory.name)
        self.args = SimpleNamespace(
            xwayland=self.out / "hearthspace-Xwayland",
            source=self.out / "source",
            compile_commands=self.out / "build/compile_commands.json",
        )
        self.prepared = {"sha256": "a" * 64}

    def stages(self, axis_result):
        # These are the build/process boundaries; never launch a fixture from
        # Python unit tests. The real baseline validation remains in the path.
        verify = self.enterContext(
            patch.object(runner, "verify_prepared", return_value=self.prepared)
        )
        build = self.enterContext(
            patch.object(runner, "build_relative", return_value=self.out / "fixture")
        )
        relative = self.enterContext(
            patch.object(runner, "run_relative", return_value={"accepted": True})
        )
        axis = self.enterContext(
            patch.object(runner.axis, "main", return_value=axis_result)
        )
        return verify, build, relative, axis

    def test_bad_prepared_inputs_never_build_or_run(self):
        verify, build, relative, axis = self.stages(accepted_axis())
        verify.side_effect = RuntimeError("prepared source inventory mismatch")
        with self.assertRaisesRegex(RuntimeError, "inventory mismatch"):
            runner.run_checks(self.args, self.out)
        build.assert_not_called()
        relative.assert_not_called()
        axis.assert_not_called()
        self.assertFalse((self.out / "result.json").exists())

    def test_success_runs_both_suites_and_rechecks_provenance(self):
        verify, build, relative, axis = self.stages(accepted_axis())
        result = runner.run_checks(self.args, self.out)
        self.assertTrue(result["accepted"])
        build.assert_called_once_with(self.out)
        relative.assert_called_once_with(
            self.out / "fixture", self.args.xwayland, self.out
        )
        axis.assert_called_once_with(
            [
                "--source",
                str(self.args.source),
                "--compile-commands",
                str(self.args.compile_commands),
                "--output",
                str(self.out / "axis"),
                "--suite",
                "extended",
                "--expect",
                "pass",
            ]
        )
        self.assertEqual(verify.call_count, 2)
        self.assertEqual(json.loads((self.out / "result.json").read_text()), result)

    def test_incomplete_axis_success_marker_is_rejected(self):
        self.stages({"accepted": True, "total_cases": 98})
        with self.assertRaisesRegex(RuntimeError, "incomplete, skipped, or rejected"):
            runner.run_checks(self.args, self.out)
        self.assertFalse((self.out / "result.json").exists())

    def test_provenance_change_after_checks_prevents_acceptance(self):
        verify, _, _, _ = self.stages(accepted_axis())
        verify.side_effect = [self.prepared, {"sha256": "b" * 64}]
        with self.assertRaisesRegex(RuntimeError, "changed during regression checks"):
            runner.run_checks(self.args, self.out)
        self.assertFalse((self.out / "result.json").exists())

    def test_bounded_child_receives_limits_and_private_environment(self):
        code = (
            "import json, os, resource; "
            "print(json.dumps({'limits': [resource.getrlimit(kind) for kind in "
            "(resource.RLIMIT_CORE, resource.RLIMIT_CPU, resource.RLIMIT_AS, "
            "resource.RLIMIT_FSIZE, resource.RLIMIT_NOFILE)], "
            "'home': os.environ['HOME'], 'display': os.environ.get('DISPLAY')}))"
        )
        with patch.dict("os.environ", {"DISPLAY": ":private-must-not-leak"}):
            invoke(
                [sys.executable, "-B", "-c", code],
                self.out,
                "limits",
                timeout=5,
                cpu=2,
                address_space=512 * 1024**2,
                file_size=1024**2,
            )
        evidence = json.loads((self.out / "limits.stdout").read_text())
        self.assertEqual(
            evidence["limits"],
            [[0, 0], [2, 3], [512 * 1024**2] * 2, [1024**2] * 2, [128, 128]],
        )
        self.assertEqual(evidence["home"], str(self.out / "tmp"))
        self.assertIsNone(evidence["display"])

    def test_output_must_be_fresh(self):
        output = self.out / "evidence"
        self.assertEqual(fresh_output(output), output)
        with self.assertRaises(FileExistsError):
            fresh_output(output)


if __name__ == "__main__":
    unittest.main()
