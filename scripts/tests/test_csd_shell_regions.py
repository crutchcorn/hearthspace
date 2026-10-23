"""Independent system controls cannot disappear inside aggregate hide evidence."""

import copy
import sys
import unittest
from pathlib import Path
from unittest.mock import Mock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib import csd_shell_regions
from agentlib.control import ControlError
from agentlib.csd_shell import hidden_regions
from test_agent_visual import capture


def rect(x, y, width, height):
    return {"x": x, "y": y, "width": width, "height": height}


def scene():
    status = {
        "outputs": [
            {
                "name": "offset",
                "logical_x": -100,
                "logical_y": 50,
                "logical_width": 200,
                "logical_height": 150,
                "scale": 1.5,
            }
        ],
        "shell_corners": [],
    }
    members = {
        "notifications": rect(-90, 60, 25, 20),
        "tools": rect(-90, 170, 45, 20),
        "apps": rect(60, 170, 30, 20),
        "clock": rect(0, 62, 20, 14),
        "wifi": rect(24, 62, 10, 14),
        "volume": rect(38, 62, 8, 8),
        "brightness": rect(50, 62, 8, 14),
        "battery": rect(62, 62, 8, 14),
        "tray": rect(-85, 100, 10, 10),
        "power": rect(76, 62, 10, 14),
    }
    layout = {
        "outputs": [
            {
                "output": "offset",
                "groups": [{"id": "system", "rect": rect(0, 60, 90, 20)}],
                "widgets": [
                    {"id": name, "hidden": False, "rect": bounds, "group": None}
                    for name, bounds in members.items()
                ],
            }
        ],
    }
    return status, layout


def smoke_with(status, layout):
    smoke = Mock()
    smoke.control.status.return_value = status
    smoke.control.query.return_value = layout
    return smoke


def legacy_scene():
    status, layout = scene()
    widgets = {item["id"]: item["rect"] for item in layout["outputs"][0]["widgets"]}
    groups = {
        "notifications": widgets["notifications"],
        "system": layout["outputs"][0]["groups"][0]["rect"],
        "canvas": widgets["tools"],
        "apps": widgets["apps"],
    }
    status["shell_corners"] = [
        bounds | {"output": "offset", "corner": name} for name, bounds in groups.items()
    ]
    elements = [
        {"role": "Frame", "bounds": rect(0, 0, 135, 30)},
        {"role": "Button", "name": "Sound settings", "bounds": rect(9, 3, 12, 12)},
        {
            "role": "ToggleButton",
            "name": "Wi-Fi settings",
            "bounds": rect(30, 3, 12, 12),
        },
        {
            "role": "Button",
            "name": "Battery: 72%, Charging",
            "bounds": rect(60, 3, 12, 12),
        },
        {"role": "Button", "name": "Chat tray", "bounds": rect(90, 3, 12, 12)},
    ]
    return status, elements


class LiveShellRegionTests(unittest.TestCase):
    def test_collects_all_live_members_and_retains_each_corner_group(self):
        status, layout = scene()
        smoke = smoke_with(status, layout)
        with patch.object(csd_shell_regions, "a11y") as accessibility:
            regions = csd_shell_regions.surfaces(smoke)
        accessibility.assert_not_called()
        smoke.control.query.assert_called_once_with("layout-status")
        self.assertEqual(
            set(regions),
            {
                "notifications",
                "system",
                "canvas",
                "apps",
                "system-clock",
                "system-wifi",
                "system-volume",
                "system-brightness",
                "system-battery",
                "system-tray",
                "system-power",
            },
        )
        self.assertEqual(
            regions["system"],
            rect(0, 60, 90, 20)
            | {
                "output": "offset",
                "corner": "system",
            },
        )
        for item in layout["outputs"][0]["widgets"]:
            key = f"system-{item['id']}"
            if key in regions:
                self.assertEqual(
                    regions[key],
                    item["rect"]
                    | {
                        "output": "offset",
                        "corner": "system",
                    },
                )
        # The tray is a standalone member outside the group, on an offset output.
        self.assertEqual(regions["system-tray"]["x"], -85)

    def test_optional_unavailable_or_hidden_widgets_do_not_invent_rectangles(self):
        status, layout = scene()
        for item in layout["outputs"][0]["widgets"]:
            if item["id"] == "battery":
                item["rect"] = rect(0, 0, 0, 0)
            elif item["id"] == "tray":
                item["rect"] = None
            elif item["id"] == "wifi":
                item["hidden"] = True
        regions = csd_shell_regions.surfaces(smoke_with(status, layout))
        self.assertNotIn("system-battery", regions)
        self.assertNotIn("system-tray", regions)
        self.assertNotIn("system-wifi", regions)
        self.assertIn("system-volume", regions)

    def test_volume_must_supply_independent_usable_live_geometry(self):
        for failure in (
            "missing",
            "hidden",
            "empty",
            "nonfinite",
            "duplicate",
            "outside",
        ):
            with self.subTest(failure=failure):
                status, layout = scene()
                widgets = layout["outputs"][0]["widgets"]
                volume = next(item for item in widgets if item["id"] == "volume")
                if failure == "missing":
                    widgets.remove(volume)
                elif failure == "hidden":
                    volume["hidden"] = True
                elif failure == "empty":
                    volume["rect"]["width"] = 0
                elif failure == "nonfinite":
                    volume["rect"]["x"] = float("nan")
                elif failure == "duplicate":
                    widgets.append(copy.deepcopy(volume))
                else:
                    volume["rect"]["x"] = 99
                with self.assertRaisesRegex(ControlError, "volume"):
                    csd_shell_regions.surfaces(smoke_with(status, layout))

    def test_wrong_output_cannot_supply_system_member_geometry(self):
        status, layout = scene()
        layout["outputs"][0]["output"] = "other"
        with self.assertRaisesRegex(ControlError, "active output"):
            csd_shell_regions.surfaces(smoke_with(status, layout))

    def test_small_stuck_volume_fails_when_all_other_shell_regions_hide(self):
        status, layout = scene()
        regions = csd_shell_regions.surfaces(smoke_with(status, layout))
        metadata = {
            "output": "offset",
            "frame_id": 1,
            "width": 300,
            "height": 225,
            "scale": 1.5,
            "logical_x": -100,
            "logical_y": 50,
        }

        def painted(surface):
            return (
                (
                    surface["x"],
                    surface["y"],
                    surface["x"] + surface["width"],
                    surface["y"] + surface["height"],
                ),
                (245, 244, 240),
            )

        ink = {
            name: (
                (
                    surface["x"] + surface["width"] / 4,
                    surface["y"] + surface["height"] / 4,
                    surface["x"] + surface["width"] * 3 / 4,
                    surface["y"] + surface["height"] * 3 / 4,
                ),
                (40, 45, 50),
            )
            for name, surface in regions.items()
            if name.startswith("system-")
        }
        revealed = capture(
            metadata,
            [painted(surface) for surface in regions.values()] + list(ink.values()),
        )
        stuck = capture(
            metadata | {"frame_id": 2},
            [painted(regions["system-volume"]), ink["system-volume"]],
        )
        groups_only = {
            name: regions[name]
            for name in ("notifications", "system", "canvas", "apps")
        }
        # This is the original false positive: the large system group changes
        # enough to pass even though its small volume member never disappears.
        self.assertGreater(hidden_regions(revealed, stuck, groups_only)["system"], 0.9)
        with self.assertRaisesRegex(ControlError, "system-volume"):
            hidden_regions(revealed, stuck, regions)
        hidden = capture(metadata | {"frame_id": 3}, [])
        self.assertEqual(set(hidden_regions(revealed, hidden, regions)), set(regions))


