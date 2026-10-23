"""Live shell evidence rejects still-visible corners and unrelated pixel changes."""

import sys
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import Mock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib import csd_shell
from agentlib.control import ControlError
from test_agent_visual import capture


class FocusedShellEvidenceTests(unittest.TestCase):
    def setUp(self):
        self.metadata = {
            "output": "native",
            "frame_id": 1,
            "width": 400,
            "height": 300,
            "scale": 2,
            "logical_x": -100,
            "logical_y": 50,
        }
        self.output = {
            "name": "native",
            "logical_x": -100,
            "logical_y": 50,
            "logical_width": 200,
            "logical_height": 150,
            "scale": 2,
        }
        self.surfaces = {
            "notifications": {"x": -90, "y": 60, "width": 40, "height": 20},
            "system": {"x": 40, "y": 60, "width": 50, "height": 20},
            "canvas": {"x": -90, "y": 170, "width": 50, "height": 20},
            "apps": {"x": 60, "y": 170, "width": 30, "height": 20},
        }
        self.rectangles = [
            (
                (
                    rect["x"],
                    rect["y"],
                    rect["x"] + rect["width"],
                    rect["y"] + rect["height"],
                ),
                (245, 244, 240),
            )
            for rect in self.surfaces.values()
        ]
        self.visible = capture(self.metadata, self.rectangles)
        self.hidden = capture(self.metadata | {"frame_id": 2}, [])

    def test_requires_each_observed_shell_region_to_disappear(self):
        proof = csd_shell.hidden_regions(self.visible, self.hidden, self.surfaces)
        self.assertEqual(set(proof), set(self.surfaces))
        stuck = capture(self.metadata | {"frame_id": 2}, self.rectangles[-1:])
        with self.assertRaisesRegex(ControlError, "apps shell region"):
            csd_shell.hidden_regions(self.visible, stuck, self.surfaces)

    def test_frozen_frame_and_clock_or_cursor_changes_cannot_prove_hidden(self):
        tick = capture(
            self.metadata | {"frame_id": 2},
            self.rectangles + [((45, 62, 47, 64), (0, 0, 0))],
        )
        for image in (self.visible, tick):
            with (
                self.subTest(frozen=image is self.visible),
                self.assertRaises(ControlError),
            ):
                csd_shell.hidden_regions(self.visible, image, self.surfaces)
        with self.assertRaisesRegex(ControlError, "no observed regions"):
            csd_shell.hidden_regions(self.visible, self.hidden, {})

    @staticmethod
    def frame(image, frame_id):
        return image[0] | {"frame_id": frame_id}, image[1]

    def observer(self, images):
        window = {
            "focused": True,
            "focused_view": True,
            "fullscreen": False,
            "x": -100,
            "y": 50,
            "width": 200,
            "height": 150,
        }
        samples = []
        events = []
        frames = iter(images)

        def until(_label, observe):
            for _ in range(4):
                samples.append(value := observe())
                if value:
                    return value
            raise ControlError("bounded observer expired")

        def capture_frame(label):
            events.append(label)
            return next(frames)

        smoke = SimpleNamespace(
            target=7,
            record=Mock(),
            window=lambda _id: window,
            until=until,
            control=SimpleNamespace(
                path="unused",
                request=Mock(side_effect=lambda command: events.append(command)),
                deadline=SimpleNamespace(remaining=lambda: 30),
                status=lambda: {
                    "outputs": [self.output],
                    "shell_corners": [],
                    "shell_panels": [],
                    "canvas_maps": [],
                },
            ),
            capture=Mock(side_effect=capture_frame),
        )
        self.enterContext(
            patch.object(
                csd_shell, "a11y", return_value={"elements": [{"name": "Canvas map"}]}
            )
        )
        self.enterContext(
            patch.object(
                csd_shell.csd_shell_regions, "surfaces", return_value=self.surfaces
            )
        )
        return smoke, events, samples

    def test_observer_proves_initial_hide_and_waits_for_hide_after_hover(self):
        smoke, events, samples = self.observer(
            [
                self.frame(self.hidden, 1),
                self.frame(self.visible, 2),
                self.frame(self.visible, 3),
                self.frame(self.hidden, 4),
            ]
        )
        csd_shell.focused_hidden(smoke, "focused")
        self.assertIsNone(samples[1])
        self.assertEqual(set(samples[2]), set(self.surfaces))
        self.assertEqual(smoke.record.call_args.args[0], "csd-shell-hidden")
        self.assertEqual(
            set(smoke.record.call_args.kwargs["initial_changed_fractions"]),
            set(self.surfaces),
        )
        self.assertEqual(smoke.capture.call_count, 4)
        self.assertEqual(events[0], "focused-shell-entry")
        entry = smoke.record.call_args_list[0]
        self.assertEqual(entry.args[0], "csd-shell-entry")
        self.assertEqual(entry.kwargs["status"]["outputs"], [self.output])

    def test_initially_visible_shell_fails_even_if_hover_exit_would_hide_it(self):
        smoke, events, _ = self.observer(
            [
                self.frame(self.visible, 1),
                self.frame(self.visible, 2),
                self.frame(self.hidden, 3),
            ]
        )
        with self.assertRaisesRegex(
            ControlError, "initial focused shell was not hidden"
        ):
            csd_shell.focused_hidden(smoke, "focused")
        self.assertEqual(smoke.capture.call_count, 2)
        self.assertEqual(events[0], "focused-shell-entry")
        self.assertNotIn("focused-shell-hidden", events)

    def test_existing_touch_grace_expires_without_corrective_input_before_baseline(
        self,
    ):
        smoke, events, _ = self.observer(
            [
                self.frame(self.visible, 1),
                self.frame(self.hidden, 2),
                self.frame(self.visible, 3),
                self.frame(self.hidden, 4),
            ]
        )
        smoke.csd_shell_grace_until = 12
        with (
            patch.object(csd_shell.time, "monotonic", return_value=10),
            patch.object(
                csd_shell.time,
                "sleep",
                side_effect=lambda seconds: events.append(("passive-wait", seconds)),
            ),
        ):
            csd_shell.focused_hidden(smoke, "focused")
        self.assertEqual(
            events[:3],
            [
                "focused-shell-entry",
                ("passive-wait", 2),
                "focused-shell-entry-after-grace",
            ],
        )
        self.assertTrue(events[3].startswith("pointer-motion-abs"))
        self.assertEqual(smoke.record.call_args.args[0], "csd-shell-hidden")

    def test_expired_grace_does_not_excuse_a_still_visible_initial_shell(self):
        smoke, events, _ = self.observer(
            [
                self.frame(self.visible, 1),
                self.frame(self.visible, 2),
                self.frame(self.visible, 3),
                self.frame(self.hidden, 4),
            ]
        )
        smoke.csd_shell_grace_until = 12
        with (
            patch.object(csd_shell.time, "monotonic", return_value=10),
            patch.object(csd_shell.time, "sleep"),
            self.assertRaisesRegex(
                ControlError, "initial focused shell was not hidden"
            ),
        ):
            csd_shell.focused_hidden(smoke, "focused")
        self.assertNotIn("focused-shell-hidden", events)

    def test_pre_entry_parking_uses_output_origin_and_sets_only_existing_grace(self):
        smoke, events, _ = self.observer([])
        with patch.object(csd_shell.time, "monotonic", return_value=10):
            csd_shell.park_before_entry(smoke)
        self.assertEqual(events, ["pointer-motion-abs 0.0 125.0"])
        self.assertEqual(smoke.csd_shell_grace_until, 10.45)

    def test_fullscreen_cannot_stand_in_for_focused_shell_hiding(self):
        smoke = SimpleNamespace(
            target=7,
            window=lambda _id: {"focused_view": True, "fullscreen": True},
            control=SimpleNamespace(status=lambda: {"outputs": [self.output]}),
        )
        with self.assertRaisesRegex(ControlError, "non-fullscreen focused"):
            csd_shell.focused_hidden(smoke, "wrong-mode")


if __name__ == "__main__":
    unittest.main()
