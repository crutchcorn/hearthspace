"""Native GTK 3/4 titlebar moves with real touch input and content assertions."""

import argparse
import json
import os
import shutil
import subprocess
import tempfile
import uuid
from contextlib import contextmanager
from pathlib import Path

from . import gtk_titlebar
from . import gtk_touch_checks as checks
from .control import ControlError, Deadline
from .daily import DailySmoke
from .daily_checks import a11y
from .smoke import Smoke


@contextmanager
def application(smoke, kind):
    owned = smoke.artifacts / f"{kind}-profile"
    owned.mkdir()
    smoke.client_state = owned / "state.json"
    smoke.kind = kind
    fixture_id = uuid.uuid4().hex
    app_id, title = {
        "calculator": ("org.gnome.Calculator", "Calculator"),
        "generic": ("dev.hearthspace.WindowTest", "Window states fixture"),
        "gtk3": (
            f"gtk_touch.py-{fixture_id}",
            f"Hearthspace GTK3 touch fixture {fixture_id}",
        ),
        "gtk4": (
            f"gtk_touch.py-{fixture_id}",
            f"Hearthspace GTK4 touch fixture {fixture_id}",
        ),
    }[kind]
    smoke.client_identity = {"fixture_id": fixture_id, "app_id": app_id, "title": title}
    env = {
        **os.environ,
        "DBUS_SESSION_BUS_ADDRESS": smoke.manifest["dbus_session_bus_address"],
        "XDG_RUNTIME_DIR": smoke.manifest["runtime_dir"],
        "WAYLAND_DISPLAY": smoke.manifest["wayland_display"],
        "GDK_BACKEND": "wayland",
        "GTK_A11Y": "atspi",
        "GSETTINGS_BACKEND": "memory",
        "LC_ALL": "C.UTF-8",
        "HEARTHSPACE_GTK_TOUCH_VERSION": "4" if kind == "gtk4" else "3",
        "HEARTHSPACE_GTK_TOUCH_STATE": str(smoke.client_state),
        "HEARTHSPACE_GTK_TOUCH_ID": fixture_id,
        "HEARTHSPACE_GTK_TOUCH_APP_ID": app_id,
        "HEARTHSPACE_GTK_TOUCH_TITLE": title,
    }
    for name in ("CONFIG", "CACHE", "DATA", "STATE"):
        path = owned / name.lower()
        path.mkdir(parents=True, exist_ok=True)
        env[f"XDG_{name}_HOME"] = str(path)
    for name in ("DISPLAY", "AT_SPI_BUS_ADDRESS", "DBUS_SESSION_BUS_PID"):
        env.pop(name, None)
    # Generic fixture options inherited from other testing must not alter this
    # run's initial state or app identity.
    for name in tuple(env):
        if name.startswith("HEARTHSPACE_WINDOW_TEST_"):
            env.pop(name)
    if smoke.args.titlebar_maximize:
        env["WAYLAND_DEBUG"] = "client"
    if kind == "calculator":
        command = [str(smoke.args.calculator), "--mode=basic"]
    elif kind == "generic":
        command = [str(smoke.args.window_test.resolve()), str(smoke.client_state)]
    else:
        command = [
            "/usr/bin/python3",
            str(Path(__file__).resolve().parents[1] / "tests/fixtures/gtk_touch.py"),
        ]
    previous = {window["id"] for window in smoke.control.status()["windows"]}
    with (smoke.artifacts / f"{kind}.log").open("w") as log:
        process = subprocess.Popen(command, env=env, stdout=log, stderr=log)
        smoke.client_process = process
        smoke.record(
            "gtk-launch",
            kind=kind,
            argv=command,
            pid=process.pid,
            identity=smoke.client_identity,
            state=str(smoke.client_state),
        )
        try:

            def mapped():
                if process.poll() is not None:
                    raise ControlError(f"{kind} exited early: {process.returncode}")
                matches = [
                    window
                    for window in smoke.control.status()["windows"]
                    if window.get("mapped")
                    and window.get("app_id") == app_id
                    and window.get("title") == title
                ]
                if len(matches) > 1:
                    raise ControlError(f"ambiguous {kind} fixture windows: {matches}")
                if not matches or matches[0]["id"] in previous:
                    return None
                if kind != "calculator" and checks.client_state(smoke) is None:
                    return None
                return matches[0]

            window = smoke.until(f"new native {kind} window", mapped)
            smoke.target = window["id"]
            smoke.gtk_count, smoke.fullscreen_entries = 0, 0
            checks.snapshot(smoke, f"{kind}-ready")
            if kind != "generic" and window["titlebar_height"] != 0:
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
        if args.titlebar_maximize:
            # Borrow the root-owned session; do not call DailySmoke.run/start/
            # stop or replace its artifacts. Each invocation gets fresh state.
            if not (args.session / "session.json").is_file():
                raise ControlError(
                    "--titlebar-maximize requires an existing native session"
                )
            Smoke.__init__(self, args)
            self.artifacts = Path(
                tempfile.mkdtemp(prefix="gtk-titlebar-", dir=self.artifacts)
            )
            self.total = Deadline(args.total_timeout)
            self.manifest, self.target = None, None
        else:
            super().__init__(args)

    def run(self):
        if not self.args.titlebar_maximize:
            return super().run()
        try:
            self.workflow()
        except BaseException as failure:
            self.record("operation-failed", error=str(failure))
            raise
        self.record("complete", ok=True, session_owned=False)

    def workflow(self):
        self.manifest = self.connect()
        if self.args.inspect_only:
            status = self.control.status()
            if status["backend"] != "native" or not status.get("shell_ready"):
                raise ControlError("GTK inspection requires a ready native shell")
            self.capture("initial")
        else:
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
        defaults = (
            ("gtk3", "gtk4", "generic")
            if self.args.titlebar_maximize
            else ("calculator", "gtk3")
        )
        kinds = defaults if self.args.case == "all" else (self.args.case,)
        for kind in kinds:
            self.step(f"{kind}-workflow", lambda smoke, kind=kind: smoke.client(kind))

    def client(self, kind):
        with application(self, kind):
            try:
                if not self.args.inspect_only:
                    for scale in (1, 0.8):
                        if self.args.titlebar_maximize:
                            gtk_titlebar.run(self, scale)
                        else:
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
    parser.add_argument(
        "--case",
        choices=("all", "calculator", "gtk3", "gtk4", "generic"),
        default="all",
    )
    parser.add_argument(
        "--titlebar-maximize",
        action="store_true",
        help="attach to an existing root-owned session for GTK3/GTK4/F9 CSD acceptance; never start/stop it",
    )
    parser.add_argument(
        "--window-test", type=Path, default=Path("target/debug/hearthspace-window-test")
    )
    parser.add_argument("--inspect-only", action="store_true")
    parser.add_argument("--timeout", type=float, default=120)
    parser.add_argument("--total-timeout", type=float, default=420)
    parser.add_argument("--log-level", default="info")
    args = parser.parse_args()
    try:
        if args.titlebar_maximize and args.case == "calculator":
            raise ControlError(
                "titlebar acceptance uses instrumented gtk3/gtk4 fixtures; select --case all, gtk3, gtk4, or generic"
            )
        if not args.titlebar_maximize and args.case in ("gtk4", "generic"):
            raise ControlError("--case gtk4/generic requires --titlebar-maximize")
        if shutil.which("libinput") is None:
            raise ControlError("GTK touch smoke requires libinput-tools")
        subprocess.run(
            ["/usr/bin/python3", "-c", "import evdev"], check=True, timeout=5
        )
        if (
            not args.titlebar_maximize
            and args.case in ("calculator", "all")
            and not os.access(args.calculator, os.X_OK)
        ):
            raise ControlError(
                "GTK touch smoke requires an executable GNOME Calculator"
            )
        if (
            args.titlebar_maximize
            and args.case in ("generic", "all")
            and not os.access(args.window_test, os.X_OK)
        ):
            raise ControlError(
                "titlebar acceptance requires the already-built hearthspace-window-test binary"
            )
        versions = (
            ("3.0", "4.0")
            if args.titlebar_maximize and args.case == "all"
            else (
                ("4.0",)
                if args.case == "gtk4"
                else (("3.0",) if args.case in ("gtk3", "all") else ())
            )
        )
        for version in versions:
            subprocess.run(
                [
                    "/usr/bin/python3",
                    "-c",
                    f"import gi; gi.require_version('Gtk', '{version}'); from gi.repository import Gtk",
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
