"""Record an explicitly owned native session; lifecycle remains with its launcher."""

import argparse
import hashlib
import json
import math
import shutil
import signal
import sys
import threading
import time
from pathlib import Path

from .control import Control, ControlError, Deadline
from .recording_capture import Frames, collect
from .recording_encode import encode


def parser():
    result = argparse.ArgumentParser(description=__doc__)
    result.add_argument("--session", type=Path, required=True)
    result.add_argument(
        "--output", type=Path, required=True, help="new .mp4 destination"
    )
    result.add_argument(
        "--duration",
        type=float,
        default=60,
        help="maximum live capture seconds (1–180)",
    )
    result.add_argument("--fps", type=int, default=30, choices=range(10, 61))
    result.add_argument(
        "--width",
        type=int,
        default=1280,
        help="even encoded width, at most native width",
    )
    result.add_argument(
        "--monitor", help="output name; defaults to first native output"
    )
    result.add_argument(
        "--stop-file", type=Path, help="finish capture when this new file appears"
    )
    return result


def run(args):
    if not math.isfinite(args.duration) or not 1 <= args.duration <= 180:
        raise ControlError("duration must be between 1 and 180 seconds")
    if args.width < 320 or args.width > 3840 or args.width % 2:
        raise ControlError("encoded width must be even and between 320 and 3840")
    destination = args.output.resolve()
    if destination.suffix != ".mp4" or destination.exists():
        raise ControlError("output must be a new .mp4 file")
    if args.stop_file and args.stop_file.exists():
        raise ControlError("stop file already exists")
    for command in ("ffmpeg", "ffprobe"):
        if shutil.which(command) is None:
            raise ControlError(f"{command} is required; see CONTRIBUTING.md")
    manifest_path = args.session.resolve() / "session.json"
    manifest = json.loads(manifest_path.read_text())
    if manifest.get("status") != "ready" or manifest.get("cleanup", {}).get("complete"):
        raise ControlError(
            "recording requires a ready, running native session manifest"
        )
    socket = manifest["command_socket"]
    status = Control(socket, Deadline(10)).status()
    if (
        status.get("backend") != "native"
        or not status.get("active")
        or status.get("locked")
    ):
        raise ControlError("recording requires an active, unlocked native session")
    outputs = status.get("outputs", [])
    selected = next(
        (
            output
            for output in outputs
            if not args.monitor or output["name"] == args.monitor
        ),
        None,
    )
    if selected is None or args.width > selected["width"]:
        raise ControlError(
            "recording monitor is missing or encoded width exceeds native width"
        )
    destination.parent.mkdir(parents=True, exist_ok=True)
    artifacts = destination.with_suffix(".recording")
    artifacts.mkdir(mode=0o700)
    frames = Frames(artifacts / "frames")
    stop = threading.Event()
    provenance = {
        "version": 1,
        "session": str(manifest_path),
        "command_socket": socket,
        "started_unix_seconds": time.time(),
        "requested_fps": args.fps,
        "maximum_duration_seconds": args.duration,
        "encoded_width": args.width,
        "timestamps": "monotonic receive times, relative to capture start; not hardware presentation timestamps",
    }
    binaries = args.session / "binaries.json"
    if binaries.exists():
        provenance["binaries"] = json.loads(binaries.read_text())

    def ready(frame):
        value = {
            "event": "recording",
            "artifacts": str(artifacts),
            "first_frame": frame,
        }
        (artifacts / "ready.json").write_text(json.dumps(value, indent=2) + "\n")
        print(json.dumps(value), flush=True)

    handlers = {
        number: signal.getsignal(number) for number in (signal.SIGINT, signal.SIGTERM)
    }
    for number in handlers:
        signal.signal(number, lambda *_: stop.set())
    try:
        timing = collect(
            socket,
            frames,
            args.duration,
            args.fps,
            lambda: stop.is_set() or bool(args.stop_file and args.stop_file.exists()),
            ready,
            selected["name"],
        )
        provenance.update(timing)
        provenance.update(frames.summary())
        print(json.dumps({"event": "encoding", **frames.summary()}), flush=True)
        provenance["video"] = encode(
            frames.frames,
            timing["stopped_seconds"],
            destination,
            artifacts,
            args.fps,
            args.width,
        )
        with destination.open("rb") as video:
            provenance["sha256"] = hashlib.file_digest(video, "sha256").hexdigest()
        provenance["complete"] = True
    except BaseException as error:
        provenance.update(frames.summary())
        provenance.update({"complete": False, "error": str(error)})
        raise
    finally:
        for number, handler in handlers.items():
            signal.signal(number, handler)
        (artifacts / "recording.json").write_text(
            json.dumps(provenance, indent=2) + "\n"
        )
    print(
        json.dumps(
            {
                "event": "complete",
                "video": str(destination),
                "artifacts": str(artifacts),
                **provenance["video"],
            }
        ),
        flush=True,
    )
    return 0


def main(argv=None):
    try:
        return run(parser().parse_args(argv))
    except (ControlError, OSError, ValueError, KeyError) as error:
        print(f"recording failed: {error}", file=sys.stderr)
        return 1
