#!/usr/bin/env python3
"""Burntime's Workbench icons, drawn here: no game art in them.

AROS reads a PNG icon with an `icOn` chunk (workbench/libs/icon/
diskobjPNGio.c): two PNGs back to back, normal and selected, the chunk's
tags big-endian. Kind 3 is a tool (the program), 2 a drawer.

    tools/make_icons.py DRAWER   writes DRAWER/Burntime.info and DRAWER.info
"""
import struct
import sys
import zlib
from pathlib import Path

SIZE = 48


def chunk(name, data):
    return struct.pack(">I", len(data)) + name + data + struct.pack(">I", zlib.crc32(name + data) & 0xFFFFFFFF)


def picture(selected):
    """A sun going down behind ruins, in burnt colours; selected is a
    shade lighter."""
    lift = 40 if selected else 0
    rows = []
    for y in range(SIZE):
        row = []
        for x in range(SIZE):
            dx, dy = x - SIZE / 2, y - 26
            dune = 30 + 4 * ((x * 7 // SIZE) % 2) + (2 if x % 12 < 6 else 0)
            if y > dune:
                shade = 140 - (y - dune) * 3
                pixel = (min(255, 180 + lift), min(255, shade // 2 + 40 + lift), 24, 255)
            elif dx * dx + dy * dy < 120:
                pixel = (255, min(255, 150 + lift), 40, 255)
            else:
                pixel = (min(255, 120 + y * 2 + lift), min(255, 40 + y + lift // 2), 20, 255)
            row.append(pixel)
        rows.append(row)
    return rows


def icon_png(selected, kind):
    raw = bytearray()
    for row in picture(selected):
        raw.append(0)
        for r, g, b, a in row:
            raw.extend((r, g, b, a))
    tags = [(0x8000100F, kind), (0x80001009, 1048576)]
    if kind == 2:
        tags.extend([(0x80001005, 540), (0x80001006, 300)])
    meta = b"".join(struct.pack(">II", *t) for t in tags)
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", SIZE, SIZE, 8, 6, 0, 0, 0))
            + chunk(b"icOn", meta) + chunk(b"IDAT", zlib.compress(bytes(raw), 9)) + chunk(b"IEND", b""))


def main():
    dest = Path(sys.argv[1])
    dest.mkdir(parents=True, exist_ok=True)
    (dest / "Burntime.info").write_bytes(icon_png(False, 3) + icon_png(True, 3))
    dest.with_suffix(".info").write_bytes(icon_png(False, 2) + icon_png(True, 2))


if __name__ == "__main__":
    main()
