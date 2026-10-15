"""Opt-in native suspend acceptance with automatic wake and fail-closed locking."""

import argparse
import json
import shlex
import shutil
import subprocess
from pathlib import Path

from . import power_evidence as verify
from .control import ControlError
from .daily import DailySmoke
from .daily_checks import terminal
from .input import Input
from .power_rtc import WakeAlarm, command
from .visual import marker_bounds

ROOT = Path(__file__).resolve().parents[2]
SERVICES = [
    "systemctl",
    "--user",
    "show",
    "t3code.service",
    "t3code-tailnet.service",
    "--property=Id",
    "--property=ActiveState",
    "--property=MainPID",
]


class PowerSmoke(DailySmoke):
    def __init__(self, args):
        if not args.allow_system_suspend:
            raise ControlError("physical suspend requires --allow-system-suspend")
        if not 15 <= args.wake_after <= 45:
            raise ControlError("RTC wake delay must be between 15 and 45 seconds")
        if args.timeout < args.wake_after + 15:
            raise ControlError("step timeout must exceed RTC wake delay by 15 seconds")
        args.captures, args.notifications = 3, False
        super().__init__(args)
        self.alarm = WakeAlarm(args.rtc, self.record)
        self.fixture = ROOT / "target/debug/examples/session-lock-fixture"
        self.locker = getattr(args, "locker", "fixture")

    def preflight(self):
        if self.locker == "fixture" and not self.fixture.is_file():
            raise ControlError(
                "build the compositor session-lock-fixture example first"
            )
        if self.locker == "swaylock" and shutil.which("swaylock") is None:
            raise ControlError("automatic locker acceptance requires swaylock")
        command(["sudo", "-n", "true"])
        local = command(["timedatectl", "show", "--property=LocalRTC", "--value"])
        if local["stdout"].strip() != "no":
            raise ControlError(
                "power smoke currently requires an RTC configured in UTC"
            )
        if "mem" not in Path("/sys/power/state").read_text().split():
            raise ControlError("kernel does not expose suspend-to-RAM")
        capability = command(
            [
                "busctl",
                "call",
                "org.freedesktop.login1",
                "/org/freedesktop/login1",
                "org.freedesktop.login1.Manager",
                "CanSuspend",
            ]
        )
        if capability["stdout"].strip() != 's "yes"':
            raise ControlError("logind does not allow unattended suspend")
        self.alarm.available()
        self.services_before = command(SERVICES)
        self.record(
            "power-preflight",
            active_vt=Path("/sys/class/tty/tty0/active").read_text().strip(),
            memory_sleep=Path("/sys/power/mem_sleep").read_text().strip(),
            process_cgroup=Path("/proc/self/cgroup").read_text(),
            suspend=verify.sleep_sample(),
            inhibitors=command(["systemd-inhibit", "--list", "--no-pager"]),
            background_services=self.services_before,
        )
        self.alarm.prove_interrupt()

    def power(self):
        path = Path(self.manifest["runtime_dir"]) / "power.json"
        return json.loads(path.read_text()) if path.exists() else {}

    def workflow(self):
        self.manifest = self.connect()
        self.processes_before = {
            name: verify.process_identity(self.manifest["processes"][name])
            for name in ("supervisor", "settingsd", "compositor", "shell")
        }
        if len(self.control.status()["outputs"]) != 1:
            raise ControlError("power smoke requires one connected output")
        self.step("ready", DailySmoke.ready)
        self.step("power-ready", PowerSmoke.power_ready)
        self.step("preserved-app", PowerSmoke.application)
        if self.locker == "swaylock":
            self.step("unlocked-before-sleep", PowerSmoke.unlocked)
            self.step("suspend-resume", PowerSmoke.suspend)
            self.step("automatic-swaylock", PowerSmoke.automatic_locker)
            return
        release = self.artifacts / "fixture-release"
        socket = Path(self.manifest["runtime_dir"]) / self.manifest["wayland_display"]
        with (self.artifacts / "lock-fixture.log").open("w") as log:
            process = subprocess.Popen(
                [str(self.fixture), str(socket), str(release), "unlock", "300"],
                stdout=log,
                stderr=subprocess.STDOUT,
            )
            try:
                self.step("lock-confirmed", PowerSmoke.locked)
                self.step("suspend-resume", PowerSmoke.suspend)
                release.touch(exist_ok=False)
                process.wait(timeout=10)
                if process.returncode:
                    raise ControlError("owner lock fixture failed during unlock")
                self.step("recovered-desktop", PowerSmoke.desktop)
            finally:
                if process.poll() is None:
                    process.kill()
                    process.wait(timeout=5)
                self.alarm.clear()

    def unlocked(self):
        status = self.control.status()
        if (
            status.get("locked") is not False
            or status.get("lock_confirmed") is not False
        ):
            raise ControlError(
                "automatic swaylock test must begin from an unlocked desktop"
            )
        if verify.owned_swaylocks(self.manifest["compositor_pid"]):
            raise ControlError("a swaylock process already exists in the owned session")
        self.record("unlocked-before-sleep", status=status)

    def preserved_processes(self):
        for name, identity in self.processes_before.items():
            if verify.process_identity(identity["pid"]) != identity:
                raise ControlError(f"{name} process restarted during suspend")
        self.record("processes-preserved", processes=self.processes_before)

    def automatic_locker(self):
        self.preserved_processes()
        lockers = self.until(
            "real swaylock child in the owned compositor session",
            lambda: verify.owned_swaylocks(self.manifest["compositor_pid"]),
        )
        self.locked()
        self.record(
            "automatic-swaylock-confirmed",
            lockers=lockers,
            authenticated_unlock_tested=False,
            cleanup="owned-native-session-stop",
        )

    def power_ready(self):
        self.until(
            "connected power coordinator with sleep inhibitor",
            lambda: (
                (state := self.power()).get("phase") == "awake"
                and all(
                    state.get(key) for key in ("connected", "active", "sleep_inhibited")
                )
            ),
        )
        self.power_before = self.power()
        self.record("power-ready", state=self.power_before)

    def application(self):
        terminal(self)
        with Input(self.control) as inputs:
            inputs.type(
                "printf '\\033[2J\\033[H\\033[48;2;0;255;0mPOWER_SURVIVOR\\033[0m\\n'\n"
            )
        self.window_before = self.window(self.target)
        self.frames_before = {}
        for index, output in enumerate(self.control.status()["outputs"]):

            def visible(output=output, index=index):
                frame = self.control.capture(output["name"])
                if index == 0:
                    try:
                        marker_bounds(frame, self.window(self.target))
                    except ControlError:
                        return None
                return frame

            self.until("visible application marker", visible)
            image = self.capture(f"before-sleep-{index}", output["name"])
            self.frames_before[output["name"]] = image[0]
        self.record("application-before", window=self.window_before)

    def locked(self):
        self.until(
            "owner lock presented",
            lambda: (
                (status := self.control.status()).get("locked")
                and status.get("lock_confirmed")
            ),
        )
        state = self.control.status()
        verify.locked(state)
        try:
            self.control.capture()
        except ControlError as error:
            if "locked" not in str(error):
                raise
        else:
            raise ControlError("capture exposed desktop content while locked")
        self.record("locked", status=state)

    def suspend(self):
        before = verify.sleep_sample()
        self.alarm.arm(self.args.wake_after)
        self.record("suspend-request", before=before)
        self.record(
            "suspend-command", **command(["sudo", "-n", "systemctl", "suspend"])
        )
        self.until(
            "completed kernel suspend cycle",
            lambda: verify.sleep_sample()["success"] > before["success"],
        )
        after = verify.sleep_sample()
        elapsed = verify.resumed(before, after)
        self.record(
            "kernel-resumed", before=before, after=after, suspended_seconds=elapsed
        )

        def recovery():
            state = self.power()
            try:
                verify.recovered(self.power_before, state)
            except ControlError:
                return None
            return state

        state = self.until(
            "rearmed power coordinator and presented lock shield", recovery
        )
        self.locked()
        verify.recovery_ready(self.control.status())
        self.record("power-recovered", state=state)
        self.alarm.clear()

    def desktop(self):
        self.until(
            "owner fixture unlocked", lambda: not self.control.status()["locked"]
        )
        verify.preserved(self.window_before, self.window(self.target))
        self.preserved_processes()
        after_outputs = {item["name"] for item in self.control.status()["outputs"]}
        if after_outputs != self.frames_before.keys():
            raise ControlError("connected output set changed across suspend")
        for index, (output, before) in enumerate(self.frames_before.items()):
            image = self.capture(f"after-resume-{index}", output)
            verify.fresh_frame(before, image[0])
            if index == 0:
                marker_bounds(image, self.window(self.target))
        destination = self.artifacts / "input-after-resume.txt"
        with Input(self.control) as inputs:
            inputs.type(
                f"printf '%s' {shlex.quote(self.marker)} > {shlex.quote(str(destination))}\n"
            )
        self.until(
            "same terminal accepts input after resume",
            lambda: destination.exists() and destination.read_text() == self.marker,
        )
        self.capture("input-after-resume")
        self.record(
            "application-preserved",
            window=self.window(self.target),
            marker=str(destination),
        )

    def run(self):
        self.preflight()
        try:
            super().run()
        finally:
            self.alarm.clear()

    def verify_restored(self):
        super().verify_restored()
        after = command(SERVICES)
        if after["stdout"] != self.services_before["stdout"]:
            raise ControlError(
                "background agent or remote service changed across suspend"
            )
        self.record("background-services-preserved", **after)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--session", type=Path, required=True)
    parser.add_argument("--allow-system-suspend", action="store_true")
    parser.add_argument("--locker", choices=("fixture", "swaylock"), default="fixture")
    parser.add_argument("--wake-after", type=int, default=20)
    parser.add_argument("--rtc", default="rtc0")
    parser.add_argument("--timeout", type=float, default=60)
    parser.add_argument("--total-timeout", type=float, default=240)
    parser.add_argument("--log-level", default="info")
    args = parser.parse_args()
    try:
        PowerSmoke(args).run()
        return 0
    except (
        ControlError,
        OSError,
        ValueError,
        KeyError,
        subprocess.SubprocessError,
        KeyboardInterrupt,
    ) as error:
        print(json.dumps({"ok": False, "error": str(error)}), flush=True)
        return 1
