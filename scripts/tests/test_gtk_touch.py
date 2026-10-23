"""GTK touch evidence must reject stale content and invalid app coordinates."""

import json
import os
import sys
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import Mock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib.control import ControlError
from agentlib.gtk_touch import application
from agentlib.gtk_touch_checks import client_state, local_center
from agentlib.gtk_touch_pixels import translated
from test_agent_visual import capture


class GtkTouchEvidenceTests(unittest.TestCase):
    def setUp(self):
        self.metadata = {
            "width": 620,
            "height": 520,
            "scale": 1,
            "logical_x": 0,
            "logical_y": 0,
        }
        self.before = {"x": 80, "y": 70, "width": 300, "height": 310}
        self.after = self.before | {"x": 125, "y": 95}
        self.image = self.window(self.before)
        self.captured = self.window(self.after)

    def window(self, state, *, blank=False, stale=False):
        def rectangles(window):
            x, y = window["x"], window["y"]
            values = [((x, y, x + 300, y + 310), (242, 242, 242))]
            if not blank:
                values.extend(
                    ((x + dx, y + dy, x + dx + 17, y + dy + 19), (12, 120, 25))
                    for dx in range(25, 280, 31)
                    for dy in range(80, 290, 29)
                )
            return values

        shapes = rectangles(self.before) if stale else []
        return capture(self.metadata, shapes + rectangles(state))

    def test_content_and_vacated_pixels_follow_reported_move(self):
        result = translated(self.before, self.after, self.image, self.captured)
        self.assertGreater(result["content_edge_samples"], 30)
        self.assertGreater(result["translated_fraction"], 0.98)
        self.assertGreater(result["vacated_changed_fraction"], 0.98)

    def test_geometry_without_moving_pixels_fails(self):
        with self.assertRaisesRegex(ControlError, "content did not follow"):
            translated(self.before, self.after, self.image, self.image)

    def test_uniform_window_cannot_supply_content_proof(self):
        with self.assertRaisesRegex(ControlError, "content did not follow"):
            translated(
                self.before,
                self.after,
                self.window(self.before, blank=True),
                self.window(self.after, blank=True),
            )

    def test_correct_translation_still_rejects_stale_old_window(self):
        with self.assertRaisesRegex(ControlError, "stale pixels"):
            translated(
                self.before, self.after, self.image, self.window(self.after, stale=True)
            )

    def test_output_scale_bounds_one_coherent_rounding_adjustment(self):
        self.metadata |= {"width": 1240, "height": 1040, "scale": 2}

        def striped(window):
            x, y = window["x"], window["y"]
            # One-physical-pixel stripes expose rounding errors that wide solid
            # rectangles can hide. Every feature shares the same translation.
            shapes = [((x, y, x + 300, y + 310), (242, 242, 242))]
            shapes.extend(
                (
                    (x + 15, y + 70 + row / 2, x + 285, y + 70 + (row + 1) / 2),
                    tuple(
                        25 + row * coefficient % 211 for coefficient in (53, 97, 139)
                    ),
                )
                for row in range(440)
            )
            return capture(self.metadata, shapes)

        image = striped(self.before)
        # Native status rounded the logical Y displacement to -35, while the
        # rendered surface moved -36: two physical pixels apart at output scale2.
        actual = self.after | {"y": self.after["y"] - 1}
        proof = translated(self.before, self.after, image, striped(actual))
        self.assertEqual(proof["geometry_rounding_adjustment"], [0, -2])
        self.assertGreater(proof["translated_fraction"], 0.98)
        outside = self.after | {"y": self.after["y"] - 5}
        with self.assertRaisesRegex(ControlError, "content did not follow"):
            translated(self.before, self.after, image, striped(outside))

    def test_local_bounds_use_canvas_scale_not_output_dpi(self):
        window = {"x": -200, "y": 150, "content_width": 600, "content_height": 400}
        self.assertEqual(local_center((120, 180, 100, 40), window, 0.8), (-64, 310))
        for bounds in (
            (-1, 0, 20, 20),
            (0, 0, 0, 30),
            (590, 20, 20, 20),
            (10, 390, 30, 40),
        ):
            with self.subTest(bounds=bounds), self.assertRaises(ControlError):
                local_center(bounds, window, 1)

    def test_gtk3_shadow_relative_bounds_use_observed_header_origin(self):
        # Actual GTK 3 AT-SPI geometry from the scale-2 native run: the whole
        # surface is 572x479 while xdg window geometry is 520x427 at (26,23).
        window = {"x": 216, "y": 128, "content_width": 520, "content_height": 427}
        button = (50, 135, 472, 34)
        with self.assertRaisesRegex(ControlError, "outside"):
            local_center(button, window, 1)
        self.assertEqual(local_center(button, window, 1, (26, 23)), (476, 257))
        self.assertEqual(local_center(button, window, 0.8, (26, 23)), (424, 231.2))


