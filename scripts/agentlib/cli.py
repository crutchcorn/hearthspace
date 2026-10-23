"""Terminal-friendly agent interface. Every successful result is JSON."""

import argparse
import json
import signal
import sys
from pathlib import Path

from .accessibility import run_accessibility
from .control import Control, ControlError, Deadline, pixel_to_logical, save_capture
from .input import Input
from .lock import lock_session
from .native import run_launcher
from .text import clipboard_command, paste

WINDOW_COMMANDS = {
    "focus": "focus-window",
    "close": "close-window",
    "move": "move-window",
    "resize": "resize-window",
    "focus-next": "focus-next-window",
    "focus-previous": "focus-previous-window",
    "focus-launcher": "focus-launcher",
    "fullscreen": "fullscreen-window",
    "maximize": "maximize-window",
    "restore": "restore-window",
}


def positive_integer(value):
    number = int(value)
    if not 0 < number <= (1 << 64) - 1:
        raise argparse.ArgumentTypeError("must be a positive 64-bit integer")
    return number


def canvas_coordinate(value):
    number = int(value)
    if not -1_000_000 <= number <= 1_000_000:
        raise argparse.ArgumentTypeError("canvas coordinate must be within ±1000000")
    return number


def window_size(value):
    number = int(value)
    if not 0 < number <= 16384:
        raise argparse.ArgumentTypeError("window size must be between 1 and 16384")
    return number


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
        "--dev", action="store_true", help="run development children through Cargo"
    )
    start.add_argument(
        "--vnc",
        action="store_true",
        help="share the physical dev desktop through localhost WayVNC",
    )
    start.add_argument("--vnc-port", type=int)
    start.add_argument(
        "--isolated-profile",
        action="store_true",
        help="use fresh session settings and application profiles",
    )
    start.add_argument(
        "--log-level", default="info", help="compositor/session tracing filter"
    )
    sub.add_parser("status")
    sub.add_parser("apps", help="list launchable desktop applications")
    sub.add_parser("windows", help="list live windows and their stable IDs")
    a11y = sub.add_parser(
        "a11y", help="inspect or activate UI on the session's private accessibility bus"
    )
    semantic = a11y.add_subparsers(dest="a11y_action", required=True)
    semantic.add_parser("snapshot")
    for operation in ("find", "activate"):
        semantic.add_parser(operation).add_argument(
            "selector", help="XPath matching accessible UI elements"
        )
    sub.add_parser("stop")
    lock = sub.add_parser(
        "lock", help="lock the session and wait for secure presentation"
    )
    lock.add_argument("--wait", choices=("locked", "none"), default="locked")
    screenshot = sub.add_parser("screenshot")
    screenshot.add_argument("destination", type=Path)
    wait = sub.add_parser("wait", help="wait for a fresh frame or a PNG pixel change")
    wait.add_argument(
        "--change-from", type=Path, help="baseline PNG; any visible change counts"
    )
    wait.add_argument("--save", type=Path)
    for name in (
        "click",
        "drag",
        "scroll",
        "key",
        "type",
        "paste",
        "clipboard-set",
        "launch",
        *WINDOW_COMMANDS,
    ):
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
        elif name in ("paste", "clipboard-set"):
            action.description = (
                "Replace the session clipboard with up to 16 KiB of UTF-8 text."
            )
            action.add_argument(
                "text", nargs="?", help="text to place on the clipboard"
            )
            action.add_argument(
                "--file", type=Path, help="read UTF-8 text from a file instead"
            )
            if name == "paste":
                action.add_argument(
                    "--shortcut",
                    default="ctrl+v",
                    help="paste shortcut; terminals commonly use ctrl+shift+v",
                )
        elif name == "launch":
            action.add_argument("app", help="desktop entry ID or 'foot'")
        elif name in (
            "focus",
            "close",
            "move",
            "resize",
            "fullscreen",
            "maximize",
            "restore",
        ):
            action.add_argument("window_id", type=positive_integer)
            if name == "move":
                action.add_argument("canvas_x", type=canvas_coordinate)
                action.add_argument("canvas_y", type=canvas_coordinate)
            elif name == "resize":
                action.add_argument("width", type=window_size)
                action.add_argument("height", type=window_size)
            elif name in ("fullscreen", "maximize"):
                action.add_argument(
                    "--off", action="store_true", help="leave this window state"
                )
    return root


def target_socket(args):
    if args.socket:
        return args.socket
    manifest = json.loads((args.session / "session.json").read_text())
    return Path(manifest["command_socket"])


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
        elif args.command == "clipboard-set":
            control.request(clipboard_command(args.text, args.file))
        elif args.command == "paste":
            paste(inputs, clipboard_command(args.text, args.file), args.shortcut)
        elif args.command == "launch":
            if args.app == "foot" and args.output is None:
                control.request("spawn foot")
            elif args.output is None:
                control.request(f"launch-app {args.app}")
            else:
                control.request(
                    f"launch-app-on-output {args.output.encode().hex()} {'foot.desktop' if args.app == 'foot' else args.app}"
                )
        elif args.command in WINDOW_COMMANDS:
            command = WINDOW_COMMANDS[args.command]
            if args.command in (
                "focus",
                "close",
                "move",
                "resize",
                "fullscreen",
                "maximize",
                "restore",
            ):
                command += f" {args.window_id}"
            if args.command == "move":
                command += f" {args.canvas_x} {args.canvas_y}"
            elif args.command == "resize":
                command += f" {args.width} {args.height}"
            elif args.command in ("fullscreen", "maximize"):
                command += " false" if args.off else " true"
            control.request(command)
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
    if args.command in ("start", "stop"):
        return run_launcher(args)
    if args.command == "status" and args.session:
        lifecycle = run_launcher(args)
        snapshot = lifecycle.get("compositor") or {}
        # Keep lifecycle/recovery details, while exposing the same live fields
        # as --socket status. Never promote startup output/frame metadata.
        return {**lifecycle, "outputs": [], **snapshot}
    control = Control(target_socket(args), deadline)
    if args.command == "lock":
        return lock_session(control, args.wait)
    if args.command == "a11y":
        return run_accessibility(args, control, deadline)
    if args.command == "status":
        return {"ok": True, **control.status()}
    if args.command in ("apps", "windows"):
        return {"ok": True, **control.query(args.command)}
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
