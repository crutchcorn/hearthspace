"""Import static named cursors from an unpacked Adwaita 50.0 source release."""

import argparse
import struct
from pathlib import Path

SHAPES = [
    "context-menu",
    "help",
    "pointer",
    "progress",
    "wait",
    "cell",
    "crosshair",
    "text",
    "vertical-text",
    "alias",
    "copy",
    "move",
    "no-drop",
    "not-allowed",
    "e-resize",
    "n-resize",
    "ne-resize",
    "nw-resize",
    "s-resize",
    "se-resize",
    "sw-resize",
    "w-resize",
    "col-resize",
    "row-resize",
    "all-scroll",
    "zoom-in",
    "zoom-out",
    "dnd-ask",
    "all-resize",
]
ROOT = Path(__file__).resolve().parent.parent


def first_frames(contents):
    """Keep the first image per nominal size, preserving pixels and hotspots."""
    magic, header, version, count = struct.unpack_from("<4I", contents)
    if magic != 0x72756358:
        raise ValueError("not an Xcursor file")
    images = {}
    for index in range(count):
        kind, size, offset = struct.unpack_from("<3I", contents, header + index * 12)
        if kind != 0xFFFD0002 or size in images:
            continue
        width, height = struct.unpack_from("<2I", contents, offset + 16)
        length = struct.unpack_from("<I", contents, offset)[0] + width * height * 4
        images[size] = contents[offset : offset + length]
    if not images:
        raise ValueError("cursor has no images")
    result = bytearray(struct.pack("<4I", magic, 16, version, len(images)))
    offset = 16 + len(images) * 12
    for size, image in images.items():
        result.extend(struct.pack("<3I", 0xFFFD0002, size, offset))
        offset += len(image)
    return bytes(result) + b"".join(images.values())


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path, help="unpacked adwaita-icon-theme-50.0")
    args = parser.parse_args()
    destination = ROOT / "assets/cursors/adwaita"
    destination.mkdir(exist_ok=True)
    for name in SHAPES:
        # Resolve semantic aliases without substituting the default arrow.
        source_name = {"move": "all-resize", "dnd-ask": "context-menu"}.get(name, name)
        source = args.source / "Adwaita/cursors" / source_name
        (destination / name).write_bytes(first_frames(source.read_bytes()))
    notice = (args.source / "COPYING").read_bytes().rstrip(b"\n") + b"\n"
    (destination / "COPYING").write_bytes(notice)
