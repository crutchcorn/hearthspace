"""Exercise the real E2E entry point at its child-command boundary."""

import json
import os
import select
import signal
import subprocess
import sys
import tarfile
import tempfile
import time
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
ENTRY = ROOT / "scripts/test-e2e"


class E2eRunnerTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="e2e-runner-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.tools = self.root / "bin"
        self.tools.mkdir()
        self.evidence = self.root / "evidence"
        self.env = dict(os.environ) | {
            "PATH": f"{self.tools}:{os.environ.get('PATH', '')}",
            "E2E_FIXTURE_ROOT": str(self.root),
            "HEARTHSPACE_COMMAND_SOCKET": "",
            "HEARTHSPACE_BINARY": "/stale/hearthspace",
            "VK_DRIVER_FILES": "/hardware/icd.json",
        }
        for name in ("dbus-daemon", "gdbus", "foot", "wayvnc"):
            self.tool(name, "raise SystemExit(0)\n")
        self.tool("git", "print('a' * 40)\n")
        self.tool(
            "cargo",
            "import json,os,pathlib,socket,subprocess,sys,time\n"
            "root=pathlib.Path(os.environ['E2E_FIXTURE_ROOT'])\n"
            "if sys.argv[1]=='metadata':\n"
            " print(json.dumps({'target_directory':str(root/'target')}));raise SystemExit(0)\n"
            "with (root/'commands.jsonl').open('a') as log: log.write(json.dumps(sys.argv[1:])+'\\n')\n"
            "if (root/'hold').exists():\n"
            " child=subprocess.Popen([sys.executable,'-c','import time;time.sleep(60)'])\n"
            " (root/'child.pid').write_text(str(child.pid))\n"
            " time.sleep(60)\n"
            "runtime=pathlib.Path(os.environ['TMPDIR'])\n"
            "(runtime/'observed.txt').write_text('owned fixture evidence')\n"
            "s=socket.socket(socket.AF_UNIX);s.bind(str(runtime/'fixture.sock'))\n"
            "print('intentional build failure',flush=True)\n"
            "raise SystemExit(42)\n",
        )
        self.renderer = self.root / "software.json"
        self.renderer.write_text("{}")
        self.argv = [
            sys.executable,
            str(ENTRY),
            "--evidence",
            str(self.evidence),
            "--vulkan-icd",
            str(self.renderer),
            "--egl-vendor",
            str(self.renderer),
        ]

    def tool(self, name, body):
        path = self.tools / name
        path.write_text(f"#!{sys.executable}\n{body}")
        path.chmod(0o700)

    def result(self):
        return json.loads((self.evidence / "result.json").read_text())

    def test_failed_build_never_tests_stale_executables_and_keeps_evidence(self):
        p = subprocess.run(
            self.argv,
            env=self.env,
            capture_output=True,
            text=True,
            timeout=15,
            check=False,
        )
        self.assertEqual(p.returncode, 1, p.stderr)
        result = self.result()
        self.assertEqual(result["outcome"], "failed")
        self.assertEqual([c["name"] for c in result["checks"]], ["build"])
        self.assertEqual(result["checks"][0]["returncode"], 42)
        self.assertEqual(result["binary_dir"], str(self.root / "target/debug"))
        self.assertIn(
            "intentional build failure", (self.evidence / "build.log").read_text()
        )
        self.assertTrue(result["runtime_removed"])
        self.assertFalse(Path(result["runtime"]).exists())
        with tarfile.open(self.evidence / "fixtures.tar.gz") as archive:
            self.assertEqual(
                archive.extractfile("fixtures/observed.txt").read(),
                b"owned fixture evidence",
            )
            self.assertNotIn("fixtures/fixture.sock", archive.getnames())

    def test_no_build_still_rejects_missing_private_xwayland(self):
        directory = self.root / "target/debug"
        directory.mkdir(parents=True)
        compositor = directory / "hearthspace-compositor"
        compositor.write_text("fixture executable\n")
        compositor.chmod(0o700)
        p = subprocess.run(
            [*self.argv, "--no-build"],
            env=self.env,
            capture_output=True,
            text=True,
            timeout=15,
            check=False,
        )
        self.assertEqual(p.returncode, 1, p.stderr)
        self.assertIn("prepare-xwayland", self.result()["error"])
        self.assertEqual(self.result()["checks"], [])
        self.assertFalse((self.root / "commands.jsonl").exists())

    def test_interrupt_stops_owned_children_and_preserves_failure_record(self):
        (self.root / "hold").touch()
        p = subprocess.Popen(
            self.argv,
            env=self.env,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        self.addCleanup(lambda: p.kill() if p.poll() is None else None)
        deadline = time.monotonic() + 10
        pidfile = self.root / "child.pid"
        while not pidfile.exists():
            self.assertIsNone(p.poll(), "runner exited before starting the owned child")
            self.assertLess(time.monotonic(), deadline, "runner child never started")
            time.sleep(0.02)
        fd = os.pidfd_open(int(pidfile.read_text()))
        self.addCleanup(os.close, fd)
        p.send_signal(signal.SIGTERM)
        self.assertEqual(p.wait(timeout=15), 130)
        self.assertTrue(
            select.select([fd], [], [], 3)[0], "owned child survived interruption"
        )
        self.assertEqual(self.result()["outcome"], "interrupted")
        self.assertTrue(self.result()["runtime_removed"])
        self.assertTrue((self.evidence / "fixtures.tar.gz").exists())
