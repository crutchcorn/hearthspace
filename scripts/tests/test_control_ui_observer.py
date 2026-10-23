"""The control workflow waits for the drag endpoint before keyboard input."""

import sys
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib import control_ui
from agentlib.control import ControlError, Deadline
from agentlib.smoke import Smoke


class SliderComplete(Exception):
    """Stop after the real workflow records its pointer and keyboard evidence."""


class ObserverWorkflow:
    until = Smoke.until

    def __init__(self, *, backend_delay=0.65, label_delay=0):
        self.now = 0.0
        self.backend_delay = backend_delay
        self.label_delay = label_delay
        self.drag_started = False
        self.released_at = None
        self.right_at = None
        self.motions = []
        self.audio_reads = []
        self.label_reads = []
        self.captures = {}
        self.records = []
        self.control = SimpleNamespace(
            path="/unused-control-observer.sock",
            status=self.status,
            request=self.request,
        )
        self.service_fixture = SimpleNamespace(status=self.audio)

    def status(self):
        return {
            "outputs": [{"scale": 2}],
            "shell_panels": [
                {"kind": "controls", "x": 948, "y": 85, "width": 320, "height": 175}
            ],
            "control_osds": [],
        }

    def elapsed_since_release(self):
        return self.now - self.released_at if self.released_at is not None else 0

    def audio(self):
        if self.right_at is not None:
            volume = 76
        elif not self.drag_started:
            volume = 40
        elif (
            self.released_at is not None
            and self.elapsed_since_release() >= self.backend_delay
        ):
            volume = 75
        else:
            volume = 69
        self.audio_reads.append((self.now, volume))
        maximum = round(volume * 65536 / 100)
        return {
            "volume": volume,
            "muted": False,
            "raw_volumes": [round(maximum / 2), maximum],
        }

    def a11y(self, _smoke, action, query):
        assert action == "find"
        slider = {"bounds": {"x": 142, "y": 170, "width": 400, "height": 56}}
        target = control_ui.selector("Volume", role="slider")
        if query in (target, f"{target}[@focused='true']"):
            return {"elements": [slider]}
        if self.right_at is not None:
            label = "76%"
        elif self.drag_started and self.elapsed_since_release() >= self.label_delay:
            # The UI can show the latest intent before the backend reaches it.
            label = "75%"
        else:
            label = "40%"
        if query == control_ui.selector(label):
            self.label_reads.append((self.now, label))
            return {"elements": [{"name": label}]}
        if query == control_ui.selector("Fixture speakers"):
            return {"elements": [{"name": "Fixture speakers"}]}
        return {"elements": []}

    def request(self, command):
        if command.startswith("pointer-motion-abs "):
            self.motions.append(tuple(map(float, command.split()[1:])))
        elif command == "pointer-button-down 272":
            self.drag_started = True
        elif command == "pointer-button-up 272":
            self.released_at = self.now
        elif command == "key-down 106":
            self.right_at = self.now
        elif command != "key-up 106":
            raise AssertionError(f"unexpected input: {command}")

    def sleep(self, duration):
        self.now += duration

    def capture(self, name):
        self.captures[name] = self.now

    def record(self, step, **data):
        self.records.append({"step": step, **data})
        if step == "volume-slider-input":
            raise SliderComplete

    def run(self):
        with (
            patch("time.monotonic", side_effect=lambda: self.now),
            patch("time.sleep", side_effect=self.sleep),
            patch.object(control_ui, "terminal"),
            patch.object(control_ui, "brightness_dropdown"),
            patch.object(control_ui, "open_sound"),
            patch.object(control_ui, "a11y", side_effect=self.a11y),
        ):
            self.control.deadline = Deadline(2)
            control_ui.run(self)


class ControlUiObserverTests(unittest.TestCase):
    def test_workflow_waits_past_quiet_intermediate_volume_for_drag_endpoint(self):
        workflow = ObserverWorkflow()
        with self.assertRaises(SliderComplete):
            workflow.run()

        self.assertEqual(workflow.motions[0], (1119, 184))
        self.assertEqual(workflow.motions[-1], (1169, 184))
        intermediate = [when for when, value in workflow.audio_reads if value == 69]
        self.assertGreater(intermediate[-1] - intermediate[0], 0.3)
        ready_at = workflow.released_at + workflow.backend_delay
        self.assertGreaterEqual(workflow.captures["sound-slider-pointer"], ready_at)
        self.assertGreaterEqual(workflow.right_at, ready_at)
        self.assertTrue(
            any(
                ready_at <= when <= workflow.right_at and label == "75%"
                for when, label in workflow.label_reads
            )
        )
        self.assertIn("sound-slider-keyboard", workflow.captures)
        self.assertEqual(
            workflow.records[-1],
            {
                "step": "volume-slider-input",
                "pointer": 75,
                "keyboard": 76,
                "raw_channels": [24904, 49807],
            },
        )

    def test_visible_endpoint_cannot_replace_backend_readback(self):
        workflow = ObserverWorkflow(backend_delay=float("inf"))
        with self.assertRaisesRegex(
            ControlError, "timed out waiting for audio 75% muted=False"
        ):
            workflow.run()
        self.assertIsNone(workflow.right_at)
        self.assertNotIn("sound-slider-pointer", workflow.captures)
        self.assertEqual(workflow.control.deadline.end, 2)

    def test_backend_endpoint_cannot_replace_visible_label(self):
        workflow = ObserverWorkflow(label_delay=float("inf"))
        with self.assertRaisesRegex(
            ControlError, "timed out waiting for dragged volume label"
        ):
            workflow.run()
        self.assertIn(75, [value for _when, value in workflow.audio_reads])
        self.assertIsNone(workflow.right_at)
        self.assertNotIn("sound-slider-pointer", workflow.captures)
        self.assertEqual(workflow.control.deadline.end, 2)


if __name__ == "__main__":
    unittest.main()
