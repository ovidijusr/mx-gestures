#!/usr/bin/env python3
"""Render the app icon (same mouse glyph as the menu bar) to icon_1024.png.
No dependencies — writes the PNG by hand. Used by make-app.sh via iconutil."""
import struct, zlib, math

S = 1024
px = bytearray(S * S * 4)

def sd_round_box(x, y, hw, hh, r):
    qx, qy = abs(x) - (hw - r), abs(y) - (hh - r)
    return math.hypot(max(qx, 0), max(qy, 0)) + min(max(qx, qy), 0) - r

BG = (245, 245, 247)      # light rounded-square canvas
FG = (29, 29, 31)         # near-black glyph, same shape as tray icon

for yy in range(S):
    for xx in range(S):
        x, y = xx + 0.5 - S / 2, yy + 0.5 - S / 2
        i = (yy * S + xx) * 4
        # canvas: rounded square, margin 100, radius 180
        bg_a = min(max(-sd_round_box(x, y, 412, 412, 180), 0.0), 1.0)
        if bg_a <= 0:
            continue
        # mouse body outline (w=460 h=760 r=205, stroke 54) + scroll bar
        sd = sd_round_box(x, y - 10, 230, 380, 205)
        glyph = min(max(27.0 - abs(sd), 0.0), 1.0)
        if abs(x) < 27 and -290 <= y <= -120:
            glyph = 1.0
        r_, g_, b_ = (FG[c] * glyph + BG[c] * (1 - glyph) for c in range(3))
        px[i:i + 4] = bytes((int(r_), int(g_), int(b_), int(bg_a * 255)))

def chunk(tag, data):
    return struct.pack(">I", len(data)) + tag + data + struct.pack(">I", zlib.crc32(tag + data))

raw = b"".join(b"\x00" + bytes(px[y * S * 4:(y + 1) * S * 4]) for y in range(S))
png = (b"\x89PNG\r\n\x1a\n"
       + chunk(b"IHDR", struct.pack(">IIBBBBB", S, S, 8, 6, 0, 0, 0))
       + chunk(b"IDAT", zlib.compress(raw, 9))
       + chunk(b"IEND", b""))
open("icon_1024.png", "wb").write(png)
print("wrote icon_1024.png")
