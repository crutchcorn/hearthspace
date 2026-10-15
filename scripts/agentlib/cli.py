"""Terminal-friendly agent interface. Every successful result is JSON."""

import argparse
import json
import signal
import subprocess
import sys
from pathlib import Path

from .control import Control, ControlError, Deadline, pixel_to_logical, save_capture
from .input import Input

LAUNCHER = Path(__file__).resolve().parents[1] / "hearthspace-native"


def parser():
    root = argparse.ArgumentParser(description=__doc__)
    targets = root.add_mutually_exclusive_group(required=True)
    targets.add_argument(
        "--session", type=Path, help="native launcher session directory"
    )
    targets.add_argument(
        "--socket", type=Path, help="explicit compositor command socket"
    )
    root.add_argument(
        "--timeout",
        type=float,
        default=10,
        help="absolute control/startup deadline (seconds)",
    )
    root.add_argument(
        "--output", help="output name to capture or use for pixel coordinates"
    )
    sub = root.add_subparsers(dest="command", required=True)
    start = sub.add_parser("start", help="start a native session on a spare VT")
    start.add_argument("--watchdog", type=int, default=300)
    start.add_argument("--no-build", action="store_true")
    start.add_argument(
        "--log-level", default="info", help="compositor/session tracing filter"
    )
    sub.add_parser("status")
    sub.add_parser("stop")
    screenshot = sub.add_parser("screenshot")
    screenshot.add_argument("destination", type=Path)
    wait = sub.add_parser("wait", help="wait for a fresh frame or a PNG pixel change")
    wait.add_argument(
        "--change-from", type=Path, help="baseline PNG; any visible change counts"
    )
    wait.add_argument("--save", type=Path)
    for name in ("click", "drag", "scroll", "key", "type", "launch"):
        action = sub.add_parser(name)
        action.add_argument(
            "--wait",
            choices=("frame", "change", "none"),
            default="frame",
            help="fresh presentation, visible change, or acceptance only",
        )
        action.add_argument(
            "--save", type=Path, help="save the frame observed after this action"
        )
        if name in ("click", "drag"):
            action.add_argument("x", type=float)
            action.add_argument("y", type=float)
            action.add_argument(
                "--units", choices=("logical", "pixels"), default="logical"
            )
            action.add_argument(
                "--metadata", type=Path, help="capture JSON for pixel coordinates"
            )
            action.add_argument(
                "--button", choices=("left", "middle", "right"), default="left"
            )
        if name == "drag":
            action.add_argument("end_x", type=float)
            action.add_argument("end_y", type=float)
            action.add_argument("--duration", type=float, default=0.3)
        elif name == "scroll":
            action.add_argument(
                "vertical",
                type=float,
                help="Wayland scroll distance; 120 is one detent",
            )
            action.add_argument("--horizontal", type=float, default=0)
            action.add_argument(
                "--modifier", action="append", default=[], help="held key, e.g. super"
            )
        elif name == "key":
            action.add_argument("chord", help="e.g. ctrl+shift+a, enter, or evdev:30")
        elif name == "type":
            action.add_argument(
                "text", help="US ASCII text; unsupported Unicode fails before input"
            )
        elif name == "launch":
            action.add_argument("app", help="desktop entry ID or 'foot'")
    return root


def target_socket(args):
    if args.socket:
        return args.socket
    manifest = json.loads((args.session / "session.json").read_text())
    return Path(manifest["command_socket"])


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


def action(args, control):
    baseline = control.capture(args.output)[1] if args.wait == "change" else None
    if args.save and args.wait == "none":
        raise ControlError("--save requires --wait frame or --wait change")
    if args.command in ("click", "drag") and args.units == "pixels":
        metadata = (
            json.loads(args.metadata.read_text())
            if args.metadata
            else control.capture(args.output)[0]
        )
        if args.output is not None and metadata["output"] != args.output:
            raise ControlError("metadata belongs to a different output")
        args.x, args.y = pixel_to_logical(args.x, args.y, metadata)
        if args.command == "drag":
            args.end_x, args.end_y = pixel_to_logical(args.end_x, args.end_y, metadata)
    with Input(control) as inputs:
        if args.command == "click":
            inputs.click(args.x, args.y, args.button)
        elif args.command == "drag":
            inputs.drag(
                args.x, args.y, args.end_x, args.end_y, args.button, args.duration
            )
        elif args.command == "scroll":
            inputs.scroll(args.horizontal, args.vertical, args.modifier)
        elif args.command == "key":
            inputs.chord(args.chord)
        elif args.command == "type":
            inputs.type(args.text)
        elif args.command == "launch":
            if args.app == "foot":
                control.request("spawn foot")
            elif args.output is None:
                control.request(f"launch-app {args.app}")
            else:
                control.request(
                    f"launch-app-on-output {args.output.encode().hex()} {args.app}"
                )
    result = {
        "ok": True,
        "command": args.command,
        "accepted": True,
        "verification": args.wait,
    }
    if args.wait != "none":
        capture = (
            control.wait_change(baseline, args.output)
            if baseline is not None
            else control.capture(args.output)
        )
        result.update(
            save_capture(capture, args.save) if args.save else {"capture": capture[0]}
        )
    return result


def run(args):
    deadline = Deadline(args.timeout)
    if args.command in ("start", "stop") or (args.command == "status" and args.session):
        return run_launcher(args)
    control = Control(target_socket(args), deadline)
    if args.command == "status":
        return {"ok": True, **control.status()}
    if args.command == "screenshot":
        return {
            "ok": True,
            **save_capture(control.capture(args.output), args.destination),
        }
    if args.command == "wait":
        capture = (
            control.wait_change(args.change_from.read_bytes(), args.output)
            if args.change_from
            else control.capture(args.output)
        )
        return {
            "ok": True,
            **(
                save_capture(capture, args.save)
                if args.save
                else {"capture": capture[0]}
            ),
        }
    return action(args, control)


def main():
    def interrupted(signum, _frame):
        raise KeyboardInterrupt(f"signal {signum}")

    signal.signal(signal.SIGTERM, interrupted)
    try:
        result = run(parser().parse_args())
        print(json.dumps(result))
        return 0
    except (ControlError, OSError, ValueError, KeyError, KeyboardInterrupt) as error:
        print(
            json.dumps({"ok": False, "error": str(error) or "interrupted"}),
            file=sys.stderr,
        )
        return 1
