#!/usr/bin/env python3
"""Generate Bolum icon set (app icon + tray state icons) with the stdlib only.

Run from the repository root: python3 scripts/make_icons.py
"""

import struct
import zlib
from pathlib import Path

OUT = Path(__file__).resolve().parent.parent / "src-tauri" / "icons"


def png(width: int, height: int, pixels: bytes) -> bytes:
    def chunk(tag: bytes, data: bytes) -> bytes:
        return (
            struct.pack(">I", len(data))
            + tag
            + data
            + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
        )

    raw = b"".join(
        b"\x00" + pixels[y * width * 4 : (y + 1) * width * 4] for y in range(height)
    )
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(raw, 9))
        + chunk(b"IEND", b"")
    )


def blend(dst, i, rgb, alpha):
    if i < 0 or i + 3 >= len(dst):
        return
    a = alpha / 255.0
    for c in range(3):
        dst[i + c] = int(dst[i + c] * (1 - a) + rgb[c] * a + 0.5)
    dst[i + 3] = min(255, dst[i + 3] + alpha)


def draw(size: int, bg, fg, with_bg: bool, pad_ratio=0.16) -> bytes:
    px = bytearray(size * size * 4)
    cx = size / 2
    s = size
    # rounded-rect background
    if with_bg:
        r = 0.24 * s
        for y in range(s):
            for x in range(s):
                dx = max(abs(x + 0.5 - cx) - (s / 2 - r), 0)
                dy = max(abs(y + 0.5 - cx) - (s / 2 - r), 0)
                if dx * dx + dy * dy <= r * r:
                    blend(px, (y * s + x) * 4, bg, 255)

    # microphone: capsule, arc holder, stem, base
    cw = 0.135 * s                        # capsule half-width
    cap_top = 0.18 * s
    cap_bot = 0.54 * s
    for y in range(int(cap_top) - 1, int(cap_bot) + 2):
        for x in range(s):
            dx = x + 0.5 - cx
            dy = 0.0
            if y + 0.5 < cap_top + cw:
                dy = y + 0.5 - (cap_top + cw)
            elif y + 0.5 > cap_bot - cw:
                dy = y + 0.5 - (cap_bot - cw)
            if dx * dx + dy * dy <= cw * cw:
                blend(px, (y * s + x) * 4, fg, 255)

    # holder: U-shaped cradle under the capsule
    arc_cx, arc_cy, arc_r = cx, 0.36 * s, 0.17 * s
    t = 0.05 * s
    for y in range(int(arc_cy) - 1, int(arc_cy + arc_r + t) + 2):
        for x in range(s):
            dx, dy = x + 0.5 - arc_cx, y + 0.5 - arc_cy
            d = (dx * dx + dy * dy) ** 0.5
            if dy >= -t and abs(d - arc_r) <= t / 2:
                blend(px, (y * s + x) * 4, fg, 255)

    # stem + base
    stem_top, stem_bot = arc_cy + arc_r, 0.63 * s
    half = 0.021 * s
    for y in range(int(stem_top) - 1, int(stem_bot)):
        for x in range(int(cx - half) - 1, int(cx + half) + 1):
            blend(px, (y * s + x) * 4, fg, 255)
    for y in range(int(stem_bot), int(0.665 * s)):
        for x in range(int(cx - 0.11 * s), int(cx + 0.11 * s)):
            blend(px, (y * s + x) * 4, fg, 255)
    return bytes(px)


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    bg, fg = (24, 26, 32), (240, 243, 248)
    (OUT / "icon.png").write_bytes(png(1024, 1024, draw(1024, bg, fg, True)))
    for state, color in (
        ("idle", (152, 161, 173)),
        ("recording", (255, 107, 107)),
        ("transcribing", (110, 168, 254)),
    ):
        art = draw(64, bg, color, False)
        (OUT / f"tray-{state}.png").write_bytes(png(64, 64, art))
    print(f"wrote icons to {OUT}")


if __name__ == "__main__":
    main()
