import json
import sys
import tempfile
import threading
import time
import unittest
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib.control import PNG_SIGNATURE, ControlError
from agentlib.recording import parser, run
from agentlib.recording_capture import Frames, collect
from agentlib.recording_encode import concat_text, encode


def capture(frame_id=1, **changes):
    return {
        "output": "eDP-1",
        "width": 1280,
        "height": 800,
        "scale": 2,
        "logical_x": 0,
        "logical_y": 0,
        "logical_width": 640,
        "logical_height": 400,
        "transform": "normal",
        "frame_id": frame_id,
        "presentation": "page-flip",
        "cursor": "embedded",
        "cursor_sample": "render",
        "cursor_source": "hardware-overlay",
        **changes,
    }, PNG_SIGNATURE + b"fixture"


class RecordingTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.path = Path(self.temporary.name)
        self.frames = Frames(self.path / "frames")

    def test_deduplicates_page_flips_without_rewriting_their_timestamps(self):
        self.assertTrue(self.frames.append(0.1, 0.2, capture(10)))
        self.assertFalse(self.frames.append(0.2, 0.3, capture(10)))
        self.assertFalse(self.frames.append(0.3, 0.4, capture(9)))
        self.assertTrue(self.frames.append(0.4, 0.7, capture(11)))
        self.assertEqual(self.frames.summary()["duplicate_or_reordered_frames"], 2)
        self.assertAlmostEqual(self.frames.summary()["distinct_frames_per_second"], 2)
        self.assertEqual(len(list((self.path / "frames").glob("*.png"))), 2)
        records = [
            json.loads(line)
            for line in (self.path / "frames.jsonl").read_text().splitlines()
        ]
        self.assertEqual([record["received_seconds"] for record in records], [0.2, 0.7])

    def test_requires_native_page_flip_and_included_cursor(self):
        for change in (
            {"presentation": "offscreen"},
            {"cursor": "omitted"},
            {"cursor_sample": "unknown"},
            {"frame_id": 0},
        ):
            with self.subTest(change=change), self.assertRaises(ControlError):
                self.frames.append(0.1, 0.2, capture(**change))
        self.assertEqual(self.frames.frames, [])

    def test_output_resize_or_switch_aborts_recording(self):
        self.frames.append(0.1, 0.2, capture())
        for change in ({"width": 800}, {"output": "HDMI-1"}, {"scale": 1}):
            with (
                self.subTest(change=change),
                self.assertRaisesRegex(ControlError, "geometry changed"),
            ):
                self.frames.append(0.2, 0.3, capture(2, **change))

    def test_disk_budget_is_checked_before_writing(self):
        self.frames.budget = len(capture()[1])
        self.frames.append(0.1, 0.2, capture())
        with self.assertRaisesRegex(ControlError, "storage budget"):
            self.frames.append(0.2, 0.3, capture(2))
        self.assertEqual(len(list((self.path / "frames").iterdir())), 1)

    def test_rejects_invalid_timestamps(self):
        for started, received in ((-1, 1), (2, 1), (0, float("nan"))):
            with self.subTest(started=started), self.assertRaises(ControlError):
                self.frames.append(started, received, capture())
        self.frames.append(0.1, 0.2, capture())
        with self.assertRaisesRegex(ControlError, "timestamps did not advance"):
            self.frames.append(0.1, 0.2, capture(2))

    def test_sparse_frames_keep_their_real_time_duration(self):
        self.frames.append(0.1, 0.2, capture())
        self.frames.append(0.3, 0.4, capture(2))
        self.frames.append(1.0, 1.2, capture(3))
        text = concat_text(self.frames.frames, 2)
        self.assertIn("duration 0.200000000", text)
        self.assertEqual(text.count("duration 0.800000000"), 2)
        self.assertEqual(text.count("option framerate 1000"), 4)
        self.assertEqual(text.count("000002.png"), 2)
        with self.assertRaises(ControlError):
            concat_text(self.frames.frames, 1)

    def test_concat_quotes_paths_without_shell_interpretation(self):
        frame = {"file": "/tmp/a'b/$literal.png", "received_seconds": 1}
        self.assertIn("file '/tmp/a'\\''b/$literal.png'", concat_text([frame], 2))
        with self.assertRaises(ControlError):
            concat_text([{**frame, "file": "/tmp/bad\nfile.png"}], 2)

    def test_concurrent_capture_is_bounded_and_stop_waits_for_owned_requests(self):
        lock = threading.Lock()
        active = 0
        maximum = 0
        counter = 0
        ready = threading.Event()

        def read(_output):
            nonlocal active, maximum, counter
            with lock:
                active += 1
                maximum = max(maximum, active)
                counter += 1
                frame_id = counter
            time.sleep(0.03)
            with lock:
                active -= 1
            return capture(frame_id)

        with mock.patch("agentlib.recording_capture.Control") as control:
            control.return_value.capture.side_effect = read
            result = collect(
                "/owned/socket", self.frames, 1, 60, ready.is_set, lambda _: ready.set()
            )
        self.assertEqual(active, 0)
        self.assertLessEqual(maximum, 2)
        self.assertGreaterEqual(len(self.frames.frames), 1)
        self.assertLess(result["stopped_seconds"], 0.5)

    def test_lock_failure_stops_collection_and_reports_error(self):
        with mock.patch("agentlib.recording_capture.Control") as control:
            control.return_value.capture.side_effect = ControlError("session is locked")
            with self.assertRaisesRegex(ControlError, "session is locked"):
                collect(
                    "/owned/socket", self.frames, 1, 30, lambda: False, lambda _: None
                )
        self.assertEqual(self.frames.frames, [])

    def test_cli_refuses_invalid_duration_existing_output_and_non_native_session(self):
        arguments = parser().parse_args(
            ["--session", str(self.path), "--output", str(self.path / "video.mp4")]
        )
        arguments.duration = float("nan")
        with self.assertRaisesRegex(ControlError, "duration"):
            run(arguments)
        arguments.duration = 1
        arguments.output.write_bytes(b"existing recording")
        with self.assertRaisesRegex(ControlError, "new .mp4"):
            run(arguments)
        arguments.output.unlink()
        (self.path / "session.json").write_text(
            json.dumps({"status": "ready", "command_socket": "/owned/socket"})
        )
        with (
            mock.patch("agentlib.recording.shutil.which", return_value="/bin/fake"),
            mock.patch("agentlib.recording.Control") as control,
        ):
            control.return_value.status.return_value = {
                "backend": "headless",
                "active": True,
                "locked": False,
            }
            with self.assertRaisesRegex(ControlError, "native session"):
                run(arguments)

    def test_encoder_validates_output_and_preserves_an_existing_destination(self):
        self.frames.append(0, 0.1, capture())
        destination = self.path / "video.mp4"
        metadata = {
            "streams": [
                {
                    "codec_name": "h264",
                    "pix_fmt": "yuv420p",
                    "width": 1280,
                    "r_frame_rate": "30/1",
                }
            ],
            "format": {"duration": "1.0"},
        }

        def command(args, **_kwargs):
            if args[0] == "ffmpeg":
                Path(args[-1]).write_bytes(b"encoded")
                destination.write_bytes(b"user recording appeared")
                return mock.Mock(returncode=0)
            return mock.Mock(returncode=0, stdout=json.dumps(metadata))

        with (
            mock.patch("agentlib.recording_encode.subprocess.run", side_effect=command),
            self.assertRaises(FileExistsError),
        ):
            encode(self.frames.frames, 1.1, destination, self.path, 30, 1280)
        self.assertEqual(destination.read_bytes(), b"user recording appeared")


if __name__ == "__main__":
    unittest.main()
