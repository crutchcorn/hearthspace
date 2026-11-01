"""Explicit temperature observation retains containment and telemetry gates."""

import json

from test_guarded_cargo import GuardedCargoFixture


class GuardedCargoTemperatureTests(GuardedCargoFixture):
    def observe(self):
        return self.run_guard("--temperature-observation-only", "build")

    def assert_hot_success(self):
        self.assertEqual(self.observe(), 0, self.result())
        self.assertTrue((self.root / "cargo.json").exists())
        state = json.loads((self.directory / "command.json").read_text())
        self.assertTrue(state["temperature_observation_only"])
        self.assertTrue(self.result()["temperature_observation_only"])
        self.assertIn("cutoff disabled", state["temperature_policy"])
        self.assertEqual(state["properties"]["CPUQuota"], "50%")
        self.assertEqual(state["properties"]["MemoryMax"], "16G")
        self.assertEqual(state["properties"]["WatchdogSec"], "10")
        samples = [
            json.loads(line)
            for line in (self.directory / "telemetry.jsonl").read_text().splitlines()
        ]
        self.assertTrue(
            any(max(row["cpu_tctl_millidegrees"].values()) >= 85000 for row in samples)
        )
        self.assertTrue(
            json.loads((self.directory / "cleanup.json").read_text())["verified_empty"]
        )

    def test_hot_preflight_is_recorded_and_build_executes(self):
        (self.root / "hwmon/cpu/temp1_input").write_text("95000\n")
        self.assert_hot_success()

    def test_temperature_crossing_after_launch_does_not_block_acknowledgment(self):
        (self.root / "heat-on-launch").touch()
        self.assert_hot_success()

    def test_missing_sensor_still_rejects_preflight(self):
        (self.root / "hwmon/cpu/temp1_input").unlink()
        self.assertEqual(self.observe(), 1)
        self.assertFalse((self.root / "launch.json").exists())
        self.assertTrue(self.result()["temperature_observation_only"])
        self.assertIn("temp1_input", self.result()["error"])

    def test_unenforced_quota_still_prevents_execution(self):
        (self.root / "template/cpu.max").write_text("max 100000\n")
        self.assertEqual(self.observe(), 1)
        self.assertFalse((self.root / "cargo.json").exists())
        self.assertIn("CPU quota", (self.directory / "worker-error.json").read_text())

    def test_invalid_runtime_telemetry_still_prevents_acknowledgment(self):
        (self.root / "template/cpu.stat").write_text("usage_usec invalid\n")
        self.assertEqual(self.observe(), 1)
        self.assertFalse((self.root / "cargo.json").exists())
        self.assertFalse((self.directory / "monitor-heartbeat.json").exists())
        self.assertIn("telemetry cpu.stat is invalid", self.result()["error"])