class LegacyShellRegionTests(unittest.TestCase):
    def test_physical_control_bounds_use_actual_surface_origin_and_fractional_scale(
        self,
    ):
        status, elements = legacy_scene()
        smoke = smoke_with(status, None)
        with patch.object(
            csd_shell_regions, "a11y", return_value={"elements": elements}
        ):
            regions = csd_shell_regions.surfaces(smoke)
        smoke.control.query.assert_not_called()
        self.assertEqual(
            regions["system-volume"],
            rect(6, 62, 8, 8)
            | {
                "output": "offset",
                "corner": "system",
            },
        )
        self.assertEqual(regions["system-wifi"]["x"], 20)
        self.assertEqual(regions["system-battery"]["x"], 40)
        self.assertEqual(regions["system-control-Chat tray"]["x"], 60)
        self.assertTrue({"notifications", "system", "canvas", "apps"} <= regions.keys())

    def test_legacy_group_only_or_incomplete_bounds_cannot_be_accepted(self):
        for failure in (
            "no frame",
            "group only",
            "no volume",
            "duplicate",
            "missing bounds",
            "nonfinite bounds",
            "duplicate frame",
            "stale frame",
            "outside",
        ):
            with self.subTest(failure=failure):
                status, elements = legacy_scene()
                if failure == "no frame":
                    elements.pop(0)
                elif failure == "group only":
                    elements = elements[:1]
                elif failure == "no volume":
                    elements.pop(1)
                elif failure == "duplicate":
                    elements.append(copy.deepcopy(elements[1]))
                elif failure == "missing bounds":
                    elements[1].pop("bounds")
                elif failure == "nonfinite bounds":
                    elements[1]["bounds"]["width"] = float("inf")
                elif failure == "duplicate frame":
                    elements.append(copy.deepcopy(elements[0]))
                elif failure == "stale frame":
                    elements[0]["bounds"]["width"] += 1
                else:
                    # Its center still fits, but its right edge does not.
                    elements[1]["bounds"] = rect(126, 3, 12, 12)
                with (
                    patch.object(
                        csd_shell_regions, "a11y", return_value={"elements": elements}
                    ),
                    self.assertRaises(ControlError),
                ):
                    csd_shell_regions.surfaces(smoke_with(status, None))

    def test_legacy_bounds_require_stable_live_geometry(self):
        status, elements = legacy_scene()
        changed = copy.deepcopy(status)
        changed["shell_corners"][1]["x"] += 1
        smoke = smoke_with(status, None)
        smoke.control.status.side_effect = [status, changed]
        with (
            patch.object(
                csd_shell_regions, "a11y", return_value={"elements": elements}
            ),
            self.assertRaisesRegex(ControlError, "geometry changed"),
        ):
            csd_shell_regions.surfaces(smoke)

    def test_legacy_invalid_scale_cannot_produce_system_detail(self):
        for scale in (0, -1, float("nan"), float("inf")):
            with self.subTest(scale=scale):
                status, _ = legacy_scene()
                status["outputs"][0]["scale"] = scale
                with self.assertRaisesRegex(ControlError, "usable output scale"):
                    csd_shell_regions.surfaces(smoke_with(status, None))


if __name__ == "__main__":
    unittest.main()
