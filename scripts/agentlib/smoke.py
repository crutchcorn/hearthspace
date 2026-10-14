"""Native acceptance run, with artifacts preserved even when assertions fail."""

import argparse
import json
import math
import os
import shlex
import shutil
import signal
import subprocess
import sys
import time
from pathlib import Path

from .control import Control, ControlError, Deadline, save_capture
from .input import Input
from .png import crop, green_pixels
from .startup_failure import verify_startup_failure
from .visual import move_resize, software_cursor

LAUNCHER = Path(__file__).resolve().parents[1] / "hearthspace-native"


class Smoke:
    def __init__(self, args, *, presentation="page-flip"):
        self.args = args
        self.directory = args.session.resolve()
        self.artifacts = self.directory / "artifacts"
        self.artifacts.mkdir(parents=True, exist_ok=True)
        self.results = []
        self.control = None
        self.presentation = presentation

    def record(self, step, **result):
        entry = {"step": step, **result}
        self.results.append(entry)
        (self.artifacts / "results.json").write_text(
            json.dumps(self.results, indent=2) + "\n"
        )
        print(json.dumps(entry), flush=True)

    def launcher(self, operation, *extra, expected=0):
        command = [
            str(LAUNCHER),
            operation,
            "--session",
            str(self.directory),
            "--timeout",
            str(self.args.timeout),
            *extra,
        ]
        if operation == "start":
            command.extend(["--log-level", getattr(self.args, "log_level", "info")])
        result = subprocess.run(
            command,
            check=False,
            capture_output=True,
            text=True,
            timeout=self.args.timeout + 30,
        )
        self.record(
            operation,
            argv=command,
            returncode=result.returncode,
            stdout=result.stdout,
            stderr=result.stderr,
        )
        if expected is not None and result.returncode != expected:
            raise ControlError(f"launcher {operation} exited {result.returncode}")
        return result

    def connect(self):
        manifest = json.loads((self.directory / "session.json").read_text())
        self.control = Control(manifest["command_socket"], Deadline(self.args.timeout))
        return manifest

    def until(self, name, condition):
        while True:
            value = condition()
            if value:
                return value
            try:
                time.sleep(min(0.05, self.control.deadline.remaining()))
            except ControlError as error:
                raise ControlError(f"timed out waiting for {name}") from error

    def capture(self, name, output=None):
        capture = self.control.capture(output)
        if capture[0].get("presentation") != self.presentation:
            raise ControlError(f"smoke requires {self.presentation} captures")
        self.record(name, **save_capture(capture, self.artifacts / f"{name}.png"))
        return capture

    def window(self, window_id=None):
        windows = self.control.status()["windows"]
        return next(
            (
                window
                for window in windows
                if (
                    window["id"] == window_id
                    if window_id is not None
                    else "foot" in window.get("app_id", "").lower()
                )
            ),
            None,
        )

    def success(self):
        manifest = self.connect()
        status = self.control.status()
        self.record("ready", status=status, manifest=manifest)
        if not status["shell_ready"]:
            raise ControlError("shell did not become ready")
        for index, output in enumerate(status["outputs"]):
            capture = self.capture(f"output-{index}", output["name"])
            if capture[0]["output"] != output["name"]:
                raise ControlError("selected capture returned another output")
        initial = self.capture("initial")
        self.terminal(initial)

    def terminal(self, initial):
        self.control.request("spawn foot")
        window = self.until("mapped foot window", self.mapped_terminal)
        self.capture("terminal-ready", initial[0]["output"])
        window = self.window(window["id"])
        if window is None:
            raise ControlError("foot exited before its first captured frame")
        self.record("launch", command="spawn foot", window=window)
        marker = self.artifacts / "typed-marker.txt"
        text = (
            "printf '\\033]0;HEARTHSPACE_SMOKE\\007\\033[48;2;0;255;0m"
            "HEARTHSPACE_NATIVE_MARKER\\033[0m\\n'; "
            f"printf HEARTHSPACE_NATIVE_MARKER > {shlex.quote(str(marker))}\n"
        )
        with Input(self.control) as inputs:
            inputs.click(
                window["x"] + window["width"] / 2, window["y"] + window["height"] / 2
            )
            inputs.type(text)
        self.record("type", text=text)
        self.until(
            "terminal input",
            lambda: (
                marker.exists() and marker.read_text() == "HEARTHSPACE_NATIVE_MARKER"
            ),
        )

        def visible_marker():
            capture = self.control.capture(initial[0]["output"])
            return capture if green_pixels(capture[1]) >= 100 else None

        visible = self.until("visible green terminal marker", visible_marker)
        self.record(
            "typed-visible",
            green_pixels=green_pixels(visible[1]),
            **save_capture(visible, self.artifacts / "typed-visible.png"),
        )
        window = self.window(window["id"])
        resized = move_resize(self, window, visible, initial)
        software_cursor(self, resized, initial[0]["output"])
        self.cursor(initial[0])

    def mapped_terminal(self):
        window = self.window()
        return (
            window
            if window and window["height"] > window.get("titlebar_height", 30) + 20
            else None
        )

    def changed(self, before, fields):
        after = self.window(before["id"])
        return (
            after
            if after and any(abs(after[key] - before[key]) > 10 for key in fields)
            else None
        )

    def cursor(self, metadata):
        # Logical bounds are rounded; only the advertised scale maps pixels.
        scale = metadata["scale"]
        x, y = metadata["logical_x"] + 20, metadata["logical_y"] + 100
        with Input(self.control) as inputs:
            inputs.motion(x, y)
        before = self.capture("cursor-first", metadata["output"])
        with Input(self.control) as inputs:
            inputs.motion(x + 70, y)
        after = self.capture("cursor-second", metadata["output"])
        if after[0].get("cursor") != "embedded":
            raise ControlError("capture cursor policy must be embedded")
        rect = (10 * scale, 90 * scale, 55 * scale, 140 * scale)
        if crop(before[1], rect) == crop(after[1], rect):
            raise ControlError("moving cursor did not change the old cursor region")
        self.record("cursor-inclusion", policy="embedded", checked_pixel_region=rect)

    def recovery(self, case):
        manifest = self.connect()
        if case in ("compositor-failure", "watchdog"):
            pid = manifest.get("compositor_pid")
            if not pid:
                readiness = Path(manifest["runtime_dir"]) / "session-ready.json"
                if readiness.exists():
                    pid = json.loads(readiness.read_text()).get("compositor_pid")
            if not pid:
                raise ControlError(
                    "manifest needs compositor_pid for failure injection"
                )
            failure_signal = signal.SIGSTOP if case == "watchdog" else signal.SIGKILL
            os.kill(int(pid), failure_signal)
            self.record(
                "injected-compositor-failure", pid=pid, signal=failure_signal.name
            )
        deadline = Deadline(self.args.timeout + 15)
        while True:
            current = json.loads((self.directory / "session.json").read_text())
            if current.get("status") in ("stopped", "failed", "expired"):
                self.record("independent-recovery", manifest=current)
                return
            time.sleep(min(0.2, deadline.remaining()))

    def interrupted_agent(self):
        """Kill an agent that owns a completed start; recovery must be independent."""
        command = [
            str(LAUNCHER),
            "start",
            "--session",
            str(self.directory),
            "--timeout",
            str(self.args.timeout),
            "--no-build",
            "--watchdog",
            "12",
            "--log-level",
            getattr(self.args, "log_level", "info"),
        ]
        worker = "import subprocess,sys,signal; subprocess.run(sys.argv[1:],check=True); signal.pause()"
        with (self.artifacts / "interrupted-agent.log").open("w") as log:
            process = subprocess.Popen(
                [sys.executable, "-c", worker, *command],
                stdout=log,
                stderr=subprocess.STDOUT,
            )
            try:
                deadline = Deadline(self.args.timeout)
                while True:
                    manifest = self.directory / "session.json"
                    state = (
                        json.loads(manifest.read_text()) if manifest.exists() else {}
                    )
                    if state.get("status") == "ready":
                        break
                    if process.poll() is not None:
                        raise ControlError(
                            "simulated agent exited before native readiness"
                        )
                    time.sleep(min(0.05, deadline.remaining()))
                process.kill()
                process.wait(timeout=5)
                self.record(
                    "injected-agent-interruption", pid=process.pid, signal="SIGKILL"
                )
                self.recovery("agent-interruption")
            finally:
                if process.poll() is None:
                    process.kill()
                    process.wait(timeout=5)

    def run(self):
        if self.args.case == "success" and shutil.which("foot") is None:
            raise ControlError("native smoke requires foot")
        host = subprocess.run(
            ["uname", "-a"], capture_output=True, text=True, check=True
        )
        self.record("host", uname=host.stdout.strip(), case=self.args.case)
        if (
            self.args.case == "startup-failure"
            and (self.directory / "session.json").exists()
        ):
            raise ControlError("startup-failure requires a new session directory")
        error = None
        try:
            if self.args.case == "agent-interruption":
                self.interrupted_agent()
                return
            if self.args.case == "startup-failure":
                result = self.launcher(
                    "start", "--no-build", "--compositor", "/bin/false", expected=None
                )
                if result.returncode == 0:
                    raise ControlError("invalid compositor unexpectedly started")
                self.record(
                    "injected-startup-failure",
                    **verify_startup_failure(self.directory, "/bin/false"),
                )
                return
            watchdog = (
                "8"
                if self.args.case == "watchdog"
                else str(math.ceil(self.args.timeout + 90))
            )
            self.launcher("start", "--no-build", "--watchdog", watchdog)
            if self.args.case == "success":
                self.success()
            else:
                self.recovery(self.args.case)
        except BaseException as failure:
            error = failure
            self.record("operation-failed", error=str(failure))
            raise
        finally:
            if (self.directory / "session.json").exists():
                try:
                    self.launcher("stop")
                    self.verify_restored()
                except BaseException as failure:
                    self.record("recovery-failed", error=str(failure))
                    if error is None:
                        raise

    def verify_restored(self):
        manifest = json.loads((self.directory / "session.json").read_text())
        active = Path("/sys/class/tty/tty0/active").read_text().strip()
        original = str(manifest["original_vt"])
        if active != (original if original.startswith("tty") else f"tty{original}"):
            raise ControlError(
                f"original VT was not restored: active={active}, original={original}"
            )
        runtime = manifest.get("runtime_dir")
        if runtime and Path(runtime).exists():
            raise ControlError("owned session runtime directory was not removed")
        if not manifest.get("cleanup", {}).get("complete"):
            raise ControlError(
                "launcher did not confirm complete process/scope cleanup"
            )
        self.record("restored", active_vt=active, manifest=manifest)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--session", required=True, type=Path, help="new evidence/session directory"
    )
    parser.add_argument("--timeout", type=float, default=90)
    parser.add_argument(
        "--log-level", default="info", help="session/compositor tracing filter"
    )
    parser.add_argument(
        "--case",
        choices=(
            "success",
            "startup-failure",
            "compositor-failure",
            "watchdog",
            "agent-interruption",
        ),
        default="success",
    )
    args = parser.parse_args()
    smoke = Smoke(args)
    try:
        smoke.run()
        smoke.record("complete", ok=True)
        return 0
    except (
        ControlError,
        OSError,
        ValueError,
        KeyError,
        subprocess.SubprocessError,
        KeyboardInterrupt,
    ) as error:
        smoke.record("failed", ok=False, error=str(error))
        return 1
