"""Generates assets/castorterm.ico: pixel-art neon prompt `>_` in the
castorterm palette, as PNG images inside an ICO (16 to 256 px).

Pure Python (zlib + struct), no dependencies:  python assets/icon/make_icon.py
"""

import struct
import zlib
from pathlib import Path

# Theme colours (src/theme.rs).
BG = (0x16, 0x12, 0x33, 255)
BG_EDGE = (0x0B, 0x0A, 0x1A, 255)
PINK = (0xFF, 0x4F, 0xB0, 255)
CYAN = (0x2D, 0xE2, 0xE6, 255)
GLOW_PINK = (0xFF, 0x4F, 0xB0, 90)
GLOW_CYAN = (0x2D, 0xE2, 0xE6, 90)
CLEAR = (0, 0, 0, 0)

# 16x16 design: '.' clear, 'b' background, 'e' edge, 'p' pink, 'c' cyan.
GRID = [
    "..eeeeeeeeeeee..",
    ".eppppppppppppe.",
    "epbbbbbbbbbbbbpe",
    "epbbbbbbbbbbbbpe",
    "epbccbbbbbbbbbpe",
    "epbbccbbbbbbbbpe",
    "epbbbccbbbbbbbpe",
    "epbbbbccbbbbbbpe",
    "epbbbccbbbbbbbpe",
    "epbbccbbbbbbbbpe",
    "epbccbbbpppppbpe",
    "epbbbbbbpppppbpe",
    "epbbbbbbbbbbbbpe",
    "epbbbbbbbbbbbbpe",
    ".eppppppppppppe.",
    "..eeeeeeeeeeee..",
]
COLOURS = {".": CLEAR, "b": BG, "e": BG_EDGE, "p": PINK, "c": CYAN}


def pixels(size: int) -> list[list[tuple[int, int, int, int]]]:
    """Nearest-neighbour scale of the grid, with a soft glow next to the
    neon strokes on sizes of 48 px and up."""
    scale = size / 16
    out = []
    for y in range(size):
        row = []
        for x in range(size):
            gx, gy = int(x / scale), int(y / scale)
            row.append(COLOURS[GRID[gy][gx]])
        out.append(row)
    if size >= 48:
        glow = [r[:] for r in out]
        for y in range(size):
            for x in range(size):
                if out[y][x] != BG:
                    continue
                near = {out[ny][nx] for ny in range(max(0, y - 2), min(size, y + 3))
                        for nx in range(max(0, x - 2), min(size, x + 3))}
                if CYAN in near:
                    glow[y][x] = blend(BG, GLOW_CYAN)
                elif PINK in near:
                    glow[y][x] = blend(BG, GLOW_PINK)
        out = glow
    return out


def blend(base, top):
    a = top[3] / 255
    return tuple(round(base[i] * (1 - a) + top[i] * a) for i in range(3)) + (255,)


def png(size: int) -> bytes:
    raw = b"".join(
        b"\x00" + b"".join(bytes(p) for p in row) for row in pixels(size)
    )

    def chunk(kind: bytes, data: bytes) -> bytes:
        body = kind + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body))

    header = struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0)  # RGBA, 8 bit
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header)
            + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b""))


def ico(sizes: list[int]) -> bytes:
    images = [png(s) for s in sizes]
    out = struct.pack("<HHH", 0, 1, len(images))
    offset = 6 + 16 * len(images)
    for size, data in zip(sizes, images):
        dim = 0 if size >= 256 else size
        out += struct.pack("<BBBBHHII", dim, dim, 0, 0, 1, 32, len(data), offset)
        offset += len(data)
    return out + b"".join(images)


if __name__ == "__main__":
    root = Path(__file__).resolve().parents[1]
    (root / "castorterm.ico").write_bytes(ico([16, 24, 32, 48, 64, 128, 256]))
    (root / "icon" / "castorterm-256.png").write_bytes(png(256))
    print("wrote", root / "castorterm.ico")
