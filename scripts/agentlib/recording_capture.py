"""Bounded, page-flip-confirmed frame collection for human-paced recordings."""

import concurrent.futures
import json
import math
import statistics
import time
from pathlib import Path

from .control import PNG_SIGNATURE, Control, ControlError, Deadline

MAX_BYTES = 8 * 1024**3
GEOMETRY = (
    "output",
    "width",
    "height",
    "scale",
    "logical_x",
    "logical_y",
    "logical_width",
    "logical_height",
    "transform",
)


class Frames:
    """Retain at most the configured byte budget, with stable output geometry."""

    def __init__(self, directory, budget=MAX_BYTES):
        self.directory = Path(directory)
        self.directory.mkdir(mode=0o700)
        self.budget = budget
        self.bytes = 0
        self.frames = []
        self.duplicates = 0
        self.geometry = None

    def append(self, started, received, capture):
        metadata, png = capture
        if (
            metadata.get("presentation") != "page-flip"
            or metadata.get("cursor") != "embedded"
            or metadata.get("cursor_sample") != "render"
            or not png.startswith(PNG_SIGNATURE)
        ):
            raise ControlError("recording requires native presented frames with cursor")
        if not all(key in metadata for key in (*GEOMETRY, "frame_id")):
            raise ControlError("recording capture metadata is incomplete")
        if (
            not all(math.isfinite(value) for value in (started, received))
            or not 0 <= started <= received
            or not isinstance(metadata["frame_id"], int)
            or metadata["frame_id"] <= 0
        ):
            raise ControlError("invalid recording timestamp or frame ID")
        geometry = {key: metadata[key] for key in GEOMETRY}
        if self.geometry is not None and geometry != self.geometry:
            raise ControlError("recording output geometry changed")
        self.geometry = geometry
        if self.frames and metadata["frame_id"] <= self.frames[-1]["frame_id"]:
            # Concurrent requests may share a page flip or complete out of order.
            self.duplicates += 1
            return False
        if self.frames and received <= self.frames[-1]["received_seconds"]:
            raise ControlError("recording timestamps did not advance")
        if self.bytes + len(png) > self.budget:
            raise ControlError("recording exceeds its 8 GiB frame storage budget")
        number = len(self.frames)
        path = self.directory / f"{number:06d}.png"
        path.write_bytes(png)
        record = {
            **metadata,
            "file": str(path),
            "requested_seconds": started,
            "received_seconds": received,
            "bytes": len(png),
        }
        with (self.directory.parent / "frames.jsonl").open("a") as journal:
            journal.write(json.dumps(record) + "\n")
        self.frames.append(record)
        self.bytes += len(png)
        return True

    def summary(self):
        elapsed = (
            self.frames[-1]["received_seconds"] - self.frames[0]["received_seconds"]
            if len(self.frames) > 1
            else 0
        )
        latencies = [
            frame["received_seconds"] - frame["requested_seconds"]
            for frame in self.frames
        ]
        return {
            "frames": len(self.frames),
            "bytes": self.bytes,
            "duplicate_or_reordered_frames": self.duplicates,
            "distinct_frames_per_second": (len(self.frames) - 1) / elapsed
            if elapsed
            else 0,
            "median_capture_seconds": statistics.median(latencies) if latencies else 0,
            "geometry": self.geometry,
        }


def collect(socket, frames, duration, fps, stop, ready, output=None):
    """Two in-flight requests maximum; timestamps describe received live frames.

    Completion timestamps preserve wall-clock pacing, including gaps. They are
    not hardware presentation timestamps. Each frame's metadata independently
    proves that its own page flip completed before it reached the recorder.
    """
    beginning = time.monotonic()
    next_frame = beginning
    end = beginning + duration
    skipped = 0
    pending = set()

    def capture():
        started = time.monotonic() - beginning
        result = Control(socket, Deadline(6)).capture(output)
        return started, time.monotonic() - beginning, result

    with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
        try:
            while time.monotonic() < end and not stop():
                finished = [future for future in pending if future.done()]
                results = sorted(
                    (future.result() for future in finished), key=lambda r: r[1]
                )
                pending.difference_update(finished)
                for result in results:
                    if frames.append(*result) and len(frames.frames) == 1:
                        ready(frames.frames[0])
                now = time.monotonic()
                if now >= next_frame:
                    if len(pending) < 2:
                        pending.add(pool.submit(capture))
                    else:
                        skipped += 1
                    # Keep an absolute cadence without accumulating polling
                    # delay, or burst-capturing missed slots after a slow frame.
                    slots = int((now - next_frame) * fps) + 1
                    skipped += slots - 1
                    next_frame += slots / fps
                time.sleep(min(0.004, max(0, min(end, next_frame) - time.monotonic())))
        finally:
            stopped = time.monotonic() - beginning
            for future in pending:
                future.cancel()
            # The context waits at most the six-second command deadline. No
            # native session or input device belongs to this read-only recorder.
    if not frames.frames:
        raise ControlError("recording stopped before receiving its first frame")
    return {"stopped_seconds": stopped, "busy_capture_slots": skipped}
