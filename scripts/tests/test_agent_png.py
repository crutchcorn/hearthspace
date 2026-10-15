import struct
import sys
import unittest
import zlib
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib.control import PNG_SIGNATURE, ControlError
from agentlib.png import crop, green_pixels, pixels


def chunk(kind, data):
    return (
        struct.pack(">I", len(data))
        + kind
        + data
        + struct.pack(">I", zlib.crc32(kind + data))
    )


def png(raw, width=2, height=1):
    return (
        PNG_SIGNATURE
        + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(raw))
        + chunk(b"IEND", b"")
    )


class PngTests(unittest.TestCase):
    def test_all_png_scanline_filters(self):
        # Two identical pixels in two identical rows make filters explicit.
        row = bytes([10, 240, 20, 10, 240, 20])
        encoded = {
            0: row,
            1: bytes([10, 240, 20, 0, 0, 0]),
            2: bytes(6),
            3: bytes([5, 120, 10, 0, 0, 0]),
            4: bytes(6),
        }
        for method, second in encoded.items():
            with self.subTest(method=method):
                image = png(b"\0" + row + bytes([method]) + second, height=2)
                self.assertEqual(pixels(image)[3], [row, row])
                self.assertEqual(green_pixels(image), 4)
                self.assertEqual(crop(image, (1, 0, 2, 1)), bytes([10, 240, 20]))

    def test_invalid_filter_and_decompressed_lengths_fail(self):
        for raw in (b"\5" + bytes(6), b"\0" + bytes(5), b"\0" + bytes(7)):
            with self.subTest(raw=raw), self.assertRaises(ControlError):
                pixels(png(raw))


if __name__ == "__main__":
    unittest.main()
