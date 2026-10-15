"""Public CLI builds before touching VTs; private commands run through sudo/systemd."""

import argparse
import hashlib
import json
import os
import shutil
import signal
import subprocess
import sys
from pathlib import Path

from .common import (
    BINARIES,
    ENTRY,
    ROOT,
    control,
    load,
    run,
    save,
    unit_info,
    wait_ready,
)


def parser():
    result = argparse.ArgumentParser(description=__doc__)
    commands = result.add_subparsers(dest="command", required=True)
    for name in ("start", "status", "stop", "_start"):
        command = commands.add_parser(name)
        command.add_argument("--session", required=True, type=Path)
        if name in ("stop", "status"):
            command.add_argument("--timeout", type=float, default=40)
        if name in ("start", "_start"):
            command.add_argument("--timeout", type=float, default=45)
            command.add_argument("--watchdog", type=int, default=300)
            command.add_argument(
                "--binary-dir", type=Path, default=ROOT / "target/debug"
            )
            command.add_argument("--no-build", action="store_true")
            command.add_argument("--compositor", type=Path)
            command.add_argument("--log-level", default="info")
            command.add_argument(
                "--login-session",
                action="store_true",
                help="exercise the installed login cleanup guard in the owned PAM session",
            )
    for name in ("_exec", "_recover", "_stop", "_graceful-stop"):
        command = commands.add_parser(name)
        command.add_argument("--state", required=True, type=Path)
    return result


def privileged(command, args, timeout=40):
    result = run(
        ["sudo", "-n", "/usr/bin/python3", ENTRY, command, *args], timeout=timeout
    )
    return json.loads(result.stdout)


def build(args, session):
    required = BINARIES
    if getattr(args, "login_session", False):
        helper = args.binary_dir.resolve() / "hearthspace-login-guard"
        if not helper.is_file() or not os.access(helper, os.X_OK):
            raise RuntimeError(
                "--login-session requires an executable installed cleanup helper: "
                f"{helper}; use --no-build --binary-dir with an installed release"
            )
        share = helper.parent.parent / "share"
        for relative in (
            "applications/dev.hearthspace.Settings.desktop",
            "icons/hicolor/scalable/apps/hearthspace.svg",
        ):
            resource = share / relative
            if not resource.is_file() or not os.access(resource, os.R_OK):
                raise RuntimeError(
                    f"--login-session requires installed shared data: {resource}"
                )
        required += ("hearthspace-login-guard",)
    if not args.no_build:
        with (session / "build.log").open("w") as log:
            command = [
                "cargo",
                "build",
                "--locked",
                "--target-dir",
                str(ROOT / "target"),
                "-p",
                "hearthspace-session",
                "-p",
                "hearthspace-compositor",
                "-p",
                "hearthspace-shell",
                "-p",
                "hearthspace-settings",
                "-p",
                "hearthspace-settingsd",
                "-p",
                "waydriver-hearthspace",
                "-p",
                "hearthspace-notifications",
                "--features",
                "hearthspace-compositor/udev",
            ]
            # Pin the artifact path even if the caller sets CARGO_TARGET_DIR.
            if args.binary_dir.resolve() != ROOT / "target/debug":
                raise ValueError("a custom --binary-dir requires --no-build")
            result = subprocess.run(
                command,
                check=False,
                cwd=ROOT,
                stdout=log,
                stderr=subprocess.STDOUT,
                timeout=1800,
            )
            if result.returncode:
                raise RuntimeError(f"native build failed; see {session / 'build.log'}")
    binaries = {}
    for name in required:
        path = args.binary_dir.resolve() / name
        if not path.is_file() or not os.access(path, os.X_OK):
            raise RuntimeError(f"missing executable {path}; build before switching VTs")
        # Retain exact binary provenance even when the caller explicitly skips building.
        with path.open("rb") as binary_file:
            digest = hashlib.file_digest(binary_file, "sha256").hexdigest()
        binaries[name] = {
            "path": str(path),
            "sha256": digest,
            "mtime": path.stat().st_mtime,
        }
    save(session / "binaries.json", binaries)