class GtkApplicationIdentityTests(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.process = Mock(pid=1234, returncode=None)
        self.process.poll.return_value = None
        self.smoke = SimpleNamespace(
            artifacts=Path(directory.name),
            manifest={
                "dbus_session_bus_address": "private-bus",
                "runtime_dir": "/private-runtime",
                "wayland_display": "private-wayland",
            },
            args=SimpleNamespace(
                titlebar_maximize=True,
                calculator=Path("/unused-calculator"),
                window_test=Path(directory.name) / "unused-window-test",
            ),
            control=Mock(),
            record=Mock(),
            window=Mock(return_value=None),
            until=self.until,
        )
        self.windows = []
        self.smoke.control.status.side_effect = lambda: {"windows": self.windows}
        self.candidates = lambda window: [window]
        self.state_change = {}
        self.write_state = True
        self.launch = self.enterContext(
            patch("agentlib.gtk_touch.subprocess.Popen", side_effect=self.launched)
        )
        self.enterContext(patch("agentlib.gtk_touch.checks.snapshot"))

    @staticmethod
    def until(label, condition):
        for _ in range(3):
            if value := condition():
                return value
        raise ControlError(f"timed out waiting for {label}")

    def launched(self, _command, *, env, **_kwargs):
        self.environment = env
        window = {
            "id": 7,
            "mapped": True,
            "titlebar_height": 0,
            "app_id": env["HEARTHSPACE_GTK_TOUCH_APP_ID"],
            "title": env["HEARTHSPACE_GTK_TOUCH_TITLE"],
        }
        self.windows = self.candidates(window)
        if self.write_state:
            state = {
                **self.smoke.client_identity,
                "pid": self.process.pid,
                "count": 0,
                "fullscreen": False,
                "maximized": False,
            } | self.state_change
            self.smoke.client_state.write_text(json.dumps(state))
        return self.process

    def test_selects_exact_fixture_after_unrelated_new_windows(self):
        self.candidates = lambda window: [
            window | {"id": 2, "app_id": "other"},
            window | {"id": 3, "title": "other"},
            window,
        ]
        with application(self.smoke, "gtk3"):
            self.assertEqual(self.smoke.target, 7)
            self.assertEqual(client_state(self.smoke)["pid"], self.process.pid)
        self.process.terminate.assert_called_once()

    def test_rejects_ambiguous_exact_windows_and_stops_owned_process(self):
        self.candidates = lambda window: [window, window | {"id": 8}]
        with (
            self.assertRaisesRegex(ControlError, "ambiguous"),
            application(self.smoke, "gtk4"),
        ):
            self.fail("ambiguous application was accepted")
        self.process.terminate.assert_called_once()

    def test_preexisting_window_is_never_selected(self):
        self.windows = [{"id": 7}]
        with (
            self.assertRaisesRegex(ControlError, "timed out"),
            application(self.smoke, "gtk3"),
        ):
            self.fail("old window was accepted")

    def test_unmapped_exact_window_is_never_selected(self):
        self.candidates = lambda window: [window | {"mapped": False}]
        with (
            self.assertRaisesRegex(ControlError, "timed out"),
            application(self.smoke, "gtk4"),
        ):
            self.fail("unmapped window was accepted")

    def test_wrong_app_or_title_cannot_supply_fixture_status(self):
        self.candidates = lambda window: [
            window | {"app_id": window["app_id"] + "-other"},
            window | {"id": 8, "title": window["title"] + "-other"},
        ]
        with (
            self.assertRaisesRegex(ControlError, "timed out"),
            application(self.smoke, "gtk3"),
        ):
            self.fail("unrelated application was accepted")

    def test_missing_status_never_establishes_gtk_ownership(self):
        self.write_state = False
        with (
            self.assertRaisesRegex(ControlError, "timed out"),
            application(self.smoke, "gtk4"),
        ):
            self.fail("unacknowledged GTK fixture was accepted")

    def test_another_process_cannot_supply_gtk_status(self):
        self.state_change = {"pid": 5678}
        with (
            self.assertRaisesRegex(ControlError, "does not belong"),
            application(self.smoke, "gtk4"),
        ):
            self.fail("unrelated process status was accepted")

    def test_reused_pid_cannot_supply_another_launch_status(self):
        self.state_change = {"fixture_id": "previous-launch"}
        with (
            self.assertRaisesRegex(ControlError, "does not belong"),
            application(self.smoke, "gtk3"),
        ):
            self.fail("stale launch status was accepted")

    def test_stopped_client_cannot_supply_cached_status(self):
        with application(self.smoke, "gtk3"):
            self.process.poll.return_value = 1
            self.process.returncode = 1
            with self.assertRaisesRegex(ControlError, "exited early"):
                client_state(self.smoke)

    def test_generic_status_uses_fresh_owned_path_without_gtk_metadata(self):
        self.state_change = {"pid": None, "fixture_id": None}
        stale = self.smoke.artifacts / "generic-state.json"
        stale.write_text('{"text":"stale evidence"}')
        with (
            patch.dict(os.environ, {"HEARTHSPACE_WINDOW_TEST_MAXIMIZED": "1"}),
            application(self.smoke, "generic"),
        ):
            self.assertNotEqual(self.smoke.client_state, stale)
            self.assertEqual(
                self.launch.call_args.args[0][1], str(self.smoke.client_state)
            )
            self.assertFalse(client_state(self.smoke)["maximized"])
            self.assertNotIn("HEARTHSPACE_WINDOW_TEST_MAXIMIZED", self.environment)
            self.assertEqual(
                self.environment["DBUS_SESSION_BUS_ADDRESS"], "private-bus"
            )
        self.assertEqual(json.loads(stale.read_text())["text"], "stale evidence")

    def test_calculator_does_not_require_instrumented_gtk_status(self):
        self.write_state = False
        with application(self.smoke, "calculator"):
            self.assertEqual(self.smoke.target, 7)
            self.assertFalse(self.smoke.client_state.exists())


if __name__ == "__main__":
    unittest.main()
