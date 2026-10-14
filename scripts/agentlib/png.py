"""Read compositor RGB/RGBA PNGs for native visual assertions, without Pillow."""

import struct
import zlib

from .control import PNG_SIGNATURE, ControlError


def pixels(png):
    if not png.startswith(PNG_SIGNATURE):
        raise ControlError("not a PNG")
    offset = 8
    compressed = bytearray()
    width = height = channels = None
    while offset < len(png):
        length, kind = struct.unpack_from(">I4s", png, offset)
        data = png[offset + 8 : offset + 8 + length]
        if len(data) != length:
            raise ControlError("truncated PNG chunk")
        if kind == b"IHDR":
            width, height, depth, color, compression, filtering, interlace = (
                struct.unpack(">IIBBBBB", data)
            )
            if (
                depth != 8
                or color not in (2, 6)
                or compression
                or filtering
                or interlace
            ):
                raise ControlError(
                    "visual assertions require non-interlaced RGB8 or RGBA8 PNG"
                )
            channels = 3 if color == 2 else 4
        elif kind == b"IDAT":
            compressed.extend(data)
        elif kind == b"IEND":
            break
        offset += length + 12
    if width is None or width * height > 64 * 1024 * 1024:
        raise ControlError("missing or oversized PNG dimensions")
    stride = width * channels
    expected = height * (stride + 1)
    decoder = zlib.decompressobj()
    raw = decoder.decompress(compressed, expected + 1)
    if len(raw) != expected or not decoder.eof:
        raise ControlError("unexpected decompressed PNG length")
    rows = []
    previous = bytearray(stride)
    for y in range(height):
        start = y * (stride + 1)
        method = raw[start]
        row = bytearray(raw[start + 1 : start + stride + 1])
        for x in range(stride):
            left = row[x - channels] if x >= channels else 0
            up = previous[x]
            upper_left = previous[x - channels] if x >= channels else 0
            if method == 1:
                predictor = left
            elif method == 2:
                predictor = up
            elif method == 3:
                predictor = (left + up) // 2
            elif method == 4:
                p = left + up - upper_left
                distances = [abs(p - value) for value in (left, up, upper_left)]
                predictor = (left, up, upper_left)[distances.index(min(distances))]
            elif method == 0:
                predictor = 0
            else:
                raise ControlError(f"invalid PNG filter {method}")
            row[x] = (row[x] + predictor) & 255
        rows.append(row)
        previous = row
    return width, height, channels, rows


def green_pixels(png, rect=None):
    width, height, channels, rows = pixels(png)
    x1, y1, x2, y2 = rect or (0, 0, width, height)
    total = 0
    for row in rows[max(0, int(y1)) : min(height, int(y2))]:
        for x in range(max(0, int(x1)), min(width, int(x2))):
            r, g, b = row[x * channels : x * channels + 3]
            total += r < 40 and g > 220 and b < 40
    return total


def crop(png, rect):
    width, height, channels, rows = pixels(png)
    x1, y1, x2, y2 = rect
    return b"".join(
        row[max(0, int(x1)) * channels : min(width, int(x2)) * channels]
        for row in rows[max(0, int(y1)) : min(height, int(y2))]
    )
