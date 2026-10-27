#!/usr/bin/env python3
"""Generates PushTalks's tray and app icons (PNG, ICNS, ICO) with no third-party dependencies.

Run from the repo root: python3 scripts/make-icons.py
"""
import math
import os
import struct
import subprocess
import tempfile
import zlib

OUT = os.path.join(os.path.dirname(__file__), '..', 'src-tauri', 'icons')


def png(width, height, rgba_rows):
    raw = b''.join(b'\x00' + bytes(row) for row in rgba_rows)
    def chunk(tag, data):
        return struct.pack('>I', len(data)) + tag + data + struct.pack('>I', zlib.crc32(tag + data) & 0xffffffff)
    return (b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', width, height, 8, 6, 0, 0, 0))
            + chunk(b'IDAT', zlib.compress(raw, 9)) + chunk(b'IEND', b''))


def sd_round_rect(px, py, x0, y0, x1, y1, r):
    cx, cy = (x0 + x1) / 2, (y0 + y1) / 2
    hx, hy = (x1 - x0) / 2 - r, (y1 - y0) / 2 - r
    dx, dy = abs(px - cx) - hx, abs(py - cy) - hy
    return math.hypot(max(dx, 0), max(dy, 0)) + min(max(dx, dy), 0) - r


def mic(px, py):
    """True if the unit-square point is inside the microphone glyph."""
    if sd_round_rect(px, py, 0.36, 0.10, 0.64, 0.60, 0.14) <= 0:
        return True
    # U-shaped holder: lower half of a ring around the capsule.
    d = math.hypot(px - 0.5, py - 0.46)
    if py >= 0.46 and abs(d - 0.25) <= 0.045:
        return True
    if abs(px - 0.5) <= 0.04 and 0.70 <= py <= 0.88:
        return True
    return sd_round_rect(px, py, 0.30, 0.84, 0.70, 0.91, 0.035) <= 0


def render(size, fg, bg=None, glyph_scale=1.0, ss=4):
    rows = []
    for y in range(size):
        row = []
        for x in range(size):
            hits = 0
            bg_hits = 0
            for sy in range(ss):
                for sx in range(ss):
                    u = (x + (sx + 0.5) / ss) / size
                    v = (y + (sy + 0.5) / ss) / size
                    if bg is not None and sd_round_rect(u, v, 0.0, 0.0, 1.0, 1.0, 0.225) <= 0:
                        bg_hits += 1
                    gu = (u - 0.5) / glyph_scale + 0.5
                    gv = (v - 0.5) / glyph_scale + 0.5
                    if mic(gu, gv):
                        hits += 1
            a_fg = hits / (ss * ss)
            if bg is None:
                row += [*fg, round(255 * a_fg)]
            else:
                a_bg = bg_hits / (ss * ss)
                t = v_grad = y / max(1, size - 1)
                base = [round(bg[0][i] * (1 - t) + bg[1][i] * t) for i in range(3)]
                col = [round(base[i] * (1 - a_fg) + fg[i] * a_fg) for i in range(3)]
                row += [*col, round(255 * max(a_bg, 0))]
        rows.append(row)
    return png(size, size, rows)


def write(name, data):
    with open(os.path.join(OUT, name), 'wb') as f:
        f.write(data)


def main():
    os.makedirs(OUT, exist_ok=True)
    # macOS tray: black template images (macOS tints them for light/dark menu bars).
    write('trayTemplate.png', render(16, (0, 0, 0), glyph_scale=0.95))
    write('trayTemplate@2x.png', render(32, (0, 0, 0), glyph_scale=0.95))
    # Windows tray: white glyph on the brand tile so it reads on light and dark taskbars.
    brand = ((17, 17, 17), (17, 17, 17))
    write('tray.png', render(32, (138, 102, 255), bg=brand, glyph_scale=0.72))
    # App icon.
    sizes = [16, 32, 64, 128, 256, 512, 1024]
    images = {s: render(s, (138, 102, 255), bg=brand, glyph_scale=0.62, ss=2 if s > 256 else 4) for s in sizes}
    write('icon.png', images[1024])



if __name__ == '__main__':
    main()
