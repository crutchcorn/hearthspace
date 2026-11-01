"""Containment gates with fake commands; real cgroup acceptance is separate."""

import fcntl
import importlib.util
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))
import guarded_cargo as guard
import guarded_cargo_evidence as evidence
import guarded_cargo_policy as policy

FIXTURE_PATH = Path(__file__).with_name("fixtures") / "guarded_cargo.py"
SPEC = importlib.util.spec_from_file_location("guarded_cargo_fixture", FIXTURE_PATH)
fixture = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(fixture)


@unittest.skipIf(os.getuid() == 0, "service fixtures run as a normal user")
class GuardedCargoFixture(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(
            prefix=".guarded-cargo-fixture-", dir=Path.home()
        )
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        for name in (
            "bin",
            "source",
            "template",
            "proc/self",
            "proc/pressure",
            "proc/sys/kernel/random",
            "proc-worker/self",
            "cgroup/app.slice",
            "hwmon/cpu",
            "hwmon/gpu",
        ):
            (self.root / name).mkdir(parents=True, exist_ok=True)
        runtime = self.root / "runtime" / str(os.getuid())
        runtime.mkdir(parents=True, mode=0o700)
        (self.root / "source/Cargo.toml").write_text("# fixture source\n")
        (self.root / "proc/self/cgroup").write_text("0::/agent.scope\n")
        (self.root / "proc/self/mountinfo").write_text(
            "1 0 8:1 / / rw - ext4 /dev/fixture rw\n"
        )
        (self.root / "proc/sys/kernel/random/boot_id").write_text("fixture-boot-id\n")
        (self.root / "proc/meminfo").write_text(
            "MemAvailable: 50000 kB\nSwapTotal: 8000 kB\nSwapFree: 8000 kB\n"
        )
        for kind in ("cpu", "memory", "io"):
            (self.root / "proc/pressure" / kind).write_text(
                "some avg10=0.00 avg60=0.00 avg300=0.00 total=0\n"
            )
        for name, value in fixture.VALUES.items():
            (self.root / "template" / name).write_text(value + "\n")
        (self.root / "cgroup/app.slice/cgroup.controllers").write_text(
            "cpu memory pids\n"
        )
        (self.root / "hwmon/cpu/name").write_text("k10temp\n")
        (self.root / "hwmon/cpu/temp1_label").write_text("Tctl\n")
        (self.root / "hwmon/cpu/temp1_input").write_text("45000\n")
        (self.root / "hwmon/gpu/name").write_text("amdgpu\n")
        (self.root / "hwmon/gpu/temp1_label").write_text("edge\n")
        (self.root / "hwmon/gpu/temp1_input").write_text("99000\n")
        for name in ("cargo", "systemctl", "systemd-run", "git"):
            path = self.root / "bin" / name
            path.write_text(
                f"#!{sys.executable}\nimport runpy\nnamespace = runpy.run_path({str(FIXTURE_PATH)!r})\nnamespace['main']()\n"
            )
            path.chmod(0o700)
        self.enterContext(patch.object(policy, "CGROUP", self.root / "cgroup"))
        self.enterContext(patch.object(policy, "PROC", self.root / "proc"))
        self.enterContext(patch.object(policy, "HWMON", self.root / "hwmon"))
        self.enterContext(patch.object(policy, "RUNTIME", self.root / "runtime"))
        self.environment = {
            "HOME": str(self.root),
            "PATH": str(self.root / "bin"),
            "CARGO_TARGET_DIR": "unchanged target",
            "CARGO_PROFILE_RELEASE_DEBUG": "2",
            "RUSTFLAGS": "-C debuginfo=2",
            "RUSTUP_HOME": "/unchanged/rustup",
            "CARGO_HOME": "/unchanged/cargo",
            "TMPDIR": "/unchanged/tmp",
            "UNRELATED_CALLER_SECRET": "must-not-reach-cargo",
        }
        self.enterContext(patch.dict(os.environ, self.environment, clear=True))
        self.directory = self.root / "evidence"

    def run_guard(self, *arguments):
        return guard.main(["--evidence", str(self.directory), *arguments])

    def result(self):
        return json.loads((self.directory / "result.json").read_text())


class GuardedCargoSmokeTests(GuardedCargoFixture):
    def test_success_executes_only_after_durable_monitor_ack_and_preserves_build_inputs(
        self,
    ):
        synced = []
        real_sync, real_link = os.fsync, os.link

        def sync(fd):
            synced.append(os.readlink(f"/proc/self/fd/{fd}"))
            real_sync(fd)

        def link(source, destination, **kwargs):
            if Path(destination).name == "monitor-ready.json":
                telemetry = synced.index(str(self.directory / "telemetry.jsonl"))
                self.assertIn(str(self.directory), synced[telemetry + 1 :])
                self.assertIn(str(self.directory / "build.log"), synced)
            return real_link(source, destination, **kwargs)

        with (
            patch.object(os, "fsync", side_effect=sync),
            patch.object(os, "link", side_effect=link),
        ):
            code = self.run_guard(
                "build", "--release", "--locked", "--target-dir", "literal $target%path"
            )
        self.assertEqual(code, 0, self.result())
        executed = json.loads((self.root / "cargo.json").read_text())
        self.assertEqual(
            executed["argv"],
            [
                "build",
                "-j1",
                "--release",
                "--locked",
                "--target-dir",
                "literal $target%path",
                *policy.WRAPPER_ARGUMENTS,
            ],
        )
        self.assertTrue(executed["lock_held"])
        self.assertFalse(executed["relay_has_lock"])
        self.assertTrue(executed["ready"])
        self.assertGreaterEqual(executed["samples_before_exec"], 2)
        for name in (
            "HOME",
            "PATH",
            "RUSTUP_HOME",
            "CARGO_HOME",
            "TMPDIR",
            "CARGO_TARGET_DIR",
            "CARGO_PROFILE_RELEASE_DEBUG",
            "RUSTFLAGS",
        ):
            self.assertEqual(executed["environment"][name], self.environment[name])
        self.assertNotIn("UNRELATED_CALLER_SECRET", executed["environment"])
        self.assertNotIn("UNRELATED_MANAGER_SECRET", executed["environment"])
        self.assertNotIn("NOTIFY_SOCKET", executed["environment"])
        self.assertEqual(
            json.loads((self.root / "exit.json").read_text())["returncode"], 0
        )
        command = json.loads((self.directory / "command.json").read_text())
        self.assertEqual(command["cpu_affinity"], min(os.sched_getaffinity(0)))
        self.assertEqual(executed["cpu_affinity"], [command["cpu_affinity"]])
        self.assertEqual(command["boot_id"], "fixture-boot-id")
        self.assertEqual(command["uname"]["release"], os.uname().release)
        self.assertEqual(command["argv"][0], str(self.root / "bin/cargo"))
        launch = json.loads((self.root / "launch.json").read_text())
        self.assertIn("--user", launch)
        self.assertIn("--property=RuntimeMaxSec=45min", launch)
        self.assertIn(f"--property=CPUAffinity={command['cpu_affinity']}", launch)
        self.assertNotIn("--scope", launch)
        self.assertNotIn("--slice-inherit", launch)
        self.assertIn(
            "fixture cargo completed", (self.directory / "build.log").read_text()
        )
        self.assertIn("source.json", self.result()["sha256"])
        self.assertIn("command.json", self.result()["sha256"])
        self.assertEqual(
            (self.directory / "result.sha256").read_text().split()[0],
            evidence.file_hash(self.directory / "result.json"),
        )
        self.assertTrue(
            json.loads((self.directory / "cleanup.json").read_text())["verified_empty"]
        )

    def test_missing_user_controller_rejects_before_service_submission(self):
        (self.root / "cgroup/app.slice/cgroup.controllers").write_text("cpu pids\n")
        self.assertEqual(self.run_guard("check"), 1)
        self.assertFalse((self.root / "launch.json").exists())
        self.assertIn("controllers", self.result()["error"])

    def test_missing_submitted_property_is_rejected_by_external_fixture(self):
        with patch.dict(policy.PROPERTIES):
            policy.PROPERTIES.pop("CPUQuota")
            self.assertEqual(self.run_guard("build"), 1)
        self.assertFalse((self.root / "cargo.json").exists())
        self.assertIn("systemd-run failed", self.result()["error"])

    def test_invalid_telemetry_never_acknowledges_worker(self):
        (self.root / "template/cpu.stat").write_text("usage_usec invalid\n")
        self.assertEqual(self.run_guard("build"), 1)
        self.assertFalse((self.root / "cargo.json").exists())
        self.assertFalse((self.directory / "monitor-heartbeat.json").exists())
        self.assertIn("telemetry cpu.stat is invalid", self.result()["error"])

    def assert_postexec_stop(self, action, error_text):
        (self.root / "post-exec").write_text(action)
        self.assertEqual(self.run_guard("build"), 1)
        self.assertTrue((self.root / "cargo.json").exists())
        # Recorded by wait(), proving the still-running workload was terminated.
        self.assertLess(
            json.loads((self.root / "exit.json").read_text())["returncode"], 0
        )
        self.assertIn(error_text, self.result()["error"])
        self.assertEqual(self.result()["cleanup"], "owned unit stopped")

    def test_postexec_heat_stops_running_workload(self):
        self.assert_postexec_stop("heat", "operational")

    def test_postexec_sensor_loss_stops_running_workload(self):
        self.assert_postexec_stop("sensor", "temp1_input")

    def test_postexec_telemetry_write_failure_stops_running_workload(self):
        self.assert_postexec_stop("telemetry", "telemetry.jsonl")

    def test_postexec_sigint_stops_running_workload(self):
        self.assert_postexec_stop("SIGINT", "interrupted")

    def test_postexec_sighup_stops_running_workload(self):
        self.assert_postexec_stop("SIGHUP", "interrupted")

    def test_absent_unit_is_only_clean_after_its_cgroup_is_gone(self):
        (self.root / "absent-unit").touch()
        self.assertEqual(self.run_guard("build"), 1)
        self.assertTrue((self.root / "exit.json").exists())
        cleanup = json.loads((self.directory / "cleanup.json").read_text())
        self.assertTrue(cleanup["cgroup_removed"])
        self.assertTrue(cleanup["verified_empty"])

    def test_cleanup_command_timeout_remains_failure(self):
        real_run = subprocess.run

        def run(argv, **kwargs):
            if "stop" in argv:
                raise subprocess.TimeoutExpired(argv, kwargs["timeout"])
            return real_run(argv, **kwargs)

        with patch.object(subprocess, "run", side_effect=run):
            self.assertEqual(self.run_guard("build"), 1)
        self.assertEqual(self.result()["outcome"], "cleanup-failed")
        self.assertIn("15s deadline", self.result()["cleanup_error"])
        self.assertNotIn("cleanup", self.result())

    def test_unenforced_inner_limit_never_executes_cargo(self):
        (self.root / "template/memory.oom.group").write_text("0\n")
        self.assertEqual(self.run_guard("build"), 1)
        self.assertFalse((self.root / "cargo.json").exists())
        self.assertIn(
            "memory.oom.group", (self.directory / "worker-error.json").read_text()
        )

    def test_unbounded_lifecycle_property_never_executes_cargo(self):
        fixture.write_json(
            self.root / "properties.json", {"RuntimeMaxUSec": "infinity"}
        )
        self.assertEqual(self.run_guard("build"), 1)
        self.assertFalse((self.root / "cargo.json").exists())
        self.assertIn(
            "RuntimeMaxUSec", (self.directory / "worker-error.json").read_text()
        )

    def test_unenforced_cpu_quota_never_executes_cargo(self):
        (self.root / "template/cpu.max").write_text("max 100000\n")
        self.assertEqual(self.run_guard("check"), 1)
        self.assertFalse((self.root / "cargo.json").exists())
        self.assertIn("CPU quota", (self.directory / "worker-error.json").read_text())

    @unittest.skipIf(len(os.sched_getaffinity(0)) < 2, "requires two permitted CPUs")
    def test_worker_actual_affinity_must_match_even_if_service_property_does(self):
        (self.root / "skip-affinity").touch()
        self.assertEqual(self.run_guard("build"), 1)
        self.assertFalse((self.root / "cargo.json").exists())
        self.assertIn(
            "CPU affinity", (self.directory / "worker-error.json").read_text()
        )

    def test_unenforced_affinity_property_never_executes_cargo(self):
        fixture.write_json(self.root / "properties.json", {"CPUAffinity": ""})
        self.assertEqual(self.run_guard("build"), 1)
        self.assertFalse((self.root / "cargo.json").exists())
        self.assertIn(
            "CPU affinity", (self.directory / "worker-error.json").read_text()
        )

    def test_previous_higher_cpu_quota_never_executes_cargo(self):
        (self.root / "template/cpu.max").write_text("200000 100000\n")
        self.assertEqual(self.run_guard("check"), 1)
        self.assertFalse((self.root / "cargo.json").exists())
        self.assertIn("CPU quota", (self.directory / "worker-error.json").read_text())

    def test_hot_cpu_rejects_preflight_without_submitting_service(self):
        (self.root / "hwmon/cpu/temp1_input").write_text("85000\n")
        self.assertEqual(self.run_guard("build"), 1)
        self.assertFalse((self.root / "launch.json").exists())
        self.assertIn("operational", self.result()["error"])

    def test_temperature_crossing_stops_only_owned_unit_before_ack(self):
        (self.root / "heat-on-launch").touch()
        self.assertEqual(self.run_guard("build"), 1)
        self.assertFalse((self.root / "cargo.json").exists())
        self.assertFalse((self.directory / "monitor-ready.json").exists())
        unit = json.loads((self.root / "state.json").read_text())["unit"]
        calls = [
            json.loads(line)
            for line in (self.root / "control.log").read_text().splitlines()
        ]
        stops = [call for call in calls if "stop" in call]
        self.assertEqual(stops, [["--user", "stop", unit]])
        self.assertEqual(self.result()["cleanup"], "owned unit stopped")

    def test_remaining_tasks_prevent_cleanup_success(self):
        (self.root / "keep-tasks").touch()
        self.assertEqual(self.run_guard("build"), 1)
        self.assertEqual(self.result()["outcome"], "cleanup-failed")
        self.assertNotIn("cleanup", self.result())
        self.assertIn("still contains tasks", self.result()["cleanup_error"])

    def test_remaining_job_prevents_cleanup_success(self):
        fixture.write_json(self.root / "properties.json", {"Job": "12"})
        self.assertEqual(self.run_guard("build"), 1)
        self.assertEqual(self.result()["outcome"], "cleanup-failed")
        self.assertIn("process, job, or active state", self.result()["cleanup_error"])

    def test_interrupt_after_submission_cleans_up_without_acknowledging_worker(self):
        (self.root / "interrupt-on-launch").touch()
        self.assertEqual(self.run_guard("build"), 1)
        self.assertFalse((self.root / "cargo.json").exists())
        self.assertFalse((self.directory / "monitor-ready.json").exists())
        self.assertIn("interrupted", self.result()["error"])
        self.assertEqual(self.result()["cleanup"], "owned unit stopped")

    def test_existing_lock_refuses_concurrent_service(self):
        with (
            self.root / "runtime" / str(os.getuid()) / "hearthspace-guarded-cargo.lock"
        ).open("w") as lock:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            self.assertEqual(self.run_guard("build"), 1)
        self.assertFalse((self.root / "launch.json").exists())

    def test_tmpfs_and_existing_evidence_are_rejected_without_launch(self):
        (self.root / "proc/self/mountinfo").write_text(
            "1 0 0:1 / / rw - tmpfs tmpfs rw\n"
        )
        self.assertEqual(self.run_guard("build"), 1)
        self.assertFalse(self.directory.exists())
        (self.root / "proc/self/mountinfo").write_text(
            "1 0 8:1 / / rw - ext4 /dev/fixture rw\n"
        )
        self.directory.mkdir()
        (self.directory / "prior-attempt").write_text("retain me")
        self.assertEqual(self.run_guard("build"), 1)
        self.assertEqual((self.directory / "prior-attempt").read_text(), "retain me")
        self.assertFalse((self.root / "launch.json").exists())

    def test_service_paths_with_expansion_characters_are_rejected(self):
        self.directory = self.root / "evidence%$"
        self.assertEqual(self.run_guard("build"), 1)
        self.assertFalse(self.directory.exists())


if __name__ == "__main__":
    unittest.main()
