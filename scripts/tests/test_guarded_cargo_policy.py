"""Focused policy invariants, including fail-closed heartbeat freshness."""

import json
import os
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import guarded_cargo_evidence as evidence
import guarded_cargo_policy as policy
import guarded_cargo_watchdog as watchdog


class GuardedCargoPolicyTests(unittest.TestCase):
    def test_root_is_rejected(self):
        with (
            patch.object(os, "geteuid", return_value=0),
            self.assertRaisesRegex(RuntimeError, "normal user"),
        ):
            policy.normal_user()

    def test_execution_commands_and_contradictory_jobs_are_rejected(self):
        for arguments in (
            ["test"],
            ["run"],
            ["install"],
            ["build", "-j2"],
            ["check", "--jobs", "8"],
            ["build", "--jobs=0"],
            ["build", "-j8"],
            ["build", "-qj8"],
            ["build", "-qj8pserde_json"],
            ["build", "-pserde_json", "-j8"],
            ["check", "-Fjson", "-qj8"],
        ):
            with self.subTest(arguments=arguments), self.assertRaises(ValueError):
                policy.cargo_arguments(arguments)
        with self.assertRaisesRegex(ValueError, "CARGO_BUILD_JOBS"):
            policy.build_environment(
                {"HOME": "/home/fixture", "PATH": "/bin", "CARGO_BUILD_JOBS": "8"}
            )

    def test_one_job_and_clippy_arguments_preserve_the_command(self):
        self.assertEqual(
            policy.cargo_arguments(["clippy", "--jobs=1", "--", "-D", "warnings"]),
            ["clippy", "-j1", *policy.WRAPPER_ARGUMENTS, "--", "-D", "warnings"],
        )

    def test_attached_package_and_feature_values_are_not_jobs_options(self):
        for options in (
            ["-pserde_json", "-Fjson"],
            ["-p=serde_json", "-F=json"],
            ["-vpserde_json", "-vFjson"],
            ["-pj8", "-Fj8"],
            ["-p", "serde_json", "-F", "json"],
        ):
            with self.subTest(options=options):
                self.assertEqual(
                    policy.cargo_arguments(["build", *options]),
                    ["build", "-j1", *options, *policy.WRAPPER_ARGUMENTS],
                )

    def test_global_short_option_values_are_left_for_cargo_to_validate(self):
        # -C placement and nightly -Z availability remain Cargo's responsibility.
        options = ["-Zunstable-options", "-C/path/to/project"]
        self.assertEqual(
            policy.cargo_arguments(["build", *options]),
            ["build", "-j1", *options, *policy.WRAPPER_ARGUMENTS],
        )

    def test_clippy_attached_values_preserve_the_trailing_compiler_arguments(self):
        self.assertEqual(
            policy.cargo_arguments(
                ["clippy", "-pserde_json", "-Fjson", "--", "-D", "warnings", "-j8"]
            ),
            [
                "clippy",
                "-j1",
                "-pserde_json",
                "-Fjson",
                *policy.WRAPPER_ARGUMENTS,
                "--",
                "-D",
                "warnings",
                "-j8",
            ],
        )

    def test_cleanup_observes_descendant_processes_even_with_empty_parent(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "cgroup.events").write_text("populated 0\n")
            (root / "cgroup.procs").write_text("")
            (root / "child").mkdir()
            (root / "child/cgroup.procs").write_text("456\n")
            self.assertEqual(
                policy.empty_cgroup(root)["remaining_tasks"],
                {"child/cgroup.procs": ["456"]},
            )

    def test_disk_backed_tmp_and_var_tmp_are_rejected_before_creation(self):
        with tempfile.TemporaryDirectory() as directory:
            proc = Path(directory) / "proc"
            (proc / "self").mkdir(parents=True)
            (proc / "self/mountinfo").write_text(
                "1 0 8:1 / / rw - ext4 /dev/fixture rw\n"
            )
            with patch.object(policy, "PROC", proc):
                for parent in (Path(directory), Path("/var/tmp")):
                    path = parent / (Path(directory).name + "-evidence")
                    with (
                        self.subTest(path=path),
                        self.assertRaisesRegex(RuntimeError, "temporary directories"),
                    ):
                        evidence.fresh_directory(path)
                    self.assertFalse(path.exists())

    def test_symlink_into_tmp_and_configured_temp_directory_are_rejected(self):
        with tempfile.TemporaryDirectory(dir=Path.home()) as directory:
            root = Path(directory)
            (root / "temp-link").symlink_to("/tmp", target_is_directory=True)
            with self.assertRaisesRegex(RuntimeError, "temporary directories"):
                evidence.fresh_directory(root / "temp-link/new-evidence")
            (root / "configured-temp").mkdir()
            with (
                patch.dict(os.environ, {"TMPDIR": str(root / "configured-temp")}),
                self.assertRaisesRegex(RuntimeError, "temporary directories"),
            ):
                evidence.fresh_directory(root / "configured-temp/new-evidence")

    def test_heartbeat_requires_new_recent_owner_matched_monotonic_sample(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "heartbeat.json"
            now = 10_000_000_000
            self.assertIsNone(watchdog.fresh_timestamp(path, "owner", 0, now))
            for record in (
                {"owner": "other", "sample_monotonic_ns": now},
                {
                    "owner": "owner",
                    "sample_monotonic_ns": now - watchdog.MAX_AGE_NS - 1,
                },
                {"owner": "owner", "sample_monotonic_ns": now + 1},
                {"owner": "owner", "sample_monotonic_ns": "10000000000"},
                {"owner": "owner"},
                [],
            ):
                path.write_text(json.dumps(record))
                with self.subTest(record=record):
                    self.assertIsNone(watchdog.fresh_timestamp(path, "owner", 0, now))
            path.write_text("invalid JSON")
            self.assertIsNone(watchdog.fresh_timestamp(path, "owner", 0, now))
            path.write_text(json.dumps({"owner": "owner", "sample_monotonic_ns": now}))
            self.assertEqual(watchdog.fresh_timestamp(path, "owner", 0, now), now)
            self.assertIsNone(watchdog.fresh_timestamp(path, "owner", now, now + 1))


if __name__ == "__main__":
    unittest.main()
