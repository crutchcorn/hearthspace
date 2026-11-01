"""Wrapper config routes, without executing Cargo or contacting shared caches."""

import contextlib
import json
import os
import subprocess
import sys
import unittest

import test_guarded_cargo as smoke

NAMES = (
    "RUSTC_WRAPPER",
    "RUSTC_WORKSPACE_WRAPPER",
    "CARGO_BUILD_RUSTC_WRAPPER",
    "CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER",
)


class GuardedCargoWrapperTests(smoke.GuardedCargoFixture):
    def setUp(self):
        super().setUp()
        self.enterContext(contextlib.chdir(self.root / "source"))
        os.environ["CARGO_HOME"] = str(self.root / "cargo-home")
        (self.root / "wrapper-probe").touch()

    def config(self, relative, content):
        path = self.root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content)
        return path

    def assert_suppressed(self, *arguments):
        self.assertEqual(self.run_guard(*arguments), 0, self.result())
        executed = json.loads((self.root / "cargo.json").read_text())
        for name in NAMES:
            self.assertEqual(executed["environment"][name], "")
        probe = json.loads((self.root / "wrapper-probe.json").read_text())
        self.assertEqual(probe["config"]["build"]["rustc-wrapper"], "")
        self.assertEqual(probe["config"]["build"]["rustc-workspace-wrapper"], "")
        for name in NAMES:
            self.assertEqual(
                probe["config"]["env"][name],
                {"value": "", "force": False, "relative": False},
            )
        return probe

    def test_environment_wrappers_are_suppressed_without_contacting_shared_process(
        self,
    ):
        os.environ.update(dict.fromkeys(NAMES, "external-cache-client"))
        os.environ["RUSTC"] = "unchanged-compiler"
        with subprocess.Popen(
            [sys.executable, "-I", "-c", "import sys; sys.stdin.buffer.read()"],
            stdin=subprocess.PIPE,
        ) as shared:
            try:
                self.assert_suppressed("build", "--release")
                self.assertIsNone(shared.poll())
                executed = json.loads((self.root / "cargo.json").read_text())
                self.assertEqual(executed["environment"]["RUSTC"], "unchanged-compiler")
                unit = json.loads((self.root / "state.json").read_text())["unit"]
                calls = [
                    json.loads(line)
                    for line in (self.root / "control.log").read_text().splitlines()
                ]
                self.assertEqual(
                    [call for call in calls if "stop" in call],
                    [["--user", "stop", unit]],
                )
            finally:
                shared.stdin.close()
                shared.wait(timeout=3)

    def test_ancestor_legacy_and_cargo_home_configuration_are_overridden(self):
        home = self.config(
            "cargo-home/config.toml",
            '[build]\nrustc-wrapper = "home-client"\nrustc-workspace-wrapper = "home-ws"\n',
        )
        ancestor = self.config(
            ".cargo/config", '[build]\nrustc-wrapper = "ancestor-client"\n'
        )
        ignored = self.config(
            ".cargo/config.toml", '[build]\nrustc-wrapper = "ignored-client"\n'
        )
        local = self.config(
            "source/.cargo/config.toml",
            '[build]\nrustc-workspace-wrapper = "local-client"\n'
            "[profile.release]\ncodegen-units = 4\n",
        )
        before = {path: path.read_bytes() for path in (home, ancestor, ignored, local)}
        probe = self.assert_suppressed("build", "--release")
        self.assertEqual(set(probe["loaded"]), {str(home), str(ancestor), str(local)})
        self.assertEqual(probe["config"]["profile"]["release"]["codegen-units"], 4)
        self.assertEqual(before, {path: path.read_bytes() for path in before})

    def test_cli_file_and_inline_overrides_cannot_restore_wrappers_or_forced_env(self):
        extra = self.config(
            "extra.toml",
            '[build]\nrustc-wrapper = "file-client"\nrustc-workspace-wrapper = "file-ws"\n'
            + "\n".join(
                f'[env.{name}]\nvalue = "forced-client"\nforce = true\nrelative = true'
                for name in NAMES
            ),
        )
        probe = self.assert_suppressed(
            "clippy",
            "--config",
            str(extra),
            '--config=build.rustc-wrapper="inline-client"',
            "--config",
            'build."rustc-workspace-wrapper"="inline-ws"',
            "--config=profile.release.codegen-units=3",
            '--config=build.rustflags=["-C", "debuginfo=2"]',
            "--",
            "-D",
            "warnings",
        )
        self.assertIn(str(extra), probe["loaded"])
        self.assertEqual(
            probe["effective"]["RUSTC_WORKSPACE_WRAPPER"], "fixture-clippy-driver"
        )
        self.assertEqual(probe["config"]["profile"]["release"]["codegen-units"], 3)
        self.assertEqual(probe["config"]["build"]["rustflags"], ["-C", "debuginfo=2"])
        executed = json.loads((self.root / "cargo.json").read_text())
        self.assertEqual(executed["argv"][-3:], ["--", "-D", "warnings"])

    def test_incompatible_wrapper_environment_config_types_fail_closed(self):
        self.config(
            "source/.cargo/config.toml", '[env]\nRUSTC_WRAPPER = "configured-client"\n'
        )
        self.assertEqual(self.run_guard("build"), 1)
        self.assertFalse((self.root / "cargo.json").exists())
        self.assertIn(
            "config type conflict", (self.directory / "build.log").read_text()
        )
        self.assertEqual(self.result()["cleanup"], "owned unit stopped")


if __name__ == "__main__":
    unittest.main()
