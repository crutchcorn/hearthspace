"""Bounded command transport, presented capture, and coordinate mapping."""

import hashlib
import json
import math
import socket
import time
from pathlib import Path

MAX_PAYLOAD = 128 * 1024 * 1024
PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"


class ControlError(Exception):
    pass


class Deadline:
    def __init__(self, seconds):
        if not math.isfinite(seconds) or seconds <= 0:
            raise ControlError("timeout must be a positive finite number")
        self.end = time.monotonic() + seconds

    def remaining(self):
        remaining = self.end - time.monotonic()
        if remaining <= 0:
            raise ControlError("operation deadline exceeded")
        return remaining


class Control:
    def __init__(self, path, deadline):
        self.path = str(path)
        self.deadline = deadline

    def request(self, command):
        if "\n" in command or "\r" in command or len(command.encode()) > 4094:
            raise ControlError("command must be a single line of at most 4094 bytes")
        try:
            with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as stream:
                stream.settimeout(self.deadline.remaining())
                stream.connect(self.path)
                stream.settimeout(self.deadline.remaining())
                stream.sendall(command.encode() + b"\n")
                header = bytearray()
                while not header.endswith(b"\n"):
                    if len(header) >= 4096:
                        raise ControlError("oversized response header")
                    header.extend(self._read(stream, 1))
                line = header.decode("utf-8").rstrip("\n")
                if line.startswith("err "):
                    raise ControlError(line[4:])
                if line == "ok":
                    return b""
                parts = line.split(" ")
                if len(parts) != 2 or parts[0] != "ok" or not parts[1].isdigit():
                    raise ControlError(f"invalid response header: {line!r}")
                size = int(parts[1])
                if size > MAX_PAYLOAD:
                    raise ControlError("response exceeds 128 MiB limit")
                payload = bytearray()
                while len(payload) < size:
                    payload.extend(self._read(stream, min(size - len(payload), 65536)))
                return bytes(payload)
        except (OSError, UnicodeError) as error:
            raise ControlError(f"control socket {self.path}: {error}") from error

    def _read(self, stream, count):
        stream.settimeout(self.deadline.remaining())
        data = stream.recv(count)
        if not data:
            raise ControlError("control socket disconnected before complete reply")
        return data

    def status(self):
        try:
            return json.loads(self.request("status"))
        except ValueError as error:
            raise ControlError(f"invalid status JSON: {error}") from error

    def capture(self, output=None):
        command = "capture" if output is None else f"capture {output}"
        payload = self.request(command)
        try:
            metadata, png = payload.split(b"\n", 1)
            metadata = json.loads(metadata)
            if not png.startswith(PNG_SIGNATURE):
                raise ValueError("capture is not a PNG")
            if not all(
                key in metadata for key in ("frame_id", "output", "width", "height")
            ):
                raise ValueError("capture metadata is incomplete")
            return metadata, png
        except ValueError as error:
            raise ControlError(f"invalid capture payload: {error}") from error

    def wait_change(self, reference, output=None):
        baseline = hashlib.sha256(reference).digest()
        while True:
            metadata, png = self.capture(output)
            if hashlib.sha256(png).digest() != baseline:
                return metadata, png
            time.sleep(min(0.05, self.deadline.remaining()))


def save_capture(capture, destination):
    metadata, png = capture
    path = Path(destination).resolve()
    if path.suffix != ".png":
        raise ControlError("screenshot destination must end in .png")
    path.parent.mkdir(parents=True, exist_ok=True)
    metadata = {**metadata, "sha256": hashlib.sha256(png).hexdigest()}
    path.write_bytes(png)
    sidecar = path.with_suffix(".json")
    sidecar.write_text(json.dumps(metadata, indent=2) + "\n")
    return {"image": str(path), "metadata": str(sidecar), "capture": metadata}


def pixel_to_logical(x, y, metadata):
    """PNG pixels are in displayed orientation; transform is already applied."""
    width, height = metadata["width"], metadata["height"]
    scale = metadata["scale"]
    if not all(math.isfinite(v) for v in (x, y, width, height, scale)) or scale <= 0:
        raise ControlError("coordinates and dimensions must be finite")
    if not (0 <= x < width and 0 <= y < height):
        raise ControlError("pixel coordinates are outside the captured output")
    return (
        metadata["logical_x"] + x / scale,
        metadata["logical_y"] + y / scale,
    )
