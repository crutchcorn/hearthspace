"""Run cross-app clipboard acceptance on an owned native VT session."""

import argparse
import hashlib
import math
import os
import subprocess
from pathlib import Path

from .clipboard import run_clipboard
from .control import ControlError
from .smoke import Smoke

ROOT = Path(__file__).resolve().parents[2]


def run(args):
    fixture = ROOT / "target/debug/hearthspace-clipboard-test"
    if not fixture.is_file():
        raise ControlError(
            "build the clipboard fixture with --features test-apps first"
        )
    if (args.session / "session.json").exists():
        raise ControlError("clipboard smoke requires a new session directory")
    smoke = Smoke(args)
    with fixture.open("rb") as binary:
        smoke.record(
            "clipboard-fixture",
            path=str(fixture),
            sha256=hashlib.file_digest(binary, "sha256").hexdigest(),
        )
    error = None
    try:
        smoke.launcher(
            "start", "--no-build", "--watchdog", str(math.ceil(args.timeout + 90))
        )
        manifest = smoke.connect()
        smoke.record("ready", manifest=manifest, status=smoke.control.status())
        env = {
            **os.environ,
            "XDG_RUNTIME_DIR": manifest["runtime_dir"],
            "WAYLAND_DISPLAY": manifest["wayland_display"],
            "DBUS_SESSION_BUS_ADDRESS": manifest["dbus_session_bus_address"],
            "GSETTINGS_BACKEND": "memory",
        }
        env.pop("DBUS_SESSION_BUS_PID", None)
        run_clipboard(smoke, env, fixture)
    except BaseException as failure:
        error = failure
        smoke.record("operation-failed", error=str(failure))
        raise
    finally:
        if (smoke.directory / "session.json").exists():
            try:
                smoke.launcher("stop")
                smoke.verify_restored()
            except BaseException as failure:
                smoke.record("recovery-failed", error=str(failure))
                if error is None:
                    raise
    smoke.record("complete", ok=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--session", type=Path, required=True)
    parser.add_argument("--timeout", type=float, default=120)
    parser.add_argument("--log-level", default="info")
    args = parser.parse_args()
    try:
        run(args)
        return 0
    except (
        ControlError,
        OSError,
        ValueError,
        KeyError,
        subprocess.SubprocessError,
        KeyboardInterrupt,
    ) as error:
        print(f"clipboard smoke failed: {error}")
        return 1
