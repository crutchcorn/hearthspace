"""A missing group backdrop cannot stand in for disappearing widget ink."""

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib.control import ControlError
from agentlib.csd_shell import hidden_regions
from test_agent_visual import capture


class ForegroundDisappearanceTests(unittest.TestCase):
    def header_scene(self, *, scale=1):
        # Reproduce the observed 70x44 clock region straddling GTK's y=46 border.
        # These are synthetic rectangles, not captured user images or glyph data.
        metadata = {
            "output": "native",
            "frame_id": 2,
            "width": round(100 * scale),
            "height": round(100 * scale),
            "scale": scale,
            "logical_x": 0,
            "logical_y": 0,
        }
        surface = {"x": 10, "y": 29, "width": 70, "height": 44}
        client = [
            ((0, 0, 100, 46), (220, 216, 212)),
            ((0, 46, 100, 47), (191, 184, 177)),
            ((0, 47, 100, 100), (246, 245, 244)),
        ]
        # Exactly 13 of 60 high-contrast ink pixels lie on the border at scale 1.
        # A whole-widget hidden median therefore reproduces the native 47/60.
        ink = [
            ((25, 46, 38, 47), (40, 45, 50)),
            ((25, 48, 30, 57), (40, 45, 50)),
            ((34, 50, 36, 51), (40, 45, 50)),
        ]
        revealed = capture(
            metadata, client + [((10, 29, 80, 73), (248, 247, 245))] + ink
        )
        return metadata, surface, client, ink, revealed

    def scene(self, *, scale=1, dark=False):
        metadata = {
            "output": "native",
            "frame_id": 1,
            "width": round(120 * scale),
            "height": round(100 * scale),
            "scale": scale,
            "logical_x": -40,
            "logical_y": 10,
        }
        surface = {"x": -20, "y": 30, "width": 44, "height": 44}
        background = (30, 35, 40) if dark else (245, 244, 240)
        foreground = (230, 235, 240) if dark else (40, 45, 50)
        # Deliberately asymmetric synthetic ink, unrelated to any shell glyph.
        shape = [(-6, 42, -3, 59), (-3, 53, 5, 56), (8, 44, 11, 51)]
        ink = [(bounds, foreground) for bounds in shape]
        backing = [((-20, 30, 24, 74), background)]
        revealed = capture(metadata, backing + ink)
        return metadata, surface, backing, ink, revealed

    def test_persistent_ink_fails_even_when_most_widget_pixels_change(self):
        for scale in (1, 1.5, 2):
            with self.subTest(scale=scale):
                metadata, surface, _, ink, revealed = self.scene(scale=scale)
                stuck = capture(metadata | {"frame_id": 2}, ink)
                # The old whole-rectangle criterion accepts the vanished backing.
                old = hidden_regions(revealed, stuck, {"system": surface})
                self.assertGreater(old["system"], 0.9)
                with self.assertRaisesRegex(ControlError, "system-volume.*foreground"):
                    hidden_regions(revealed, stuck, {"system-volume": surface})

    def test_real_ink_disappearance_passes_in_light_and_dark_palettes(self):
        for dark in (False, True):
            with self.subTest(dark=dark):
                metadata, surface, _, _, revealed = self.scene(dark=dark)
                hidden = capture(metadata | {"frame_id": 2}, [])
                self.assertEqual(
                    set(hidden_regions(revealed, hidden, {"system-volume": surface})),
                    {"system-volume"},
                )

    def test_recomposited_translucent_ink_is_still_visible(self):
        metadata, surface, _, ink, revealed = self.scene()
        # Every foreground RGB value changes, but the glyph's contrast remains.
        recomposited = [(bounds, (10, 15, 20)) for bounds, _ in ink]
        stuck = capture(metadata | {"frame_id": 2}, recomposited)
        with self.assertRaisesRegex(ControlError, "foreground"):
            hidden_regions(revealed, stuck, {"system-volume": surface})

    def test_group_backing_without_observable_ink_is_insufficient(self):
        metadata, surface, backing, _, _ = self.scene()
        blank = capture(metadata, backing)
        hidden = capture(metadata | {"frame_id": 2}, [])
        with self.assertRaisesRegex(ControlError, "no identifiable foreground"):
            hidden_regions(blank, hidden, {"system-volume": surface})

    def test_initial_hidden_proof_also_rejects_persistent_ink(self):
        metadata, surface, _, ink, revealed = self.scene()
        initial = capture(metadata, ink)
        revealed = revealed[0] | {"frame_id": 2}, revealed[1]
        with self.assertRaisesRegex(ControlError, "foreground"):
            hidden_regions(revealed, initial, {"system-volume": surface}, initial=True)

    def test_clock_disappears_over_two_tone_header_and_horizontal_border(self):
        for scale in (1, 1.5, 2):
            with self.subTest(scale=scale):
                metadata, surface, client, _, revealed = self.header_scene(scale=scale)
                initial = capture(metadata | {"frame_id": 1}, client)
                hidden = capture(metadata | {"frame_id": 3}, client)
                for image, is_initial in ((initial, True), (hidden, False)):
                    proof = hidden_regions(
                        revealed, image, {"system-clock": surface}, initial=is_initial
                    )
                    self.assertEqual(set(proof), {"system-clock"})

    def test_two_tone_backing_cannot_conceal_retained_or_recomposited_ink(self):
        metadata, surface, client, ink, revealed = self.header_scene()
        for color in ((40, 45, 50), (80, 85, 90)):
            with self.subTest(color=color):
                retained = [(bounds, color) for bounds, _old_color in ink]
                hidden = capture(metadata | {"frame_id": 3}, client + retained)
                with self.assertRaisesRegex(ControlError, "system-clock.*foreground"):
                    hidden_regions(revealed, hidden, {"system-clock": surface})

    def test_ink_row_without_revealed_backing_samples_is_insufficient(self):
        metadata, surface, backing, ink, _ = self.scene()
        # This row differs from backing by more than 12 but less than 32:
        # it supplies neither backing-mask samples nor extra foreground pixels.
        unsupported = [((-20, 50, 24, 51), (230, 229, 225))]
        revealed = capture(metadata, backing + unsupported + ink)
        hidden = capture(metadata | {"frame_id": 2}, [])
        with self.assertRaisesRegex(ControlError, "foreground row has no backing"):
            hidden_regions(revealed, hidden, {"system-volume": surface})


if __name__ == "__main__":
    unittest.main()
