import json
import socket
import sys
import tempfile
import threading
import time
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib.control import (
    PNG_SIGNATURE,
    Control,
    ControlError,
    Deadline,
    pixel_to_logical,
)


class Server:
    def __init__(self, handler):
        self.directory = tempfile.TemporaryDirectory()
        self.path = Path(self.directory.name) / "control.sock"
        self.socket = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.socket.bind(str(self.path))
        self.socket.listen()
        self.handler = handler

    def __enter__(self):
        def serve():
            with self.socket.accept()[0] as client:
                client.recv(4096)
                try:
                    self.handler(client)
                except BrokenPipeError:
                    pass

        self.thread = threading.Thread(target=serve, daemon=True)
        self.thread.start()
        return Control(self.path, Deadline(0.3))

    def __exit__(self, *_):
        self.thread.join(1)
        self.socket.close()
        self.directory.cleanup()


class TransportTests(unittest.TestCase):
    def test_fragmented_large_payload(self):
        payload = bytes(range(256)) * 1000

        def reply(client):
            for byte in f"ok {len(payload)}\n".encode():
                client.sendall(bytes([byte]))
            for offset in range(0, len(payload), 173):
                client.sendall(payload[offset : offset + 173])

        with Server(reply) as control:
            self.assertEqual(control.request("screenshot"), payload)

    def test_disconnect_is_not_a_successful_short_read(self):
        with (
            Server(lambda client: client.sendall(b"ok 100\nshort")) as control,
            self.assertRaisesRegex(ControlError, "disconnected"),
        ):
            control.request("screenshot")

    def test_absolute_deadline_survives_slow_drip(self):
        def reply(client):
            client.sendall(b"ok 100\n")
            for _ in range(10):
                time.sleep(0.06)
                client.sendall(b"a")

        with Server(reply) as control:
            started = time.monotonic()
            with self.assertRaises(ControlError):
                control.request("screenshot")
            self.assertLess(time.monotonic() - started, 0.5)

    def test_error_and_oversized_replies(self):
        for response, error in [
            (b"err inactive output\n", "inactive output"),
            (b"ok 999999999\n", "128 MiB"),
            (b"ok nope\n", "invalid response"),
        ]:
            with (
                self.subTest(response=response),
                Server(lambda c, response=response: c.sendall(response)) as control,
                self.assertRaisesRegex(ControlError, error),
            ):
                control.request("capture")

    def test_capture_metadata_and_png_are_separated(self):
        meta = {"frame_id": 4, "output": "DP-1", "width": 2, "height": 1}
        payload = json.dumps(meta).encode() + b"\n" + PNG_SIGNATURE
        with Server(
            lambda c: c.sendall(f"ok {len(payload)}\n".encode() + payload)
        ) as control:
            self.assertEqual(control.capture("DP-1"), (meta, PNG_SIGNATURE))

    def test_no_newline_command_injection(self):
        with self.assertRaises(ControlError):
            Control("missing", Deadline(1)).request("status\nquit")

    def test_pixel_mapping_handles_scale_offset_and_displayed_rotation(self):
        meta = {
            "width": 2160,
            "height": 3840,
            "logical_width": 1080,
            "logical_height": 1920,
            "logical_x": -1080,
            "logical_y": 50,
            "transform": "90",
            "scale": 2,
        }
        self.assertEqual(pixel_to_logical(200, 300, meta), (-980, 200))
        for coords in [(2160, 0), (-1, 0), (float("nan"), 0)]:
            with self.assertRaises(ControlError):
                pixel_to_logical(*coords, meta)

    def test_change_wait_has_deadline_and_accepts_changed_image(self):
        control = Control("unused", Deadline(0.05))
        control.capture = lambda output: ({"frame_id": 1}, b"same")
        with self.assertRaisesRegex(ControlError, "deadline"):
            control.wait_change(b"same")
        control = Control("unused", Deadline(1))
        control.capture = lambda output: ({"frame_id": 2}, b"new")
        self.assertEqual(control.wait_change(b"old")[1], b"new")


if __name__ == "__main__":
    unittest.main()
