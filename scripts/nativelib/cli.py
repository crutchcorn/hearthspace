"""Public CLI builds before touching VTs; private commands run through sudo/systemd."""

import argparse
import json
import os
import shutil
import signal
import subprocess
import sys
from pathlib import Path

from .build import build
from .common import (
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
            command.add_argument(
                "--dev",
                action="store_true",
                help="run development children through Cargo",
            )
            command.add_argument(
                "--vnc",
                action="store_true",
                help="share the physical dev desktop through localhost WayVNC",
            )
            command.add_argument("--vnc-port", type=int)
            if name == "_start":
                command.add_argument("--dev-path", help=argparse.SUPPRESS)
            command.add_argument("--compositor", type=Path)
            command.add_argument("--log-level", default="info")
            command.add_argument(
                "--isolated-profile",
                action="store_true",
                help="use fresh session settings and app profiles inside the artifact directory",
            )
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
    validate_development(args)
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
        if getattr(args, "isolated_profile", False):
            extra.append("--isolated-profile")
        if getattr(args, "dev", False):
            extra.extend(["--dev", "--dev-path", os.environ.get("PATH", os.defpath)])
        if getattr(args, "vnc", False):
            extra.extend(["--vnc", "--vnc-port", str(args.vnc_port or 5900)])
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


def validate_development(args):
    dev = getattr(args, "dev", False)
    vnc = getattr(args, "vnc", False)
    port = getattr(args, "vnc_port", None)
    if vnc and not dev:
        raise ValueError("--vnc requires --dev")
    if port is not None and (not vnc or not 1 <= port <= 65535):
        raise ValueError("--vnc-port requires --vnc and a port between 1 and 65535")
    if dev:
        if getattr(args, "login_session", False) or args.compositor:
            raise ValueError("--dev conflicts with --login-session and --compositor")
        if args.binary_dir.resolve() != ROOT / "target/debug":
            raise ValueError("--dev requires the checkout's default --binary-dir")
        if shutil.which("cargo") is None:
            raise RuntimeError("--dev requires cargo on the launching user's PATH")
    if vnc and shutil.which("wayvnc") is None:
        raise RuntimeError("--vnc requires WayVNC; install wayvnc before starting")


def status(session, timeout=2):
    state = load(session / "session.json")
    state["service"] = unit_info(state["unit"])
    if state.get("vnc"):
        try:
            state["wayvnc"] = load(Path(state["runtime_dir"]) / "wayvnc.json")
            state["processes"] = load(Path(state["runtime_dir"]) / "processes.json")
        except (OSError, ValueError) as error:
            state["wayvnc"] = {"ready": False, "error": str(error)}
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
