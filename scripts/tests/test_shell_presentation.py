"""Reject misleading screenshot evidence and wrong popup dimensions."""

import sys
import unittest
from pathlib import Path
from unittest.mock import Mock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib import shell_presentation as presentation
from agentlib import shell_search
from agentlib.control import ControlError
from test_agent_visual import capture


class PopupPresentationTests(unittest.TestCase):
    def setUp(self):
        self.metadata = {
            "width": 320,
            "height": 240,
            "scale": 2,
            "logical_x": -20,
            "logical_y": 10,
        }
        self.surface = {"kind": "search", "x": 0, "y": 20, "width": 100, "height": 80}
        self.background = presentation.Frame(capture(self.metadata, []))
        # Two intersecting strips leave all four outside corner patches clear.
        self.rounded = [
            ((3, 20, 97, 100), (20, 20, 20)),
            ((0, 23, 100, 97), (20, 20, 20)),
        ]

    def frame(self, rectangles):
        return presentation.Frame(capture(self.metadata, rectangles))

    def test_corners_match_actual_background_on_scaled_offset_output(self):
        corners = presentation.rounded(
            self.background, self.frame(self.rounded), self.surface
        )
        self.assertEqual(corners, [[40, 20], [40, 179], [239, 20], [239, 179]])

    def test_square_corner_in_any_of_four_positions_fails(self):
        for x, y in ((0, 20), (0, 99), (99, 20), (99, 99)):
            with (
                self.subTest(x=x, y=y),
                self.assertRaisesRegex(ControlError, "corner is opaque"),
            ):
                visible = self.frame(
                    self.rounded + [((x, y, x + 1, y + 1), (20, 20, 20))]
                )
                presentation.rounded(self.background, visible, self.surface)

    def test_absent_panel_cannot_pass_transparency_assertion(self):
        with self.assertRaisesRegex(ControlError, "no visible interior"):
            presentation.rounded(self.background, self.background, self.surface)

    def test_changed_output_geometry_is_not_compared(self):
        visible = presentation.Frame(
            capture(self.metadata | {"scale": 1}, self.rounded)
        )
        with self.assertRaisesRegex(ControlError, "output geometry changed"):
            presentation.rounded(self.background, visible, self.surface)

    def test_offscreen_geometry_cannot_be_silently_clipped(self):
        with self.assertRaisesRegex(ControlError, "outside captured output"):
            self.background.rect(self.surface | {"x": -40})

    def test_selected_highlight_must_change_both_result_rows(self):
        rows = [
            {"name": name, "bounds": {"x": 10, "y": y, "width": 180, "height": 40}}
            for name, y in (("first", 20), ("second", 70))
        ]
        first = self.frame([((5, 30, 95, 50), (45, 45, 45))])
        second = self.frame([((5, 55, 95, 75), (45, 45, 45))])
        fractions = presentation.highlight_change(first, second, self.surface, rows)
        self.assertEqual(fractions, [1, 1])
        for missing in (first, self.background):
            with (
                self.subTest(missing=missing),
                self.assertRaisesRegex(ControlError, "visibly change row"),
            ):
                presentation.highlight_change(first, missing, self.surface, rows)

    def test_caret_or_tiny_text_change_cannot_pass_selection_highlight(self):
        row = {
            "name": "result",
            "bounds": {"x": 10, "y": 20, "width": 180, "height": 40},
        }
        caret = self.frame([((5, 30, 6, 40), (255, 255, 255))])
        with self.assertRaisesRegex(ControlError, "visibly change row"):
            presentation.highlight_change(self.background, caret, self.surface, [row])

    def test_content_gap_uses_surface_local_physical_bounds_and_output_scale(self):
        node = {"bounds": {"x": 20, "y": 70, "width": 160, "height": 64}}
        self.assertEqual(presentation.bottom_gap(self.surface, node, 2), 13)
        self.assertEqual(
            presentation.bottom_gap(self.surface | {"height": 400}, node, 2), 333
        )
        with self.assertRaises(ControlError):
            presentation.bottom_gap(
                self.surface, {"bounds": node["bounds"] | {"height": 0}}, 2
            )

    def test_content_fit_waits_for_measured_resize_and_stable_geometry(self):
        smoke = Mock()
        node = {"bounds": {"x": 20, "y": 70, "width": 160, "height": 64}}
        calls = []

        def until(_name, condition):
            for _ in range(6):
                result = condition()
                calls.append(result)
                if result:
                    return result
            self.fail("expected measured content fit")

        smoke.until.side_effect = until
        with (
            patch.object(
                presentation.checks,
                "panel",
                side_effect=[
                    self.surface | {"height": 400},
                    self.surface | {"height": 400},
                    self.surface,
                    self.surface,
                ],
            ),
            patch.object(presentation.checks, "elements", return_value=[node]),
        ):
            self.assertEqual(
                presentation.fitted(smoke, "search", "hint", {"scale": 2}),
                (self.surface, 13),
            )
        self.assertEqual(calls[:3], [None, None, None])

    def test_search_width_tracks_fraction_and_small_output_edges(self):
        for width, expected in (
            (1280, 768),
            (800, 480),
            (600, 360),
            (300, 276),
            (20, 1),
        ):
            with self.subTest(width=width):
                self.assertEqual(
                    presentation.search_width(
                        {"width": expected}, {"logical_width": width}
                    ),
                    expected,
                )
                with self.assertRaisesRegex(ControlError, "responsive width"):
                    presentation.search_width(
                        {"width": expected + 20}, {"logical_width": width}
                    )


class SearchSelectionTests(unittest.TestCase):
    def test_selection_requires_exactly_one_semantically_selected_result(self):
        smoke = Mock()
        results = [None, None, None, {"name": "Window: target"}]

        def until(_name, condition):
            for expected in results:
                self.assertEqual(condition(), expected)
            return results[-1]

        smoke.until.side_effect = until
        with patch.object(
            shell_search,
            "a11y",
            side_effect=[
                {"elements": []},
                {"elements": [{"name": "Open: other"}]},
                {"elements": [{"name": "Window: target"}, {"name": "Open: other"}]},
                {"elements": [{"name": "Window: target"}]},
            ],
        ) as find:
            self.assertEqual(
                shell_search.selected(smoke, "Window: target"), results[-1]
            )
        self.assertIn("@selected='true'", find.call_args.args[2])


if __name__ == "__main__":
    unittest.main()
