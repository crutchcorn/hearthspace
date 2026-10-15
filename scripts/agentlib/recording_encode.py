"""Encode recorded frames without speeding up sparse or stalled capture."""

import json
import math
import subprocess
from pathlib import Path

from .control import ControlError


def concat_text(frames, stopped):
    if not frames or stopped <= frames[-1]["received_seconds"]:
        raise ControlError("recording end must follow its final frame")
    lines = ["ffconcat version 1.0"]
    for index, frame in enumerate(frames):
        after = (
            frames[index + 1]["received_seconds"]
            if index + 1 < len(frames)
            else stopped
        )
        duration = after - frame["received_seconds"]
        if not math.isfinite(duration) or duration <= 0:
            raise ControlError("recording frame durations must be positive")
        path = frame["file"]
        if "\n" in path or "\r" in path:
            raise ControlError("recording paths cannot contain newlines")
        quoted = path.replace("'", "'\\''")
        lines.extend(
            [f"file '{quoted}'", "option framerate 1000", f"duration {duration:.9f}"]
        )
    # The concat demuxer needs a following frame to honor the final duration.
    lines.extend([f"file '{quoted}'", "option framerate 1000"])
    return "\n".join(lines) + "\n"


def encode(frames, stopped, destination, artifacts, fps, width):
    destination, artifacts = Path(destination), Path(artifacts)
    timeline = artifacts / "frames.ffconcat"
    timeline.write_text(concat_text(frames, stopped))
    duration = stopped - frames[0]["received_seconds"]
    temporary = artifacts / "encoded.mp4"
    command = [
        "ffmpeg",
        "-hide_banner",
        "-nostdin",
        "-y",
        "-f",
        "concat",
        "-safe",
        "0",
        "-i",
        str(timeline),
        "-an",
        "-vf",
        f"scale={width}:-2:flags=lanczos,setsar=1,fps={fps},tpad=stop_mode=clone:stop_duration=1",
        "-c:v",
        "libx264",
        "-preset",
        "medium",
        "-crf",
        "20",
        "-pix_fmt",
        "yuv420p",
        "-r",
        str(fps),
        "-fps_mode",
        "cfr",
        "-threads",
        "4",
        "-t",
        f"{duration:.9f}",
        "-movflags",
        "+faststart",
        str(temporary),
    ]
    with (artifacts / "ffmpeg.log").open("w") as log:
        try:
            result = subprocess.run(
                command,
                stdout=log,
                stderr=subprocess.STDOUT,
                timeout=max(120, duration * 10),
                check=False,
            )
        except subprocess.TimeoutExpired as error:
            raise ControlError("video encoding exceeded its deadline") from error
    if result.returncode:
        raise ControlError(f"video encoding failed; see {artifacts / 'ffmpeg.log'}")
    try:
        probe = subprocess.run(
            [
                "ffprobe",
                "-v",
                "error",
                "-show_streams",
                "-show_format",
                "-of",
                "json",
                str(temporary),
            ],
            capture_output=True,
            text=True,
            timeout=30,
            check=False,
        )
    except subprocess.TimeoutExpired as error:
        raise ControlError("video inspection exceeded its deadline") from error
    if probe.returncode:
        raise ControlError(f"cannot inspect encoded video: {probe.stderr.strip()}")
    metadata = json.loads(probe.stdout)
    (artifacts / "ffprobe.json").write_text(json.dumps(metadata, indent=2) + "\n")
    streams = metadata["streams"]
    if (
        len(streams) != 1
        or streams[0]["codec_name"] != "h264"
        or streams[0]["pix_fmt"] != "yuv420p"
        or streams[0]["width"] != width
        or streams[0]["r_frame_rate"] != f"{fps}/1"
        or abs(float(metadata["format"]["duration"]) - duration) > 2 / fps
    ):
        raise ControlError("encoded video does not match recording geometry or timing")
    # Never replace an existing user recording, even if it appeared mid-encode.
    destination.hardlink_to(temporary)
    temporary.unlink()
    return {
        "duration_seconds": duration,
        "bytes": destination.stat().st_size,
        "command": command,
    }