def preflight():
    for command in (
        "sudo",
        "systemd-run",
        "systemctl",
        "loginctl",
        "chvt",
        "dbus-run-session",
    ):
        if shutil.which(command) is None:
            raise RuntimeError(f"native launcher requires {command}")
    result = run(["sudo", "-n", "true"], check=False)
    if result.returncode:
        raise RuntimeError(
            "native launcher requires passwordless sudo for VT/session setup: "
            + result.stderr.strip()
        )


def start(args):
    if not 1 <= args.timeout <= 600 or not 1 <= args.watchdog <= 86400:
        raise ValueError("timeout must be 1–600s; watchdog must be 1–86400s")
    session = args.session.resolve()
    session.mkdir(parents=True, exist_ok=True, mode=0o700)
    if (session / "session.json").exists():
        raise ValueError(
            "session manifest already exists; use a new artifact directory"
        )
    session.chmod(0o700)
    preflight()
    build(args, session)
    state = None
    try:
        extra = ["--compositor", args.compositor.resolve()] if args.compositor else []
        if getattr(args, "login_session", False):
            extra.append("--login-session")
        state = privileged(
            "_start",
            [
                "--session",
                session,
                "--binary-dir",
                args.binary_dir.resolve(),
                "--timeout",
                args.timeout,
                "--watchdog",
                args.watchdog,
                "--log-level",
                args.log_level,
                *extra,
            ],
        )
        state = wait_ready(state, args.timeout)
        save(session / "session.json", state)
        return state
    except BaseException:
        # _start persists state before submitting the service, so an interrupted
        # parent can recover even if the accepted systemd-run response was lost.
        if (session / "session.json").exists():
            state = load(session / "session.json")
        if state:
            try:
                privileged("_stop", ["--state", state["state_file"]])
            except (
                OSError,
                RuntimeError,
                ValueError,
                subprocess.SubprocessError,
            ) as error:
                print(
                    f"startup cleanup failed; systemd watchdog remains armed: {error}",
                    file=sys.stderr,
                )
        raise


def status(session, timeout=2):
    state = load(session / "session.json")
    state["service"] = unit_info(state["unit"])
    try:
        state["compositor"] = json.loads(
            control(state["command_socket"], "status", timeout)
        )
        state["responsive"] = True
    except (OSError, ValueError, RuntimeError) as error:
        state["responsive"] = False
        state["control_error"] = str(error)
    return state


def dispatch(args):
    if args.command == "start":
        return start(args)
    if args.command == "status":
        return status(args.session.resolve(), args.timeout)
    if args.command == "stop":
        state = load(args.session.resolve() / "session.json")
        return privileged(
            "_stop", ["--state", state["state_file"]], timeout=args.timeout
        )
    if args.command == "_start":
        from .service import launch

        return launch(args)
    if args.command == "_exec":
        from .service import exec_session

        return exec_session(args.state)
    if args.command == "_graceful-stop":
        from .graceful_stop import request_stop

        return request_stop(args.state)
    if args.command == "_recover":
        from .recovery import recover

        result = recover(args.state)
        if not result["cleanup"]["complete"]:
            raise RuntimeError(f"incomplete independent recovery: {result['cleanup']}")
        return result
    from .recovery import stop

    return stop(args.state)


def main():
    def interrupted(signum, _frame):
        raise KeyboardInterrupt(f"interrupted by signal {signum}")

    signal.signal(signal.SIGTERM, interrupted)
    signal.signal(signal.SIGHUP, interrupted)
    try:
        result = dispatch(parser().parse_args())
        print(json.dumps({"ok": True, **result}))
    except (
        OSError,
        RuntimeError,
        ValueError,
        KeyError,
        subprocess.SubprocessError,
        KeyboardInterrupt,
    ) as error:
        print(json.dumps({"ok": False, "error": str(error) or "interrupted"}))
        sys.exit(1)
