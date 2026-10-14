"""Semantic commands always target the launcher's explicit private bus."""

import json
import subprocess
from pathlib import Path

from .control import ControlError


def run_accessibility(args, control, deadline):
    if args.session is None:
        raise ControlError("a11y requires --session with its private D-Bus manifest")
    manifest = json.loads((args.session / "session.json").read_text())
    bus = manifest.get("dbus_session_bus_address")
    if not bus:
        raise ControlError("session manifest has no private D-Bus address")
    # Reject stale session manifests before enabling or acting on accessibility.
    control.status()
    binary = Path(manifest["binary_dir"]) / "hearthspace-a11y"
    if not binary.is_file():
        raise ControlError(
            "build hearthspace-a11y in the session binary directory first"
        )
    command = [
        str(binary),
        "--bus",
        bus,
        "--timeout",
        str(min(300, deadline.remaining())),
        args.a11y_action,
    ]
    if args.a11y_action != "snapshot":
        command.append(args.selector)
    try:
        result = subprocess.run(
            command,
            capture_output=True,
            text=True,
            check=False,
            timeout=deadline.remaining(),
        )
    except subprocess.TimeoutExpired as error:
        raise ControlError("accessibility command timed out") from error
    payload = json.loads(result.stdout if result.returncode == 0 else result.stderr)
    if result.returncode:
        raise ControlError(payload.get("error", "accessibility command failed"))
    if args.a11y_action == "activate":
        payload["capture"] = control.capture(args.output)[0]
    return payload
