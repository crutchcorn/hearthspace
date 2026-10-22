"""Registration may precede native tray mapping; later mapping loss is a failure."""

import sys
import unittest
from copy import deepcopy
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import Mock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib import tray_overflow_checks as checks
from agentlib.control import ControlError
from agentlib.smoke import Smoke
from agentlib.tray_overflow_fixture import title


def native_snapshot(*, mapped, edge="top"):
    rect = {"x": 24, "y": 24, "width": 1872, "height": 54}
    if edge == "left":
        rect.update(width=54, height=1032)
    if not mapped:
        rect = dict.fromkeys(rect, 0)
    widget = {
        "id": "tray",
        "group": None,
        "merged": False,
        "hidden": False,
        "orientation": "vertical" if mapped and edge == "left" else "horizontal",
        "rect": rect,
    }
    status = {
        "outputs": [
            {
                "name": "eDP-1",
                "logical_x": 0,
                "logical_y": 0,
                "logical_width": 1920,
                "logical_height": 1080,
                "scale": 1,
            }
        ],
        "shell_corners": [],
        "viewport": {"x": -1, "y": 1, "scale": 1},
    }
    layout = {
        "outputs": [
            {
                "output": "eDP-1",
                "widgets": [widget],
                "layout": {
                    "placements": {"widget:tray": {"surface": "frame", "edge": edge}}
                },
            }
        ]
    }
    return status, layout


def harness(layouts, *, status=None):
    status = status or native_snapshot(mapped=False)[0]
    control = Mock()
    control.status.return_value = status
    control.query.side_effect = layouts
    # A zero remaining interval avoids sleeping while using the real wait loop.
    # The separate expiration case proves failure still uses this same deadline.
    control.deadline.remaining.return_value = 0
    smoke = SimpleNamespace(control=control)
    smoke.until = lambda name, condition: Smoke.until(smoke, name, condition)
    return smoke


def icons(count=59, edge="top"):
    return [
        {
            "name": title(index),
            "bounds": {
                "x": 5 if edge == "left" else 5 + index * 38,
                "y": 5 + index * 38 if edge == "left" else 5,
                "width": 34,
                "height": 34,
            },
        }
        for index in range(count)
    ]


class TrayMappingReadinessTests(unittest.TestCase):
    def test_initial_wait_accepts_native_clipped_subset_after_unmapped_and_fitting(
        self,
    ):
        status, unmapped = native_snapshot(mapped=False)
        _, mapped = native_snapshot(mapped=True)
        smoke = harness([unmapped, mapped, mapped], status=status)
        deadline = smoke.control.deadline
        # Native evidence: 000–049 plus 058, with 050–057 pruned by AccessKit.
        clipped = icons()[:50] + icons()[-1:]
        with patch.object(
            checks,
            "a11y",
            side_effect=[
                {"elements": icons(8)},
                {"elements": clipped},
            ],
        ) as accessibility:
            observed = checks.wait_for_overflow(smoke, "top", 59)
        self.assertTrue(checks.require_overflow(observed, 59))
        self.assertEqual(observed["icons"], clipped)
        self.assertEqual(len(observed["visible"]), 49)
        self.assertEqual(observed["surface"]["width"], 1872)
        self.assertEqual(accessibility.call_count, 2)
        self.assertEqual(smoke.control.query.call_count, 3)
        self.assertIs(smoke.control.deadline, deadline)

    def test_left_setup_waits_for_mapping_before_asserting_orientation(self):
        _, unmapped = native_snapshot(mapped=False, edge="left")
        _, mapped = native_snapshot(mapped=True, edge="left")
        smoke = harness([unmapped, mapped])
        with patch.object(
            checks, "a11y", return_value={"elements": icons(edge="left")}
        ):
            observed = checks.wait_for_overflow(smoke, "left", 59)
        self.assertTrue(checks.require_overflow(observed, 59))
        self.assertEqual(observed["surface"]["height"], 1032)

    def test_never_mapped_tray_expires_on_existing_deadline(self):
        _, unmapped = native_snapshot(mapped=False)
        smoke = harness([unmapped])
        deadline = smoke.control.deadline
        deadline.remaining.side_effect = ControlError("budget expired")
        with self.assertRaisesRegex(
            ControlError, "timed out waiting for standalone tray mapped"
        ):
            checks.wait_for_overflow(smoke, "top", 59)
        self.assertIs(smoke.control.deadline, deadline)
        deadline.remaining.assert_called_once()

    def test_mapping_loss_after_readiness_remains_an_immediate_failure(self):
        _, unmapped = native_snapshot(mapped=False)
        smoke = harness([unmapped])
        with self.assertRaisesRegex(ControlError, "tray frame widget is not mapped"):
            checks.revealed(smoke, "top", title(0))
        smoke.control.deadline.remaining.assert_not_called()

    def test_initial_wait_does_not_hide_bad_layout_or_geometry(self):
        _, unmapped = native_snapshot(mapped=False)
        for changes in (
            {"hidden": True},
            {"group": "system"},
            {"merged": True},
            {"rect": {"x": 0, "y": 0, "width": -1, "height": 0}},
            {"rect": {"x": 0, "y": 0, "width": float("nan"), "height": 0}},
        ):
            layout = deepcopy(unmapped)
            layout["outputs"][0]["widgets"][0].update(changes)
            smoke = harness([layout])
            with self.subTest(changes=changes), self.assertRaises(ControlError):
                checks.wait_for_overflow(smoke, "top", 59)
            smoke.control.deadline.remaining.assert_not_called()

    def test_initial_wait_rejects_wrong_edge_and_duplicate_tray_widgets(self):
        _, wrong_edge = native_snapshot(mapped=False, edge="left")
        _, duplicated = native_snapshot(mapped=False)
        duplicated["outputs"][0]["widgets"] *= 2
        for layout in (wrong_edge, duplicated):
            smoke = harness([layout])
            with self.subTest(layout=layout), self.assertRaises(ControlError):
                checks.wait_for_overflow(smoke, "top", 59)
            smoke.control.deadline.remaining.assert_not_called()


if __name__ == "__main__":
    unittest.main()
