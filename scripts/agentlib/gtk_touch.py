"""Native GTK 3/4 titlebar moves with real touch input and content assertions."""

import argparse
import json
import os
import shutil
import subprocess
from contextlib import contextmanager
from pathlib import Path

from . import gtk_touch_checks as checks
from .control import ControlError
from .daily import DailySmoke
from .daily_checks import a11y


@contextmanager
def application(smoke, kind):
    owned = smoke.directory / f"{kind}-profile"
    env = {
        **os.environ,
        "DBUS_SESSION_BUS_ADDRESS": smoke.manifest["dbus_session_bus_address"],
        "XDG_RUNTIME_DIR": smoke.manifest["runtime_dir"],
        "WAYLAND_DISPLAY": smoke.manifest["wayland_display"],
        "GDK_BACKEND": "wayland",
        "GTK_A11Y": "atspi",
        "GSETTINGS_BACKEND": "memory",
        "LC_ALL": "C.UTF-8",
    }
    for name in ("CONFIG", "CACHE", "DATA", "STATE"):
        path = owned / name.lower()
        path.mkdir(parents=True, exist_ok=True)
        env[f"XDG_{name}_HOME"] = str(path)
    for name in ("DISPLAY", "AT_SPI_BUS_ADDRESS", "DBUS_SESSION_BUS_PID"):
        env.pop(name, None)
    command = (
        [str(smoke.args.calculator), "--mode=basic"]
        if kind == "calculator"
        else [
            "/usr/bin/python3",
            str(Path(__file__).resolve().parents[1] / "tests/fixtures/gtk_touch.py"),
        ]
    )
    previous = {window["id"] for window in smoke.control.status()["windows"]}
    with (smoke.artifacts / f"{kind}.log").open("w") as log:
        process = subprocess.Popen(command, env=env, stdout=log, stderr=log)
        smoke.record("gtk-launch", kind=kind, argv=command, pid=process.pid)
        try:

            def mapped():
                if process.poll() is not None:
                    raise ControlError(f"{kind} exited early: {process.returncode}")
                return next(
                    (
                        window
                        for window in smoke.control.status()["windows"]
                        if window["id"] not in previous and window.get("mapped")
                    ),
                    None,
                )

            window = smoke.until(f"new native {kind} window", mapped)
            smoke.target = window["id"]
            smoke.kind = kind
            checks.snapshot(smoke, f"{kind}-ready")
            if window["titlebar_height"] != 0:
                raise ControlError(f"{kind} must use GTK client-side decorations")
            yield
        finally:
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=3)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait(timeout=3)
            smoke.record("gtk-client-stopped", pid=process.pid, code=process.returncode)
        smoke.until(f"{kind} window removed", lambda: not smoke.window(smoke.target))


class GtkTouchSmoke(DailySmoke):
    def __init__(self, args):
        args.captures, args.notifications = 1, False
        super().__init__(args)

    def workflow(self):
        self.manifest = self.connect()
        self.step("ready", DailySmoke.ready)
        outputs = self.control.status()["outputs"]
        if len(outputs) != 1:
            raise ControlError("GTK touch acceptance requires one active output")
        self.output = outputs[0]
        if self.output["logical_width"] < 960 or self.output["logical_height"] < 800:
            raise ControlError(
                "GTK touch acceptance requires at least 960 by 800 logical pixels"
            )
        # Enable the private AT-SPI bridge before the GTK clients start.
        a11y(self, "snapshot")
        kinds = ("calculator", "gtk3") if self.args.case == "all" else (self.args.case,)
        for kind in kinds:
            self.step(f"{kind}-workflow", lambda smoke, kind=kind: smoke.client(kind))

    def client(self, kind):
        with application(self, kind):
            try:
                if not self.args.inspect_only:
                    for scale in (1, 0.8):
                        checks.run(self, scale)
            except BaseException:
                self.failure_evidence(f"{kind}-failed")
                raise


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--session", type=Path, required=True)
    parser.add_argument(
        "--calculator", type=Path, default=Path("/usr/bin/gnome-calculator")
    )
    parser.add_argument("--case", choices=("all", "calculator", "gtk3"), default="all")
    parser.add_argument("--inspect-only", action="store_true")
    parser.add_argument("--timeout", type=float, default=120)
    parser.add_argument("--total-timeout", type=float, default=420)
    parser.add_argument("--log-level", default="info")
    args = parser.parse_args()
    try:
        if shutil.which("libinput") is None:
            raise ControlError("GTK touch smoke requires libinput-tools")
        subprocess.run(
            ["/usr/bin/python3", "-c", "import evdev"], check=True, timeout=5
        )
        if args.case in ("calculator", "all") and not os.access(
            args.calculator, os.X_OK
        ):
            raise ControlError(
                "GTK touch smoke requires an executable GNOME Calculator"
            )
        if args.case in ("gtk3", "all"):
            subprocess.run(
                [
                    "/usr/bin/python3",
                    "-c",
                    "import gi; gi.require_version('Gtk', '3.0'); from gi.repository import Gtk",
                ],
                check=True,
                timeout=5,
            )
        GtkTouchSmoke(args).run()
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
