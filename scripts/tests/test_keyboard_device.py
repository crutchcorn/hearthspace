"""Keyboard unplug evidence must retain the held chord and backend event order."""

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib.control import ControlError
from agentlib.keyboard_device import removal_proof


class KeyboardRemovalEvidenceTests(unittest.TestCase):
    ALT = " event15 KEYBOARD_KEY +1.00s KEY_LEFTALT (56) pressed\n"
    TAB = " event15 KEYBOARD_KEY +1.01s KEY_TAB (15) pressed\n"
    RELEASE = " event15 KEYBOARD_KEY +1.02s KEY_LEFTALT (56) released\n"
    REMOVED = " event15 DEVICE_REMOVED Hearthspace keyboard\n"

    def test_trace_proves_held_chord_before_keyboard_removal(self):
        proof = removal_proof(self.ALT + self.TAB + self.REMOVED)
        self.assertTrue(proof["alt_tab_before_removal"])
        self.assertFalse(proof["libinput_released_alt"])

    def test_backend_synthesized_alt_release_is_retained_as_evidence(self):
        proof = removal_proof(self.ALT + self.TAB + self.RELEASE + self.REMOVED)
        self.assertTrue(proof["libinput_released_alt"])

    def test_missing_or_out_of_order_device_events_cannot_prove_removal(self):
        for trace in (
            "",
            self.ALT + self.TAB,
            self.TAB + self.REMOVED,
            self.ALT + self.REMOVED,
            self.ALT + self.REMOVED + self.TAB,
        ):
            with (
                self.subTest(trace=trace),
                self.assertRaisesRegex(ControlError, "before device removal"),
            ):
                removal_proof(trace)


if __name__ == "__main__":
    unittest.main()
