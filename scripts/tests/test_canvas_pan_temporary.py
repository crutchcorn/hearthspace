"""Temporary-pan driver contracts at the transport boundary, not a compositor."""

import sys
import unittest
from pathlib import Path
from unittest.mock import Mock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib import canvas_pan_temporary as temporary
from agentlib.control import ControlError
from test_agent_visual import capture


def status(x=37, y=-29):
    return {
        "viewport": {"x": x, "y": y, "scale": 1},
        "windows": [
            {
                "id": 7,
                "canvas_x": 117,
                "canvas_y": 71,
                "content_width": 160,
                "content_height": 100,
                "x": 117 - x,
                "y": 71 - y,
                "width": 160,
                "height": 124,
                "titlebar_height": 24,
            }
        ],
        "shell_panels": [],
    }


def picture(state):
    window = state["windows"][0]
    x, y = window["x"] + 20, window["y"] + 40
    return capture(
        {"width": 320, "height": 300, "scale": 1, "logical_x": 0, "logical_y": 0},
        [((x, y, x + 30, y + 10), (0, 255, 0))],
    )


class TemporaryPanTests(unittest.TestCase):
    def setUp(self):
        cleanup = patch("agentlib.input.Control")
        self.cleanup = cleanup.start().return_value
        self.addCleanup(cleanup.stop)
        blank = patch("agentlib.canvas_pan_fixture.blank", return_value=(600, 500))
        blank.start()
        self.addCleanup(blank.stop)

    def smoke(self):
        smoke = Mock(target=7)
        smoke.control.path = "/owned-temporary-pan-control"
        smoke.control.status.return_value = status()
        smoke.control.query.side_effect = lambda command: (
            {"canvas_tool": "select"} if command == "layout-status" else {"items": []}
        )
        smoke.capture.return_value = picture(status())
        return smoke

    def drag_transcript(self, meta_first):
        smoke = self.smoke()
        origin, active, continued = status(), status(-6, -2), status(-18, 6)
        final = continued if meta_first else active
        phases = [origin, origin, active]
        if meta_first:
            phases += [active, continued]
        phases += [final, final]
        # These fixed observations are deliberately independent of input calls.
        # Acknowledging input cannot manufacture a successful pan in this test.
        smoke.control.status.side_effect = [
            origin,
            *(state for phase in phases for state in (phase, phase)),
            final,
            final,
            final,
            final,
            final,
        ]
        captures = iter(picture(phase) for phase in [*phases, final, final])
        observations = {}

        def captured(name):
            observations[name] = [
                call.args[0] for call in smoke.control.request.call_args_list
            ]
            return next(captures)

        smoke.capture.side_effect = captured
        return smoke, observations

    def test_both_release_orders_are_observed_before_fresh_unmodified_drag(self):
        for meta_first in (True, False):
            with self.subTest(meta_first=meta_first):
                smoke, phases = self.drag_transcript(meta_first)
                temporary.drag(smoke, "gesture", meta_first=meta_first)
                self.assertEqual(phases["gesture-meta-down"][-1], "key-down 125")
                self.assertEqual(
                    phases["gesture-primary-down"][-1], "pointer-button-down 272"
                )
                stopped = phases["gesture-primary-up"]
                self.assertEqual(stopped[-2], "pointer-button-up 272")
                self.assertTrue(stopped[-1].startswith("pointer-motion-abs"))
                if meta_first:
                    continuing = phases["gesture-continued"]
                    self.assertIn("key-up 125", continuing)
                    self.assertNotIn("pointer-button-up 272", continuing)
                else:
                    self.assertNotIn("key-up 125", stopped)
                released = phases["gesture-released"]
                first, second = (
                    ("key-up 125", "pointer-button-up 272")
                    if meta_first
                    else ("pointer-button-up 272", "key-up 125")
                )
                self.assertLess(released.index(first), released.index(second))
                next_drag = phases["gesture-next-unmodified-held"][len(released) :]
                self.assertIn("pointer-button-down 272", next_drag)
                self.assertFalse(
                    any(command.startswith("key-") for command in next_drag)
                )
                self.cleanup.request.assert_not_called()

    def test_successful_input_ack_without_viewport_movement_is_rejected(self):
        smoke = self.smoke()
        with self.assertRaisesRegex(ControlError, "pan offset"):
            temporary.drag(smoke, "stationary", meta_first=False)
        self.assertEqual(
            [call.args[0] for call in self.cleanup.request.call_args_list],
            ["pointer-button-up 272", "key-up 125"],
        )

    def test_correct_viewport_with_stale_presented_marker_is_rejected(self):
        smoke, _ = self.drag_transcript(False)
        smoke.capture.side_effect = None
        smoke.capture.return_value = picture(status())
        with self.assertRaisesRegex(ControlError, "follow the pan"):
            temporary.drag(smoke, "stale-pixels", meta_first=False)

    def test_meta_first_must_continue_after_meta_release(self):
        smoke, _ = self.drag_transcript(True)
        # The fifth phase incorrectly retains the first movement's viewport.
        origin, active = status(), status(-6, -2)
        smoke.control.status.side_effect = [origin, *([origin] * 4), *([active] * 6)]
        with self.assertRaisesRegex(ControlError, "pan offset"):
            temporary.drag(smoke, "stopped-on-meta", meta_first=True)
        self.cleanup.request.assert_called_once_with("pointer-button-up 272")

    def test_primary_first_must_stop_while_meta_remains_held(self):
        smoke, _ = self.drag_transcript(False)
        origin, active, wrong = status(), status(-6, -2), status(-23, 11)
        smoke.control.status.side_effect = [
            origin,
            *([origin] * 4),
            active,
            active,
            wrong,
        ]
        with self.assertRaisesRegex(ControlError, "pan offset"):
            temporary.drag(smoke, "did-not-stop", meta_first=False)
        self.cleanup.request.assert_called_once_with("key-up 125")

    def test_next_unmodified_sequence_cannot_keep_temporary_pan(self):
        smoke, _ = self.drag_transcript(False)
        origin, final = status(), status(-6, -2)
        smoke.control.status.side_effect = [
            origin,
            *([origin] * 4),
            *([final] * 6),
            final,
            status(-29, -19),
        ]
        with self.assertRaisesRegex(ControlError, "pan offset"):
            temporary.drag(smoke, "sticky-tool", meta_first=False)
        self.cleanup.request.assert_called_once_with("pointer-button-up 272")

    def test_post_capture_status_rejects_late_tool_menu_or_geometry_change(self):
        for fault in ("tool", "menu", "geometry", "context"):
            with self.subTest(fault=fault):
                smoke = self.smoke()
                changed = status()
                if fault == "menu":
                    changed["shell_panels"] = [{"kind": "search"}]
                if fault == "geometry":
                    changed["windows"][0]["canvas_x"] += 1
                smoke.control.status.side_effect = [status(), changed]
                smoke.control.query.side_effect = [
                    {"canvas_tool": "select"},
                    {},
                    {"canvas_tool": "pan" if fault == "tool" else "select"},
                    {"context": {"x": 10}} if fault == "context" else {},
                ]
                expected = {
                    "tool": "changed the selected tool",
                    "menu": "Apps List",
                    "geometry": "application canvas geometry",
                    "context": "context menu",
                }[fault]
                with self.assertRaisesRegex(ControlError, expected):
                    temporary.observe(smoke, "meta-up", status(), (0, 0))
                smoke.capture.assert_called_once_with("meta-up")

    def test_capture_failure_drains_both_buttons_after_meta_and_escape_release(self):
        smoke = self.smoke()
        origin, active = status(), status(5, -49)
        smoke.control.status.side_effect = [
            origin,
            origin,
            origin,
            active,
            active,
            active,
        ]
        smoke.capture.side_effect = [
            picture(origin),
            picture(active),
            ControlError("page flip failed"),
        ]
        with self.assertRaisesRegex(ControlError, "page flip failed"):
            temporary.cancelled(smoke)
        # Meta and Escape have already been explicitly released at this phase.
        self.assertEqual(
            [call.args[0] for call in self.cleanup.request.call_args_list],
            ["pointer-button-up 273", "pointer-button-up 272"],
        )

    def test_escape_preserves_viewport_and_drains_repress_before_fresh_sequence(self):
        smoke = self.smoke()
        origin, active = status(), status(5, -49)
        smoke.window.return_value = active["windows"][0]
        smoke.control.status.side_effect = [origin, origin, origin, *([active] * 12)]
        smoke.capture.side_effect = [picture(origin), *([picture(active)] * 6)]
        # Stop at the separately covered fresh drag to inspect the entire drain.
        with patch("agentlib.canvas_pan_temporary.drag") as fresh:
            temporary.cancelled(smoke)
        commands = [call.args[0] for call in smoke.control.request.call_args_list]
        secondary_up = commands.index("pointer-button-up 273")
        self.assertLess(commands.index("key-up 1"), commands.index("key-up 125"))
        self.assertEqual(commands[:secondary_up].count("pointer-button-down 272"), 2)
        self.assertEqual(commands[:secondary_up].count("pointer-button-up 272"), 1)
        self.assertEqual(commands[-1], "pointer-button-up 272")
        fresh.assert_called_once_with(
            smoke, "canvas-temporary-fresh-after-cancel", meta_first=True
        )

    def test_escape_rollback_or_drain_motion_is_not_accepted(self):
        for incorrect in (status(), status(4, -49)):
            with self.subTest(viewport=incorrect["viewport"]):
                smoke = self.smoke()
                origin, active = status(), status(5, -49)
                smoke.control.status.side_effect = [
                    origin,
                    origin,
                    origin,
                    active,
                    active,
                    incorrect,
                ]
                smoke.capture.side_effect = [picture(origin), picture(active)]
                with self.assertRaisesRegex(ControlError, "pan offset"):
                    temporary.cancelled(smoke)

    def test_rejected_sequence_cannot_be_admitted_after_other_modifier_is_released(
        self,
    ):
        smoke = self.smoke()
        smoke.control.status.side_effect = [
            status(),
            status(),
            status(),
            status(22, -39),
        ]
        with self.assertRaisesRegex(ControlError, "pan offset"):
            temporary.rejected(smoke)
        commands = [call.args[0] for call in smoke.control.request.call_args_list]
        self.assertLess(
            commands.index("key-down 29"), commands.index("pointer-button-down 272")
        )
        self.assertIn("key-up 29", commands)
        self.assertNotIn("pointer-button-up 272", commands)

    def test_middle_and_late_meta_rejections_are_explicit_input_sequences(self):
        smoke = self.smoke()
        temporary.rejected(smoke)
        phases = {
            call.kwargs["name"]: call.kwargs["delta"]
            for call in smoke.record.call_args_list
        }
        self.assertIn("canvas-temporary-reject-middle-changed", phases)
        self.assertIn("canvas-temporary-reject-late-meta-changed", phases)
        commands = [call.args[0] for call in smoke.control.request.call_args_list]
        self.assertIn("pointer-button-down 274", commands)
        self.assertIn("pointer-button-up 274", commands)
        self.assertTrue(all(delta == (0, 0) for delta in phases.values()))


class TemporaryIconTargetTests(unittest.TestCase):
    def test_target_excludes_windows_shell_and_other_tiles(self):
        state = {
            "outputs": [
                {
                    "logical_x": 0,
                    "logical_y": 0,
                    "logical_width": 800,
                    "logical_height": 600,
                }
            ],
            "viewport": {"x": 0, "y": 0, "scale": 1},
            "windows": [{"x": 0, "y": 0, "width": 300, "height": 300}],
            "shell_panels": [{"x": 400, "y": 100, "width": 100, "height": 100}],
        }
        desktop = {
            "items": [
                {"id": "covered-client", "x": 100, "y": 100},
                {"id": "covered-panel", "x": 400, "y": 100},
                {"id": "clear", "x": 600, "y": 400},
            ]
        }
        item, point = temporary.icon_target(state, desktop)
        self.assertEqual(item["id"], "clear")
        self.assertEqual(point, (644, 448))
        with self.assertRaisesRegex(ControlError, "no observed desktop icon"):
            temporary.icon_target(state, {"items": desktop["items"][:2]})


if __name__ == "__main__":
    unittest.main()
