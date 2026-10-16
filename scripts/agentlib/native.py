"""Forward native session lifecycle commands to the VT-owning launcher."""

import json
import subprocess
from pathlib import Path

from .control import ControlError

LAUNCHER = Path(__file__).resolve().parents[1] / "hearthspace-native"


def run_launcher(args):
    if args.session is None:
        raise ControlError("start and stop require --session")
    command = [
        str(LAUNCHER),
        args.command,
        "--session",
        str(args.session),
    ]
    if args.command != "status":
        command.extend(["--timeout", str(args.timeout)])
    if args.command == "start":
        command.extend(
            ["--watchdog", str(args.watchdog), "--log-level", args.log_level]
        )
        if args.no_build:
            command.append("--no-build")
        if getattr(args, "isolated_profile", False):
            command.append("--isolated-profile")
        if getattr(args, "dev", False):
            command.append("--dev")
        if getattr(args, "vnc", False):
            command.append("--vnc")
        if getattr(args, "vnc_port", None) is not None:
            command.extend(["--vnc-port", str(args.vnc_port)])
    # The launcher owns an independent watchdog, including cleanup if its caller
    # disappears. Permit its bounded cleanup window after its startup deadline.
    # Native launcher's pre-VT build has a separate 30-minute absolute deadline.
    build_budget = 1800 if args.command == "start" and not args.no_build else 0
    try:
        result = subprocess.run(
            command,
            check=False,
            capture_output=True,
            text=True,
            timeout=args.timeout + build_budget + 30,
        )
    except subprocess.TimeoutExpired as error:
        raise ControlError(
            "native launcher timed out; watchdog retains recovery ownership"
        ) from error
    if result.returncode:
        raise ControlError(
            result.stderr.strip() or result.stdout.strip() or "native launcher failed"
        )
    return json.loads(result.stdout)
