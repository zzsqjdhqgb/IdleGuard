#!/usr/bin/env python3
"""Generate the application's multi-resolution Windows icon.

The generator has no third-party dependencies. It rasterises a dark rounded
square with a cyan countdown/power ring, then writes a PNG preview and a
BMP-based multi-size .ico file suitable for Windows resources and the tray.
"""

from __future__ import annotations

import math
import struct
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parent
SS = 512  # supersampled canvas
SIZES = [16, 24, 32, 48, 64, 128, 256]


def blend(dst: list[int], src: tuple[float, float, float, float]) -> None:
    sr, sg, sb, sa = src
    da = dst[3] / 255.0
    out_a = sa + da * (1.0 - sa)
    if out_a <= 0:
        return
    for i, sc in enumerate((sr, sg, sb)):
        dc = dst[i] / 255.0
        dst[i] = round(255 * (sc * sa + dc * da * (1.0 - sa)) / out_a)
    dst[3] = round(255 * out_a)


def rounded_rect_alpha(x: float, y: float, left: float, top: float,
                       right: float, bottom: float, radius: float) -> float:
    cx = min(max(x, left + radius), right - radius)
    cy = min(max(y, top + radius), bottom - radius)
    dx, dy = x - cx, y - cy
    return 1.0 if dx * dx + dy * dy <= radius * radius else 0.0


def render_supersampled() -> list[list[int]]:
    n = SS
    pixels = [[0, 0, 0, 0] for _ in range(n * n)]
    cx = cy = n / 2.0
    outer = n * 0.39
    ring = n * 0.055
    corner = n * 0.19

    for py in range(n):
        for px in range(n):
            x = px + 0.5
            y = py + 0.5
            dst = pixels[py * n + px]

            # Rounded dark panel with a subtle diagonal gradient.
            panel = rounded_rect_alpha(
                x, y, n * 0.055, n * 0.055, n * 0.945, n * 0.945, corner
            )
            if panel:
                t = (x / n) * 0.55 + (y / n) * 0.45
                r = 0.075 * (1 - t) + 0.035 * t
                g = 0.135 * (1 - t) + 0.075 * t
                b = 0.180 * (1 - t) + 0.105 * t
                blend(dst, (r, g, b, 0.98))

            # A soft cyan rim light on the panel.
            edge = rounded_rect_alpha(
                x, y, n * 0.055 + 2, n * 0.055 + 2,
                n * 0.945 - 2, n * 0.945 - 2, corner - 1.5
            )
            if panel and not edge:
                blend(dst, (0.22, 0.74, 0.97, 0.35))

            # Countdown ring with a gap at the top, like a power symbol.
            dx, dy = x - cx, y - cy
            dist = math.hypot(dx, dy)
            if abs(dist - outer) <= ring / 2:
                angle = math.degrees(math.atan2(dy, dx))  # -180..180, 0=right
                # Gap is 44 degrees wide, centred on straight up (-90 deg).
                gap = 42.0
                delta = (angle + 90.0 + 180.0) % 360.0 - 180.0
                if abs(delta) > gap / 2:
                    t = (angle + 180.0) / 360.0
                    r = 0.20 + 0.04 * math.sin(t * math.tau)
                    g = 0.76 + 0.12 * t
                    b = 0.99
                    blend(dst, (r, g, b, 0.98))

            # Rounded power stem through the gap.
            stem_half = n * 0.033
            stem_top = cy - outer - n * 0.055
            stem_bottom = cy - n * 0.085
            if abs(dx) <= stem_half and stem_top <= y <= stem_bottom:
                # Capsule ends.
                cap_ok = True
                if y < stem_top + stem_half:
                    cap_ok = math.hypot(dx, y - (stem_top + stem_half)) <= stem_half
                elif y > stem_bottom - stem_half:
                    cap_ok = math.hypot(dx, y - (stem_bottom - stem_half)) <= stem_half
                if cap_ok:
                    blend(dst, (0.42, 0.91, 1.0, 0.99))

            # Clock hand pointing up-right, inside the ring.
            hand_len = n * 0.155
            hx = cx + math.cos(math.radians(-38)) * hand_len
            hy = cy + math.sin(math.radians(-38)) * hand_len
            # Distance to the line segment from centre to (hx, hy).
            vx, vy = hx - cx, hy - cy
            wx, wy = x - cx, y - cy
            t = max(0.0, min(1.0, (wx * vx + wy * vy) / (vx * vx + vy * vy)))
            lx, ly = cx + t * vx, cy + t * vy
            if math.hypot(x - lx, y - ly) <= n * 0.021:
                blend(dst, (0.86, 0.97, 1.0, 0.95))

            # Centre dot.
            if math.hypot(dx, dy) <= n * 0.037:
                blend(dst, (0.92, 0.99, 1.0, 1.0))
    return pixels


def downsample(src: list[list[int]], src_size: int, dst_size: int) -> list[list[int]]:
    factor = src_size // dst_size
    out = []
    for y in range(dst_size):
        for x in range(dst_size):
            rs = gs = bs = as_ = 0
            for sy in range(factor):
                row = (y * factor + sy) * src_size
                for sx in range(factor):
                    px = src[row + x * factor + sx]
                    a = px[3]
                    rs += px[0] * a
                    gs += px[1] * a
                    bs += px[2] * a
                    as_ += a
            count = factor * factor
            if as_ == 0:
                out.append([0, 0, 0, 0])
            else:
                out.append([
                    round(rs / as_),
                    round(gs / as_),
                    round(bs / as_),
                    round(as_ / count),
                ])
    return out


def png_bytes(pixels: list[list[int]], width: int, height: int) -> bytes:
    raw = bytearray()
    for y in range(height):
        raw.append(0)
        for x in range(width):
            raw.extend(pixels[y * width + x])
    def chunk(kind: bytes, data: bytes) -> bytes:
        return (struct.pack(">I", len(data)) + kind + data
                + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF))
    header = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header)
            + chunk(b"IDAT", zlib.compress(bytes(raw), 9)) + chunk(b"IEND", b""))


def bmp_icon_image(pixels: list[list[int]], size: int) -> bytes:
    # BITMAPINFOHEADER + bottom-up BGRA XOR bitmap + 1bpp AND mask.
    header = struct.pack("<IiiHHIIiiII", 40, size, size * 2, 1, 32, 0,
                         size * size * 4, 0, 0, 0, 0)
    xor = bytearray()
    for y in range(size - 1, -1, -1):
        for x in range(size):
            r, g, b, a = pixels[y * size + x]
            xor.extend((b, g, r, a))
    mask_row_bytes = ((size + 31) // 32) * 4
    mask = bytearray()
    for y in range(size - 1, -1, -1):
        row = bytearray(mask_row_bytes)
        for x in range(size):
            if pixels[y * size + x][3] < 128:
                row[x // 8] |= 0x80 >> (x % 8)
        mask.extend(row)
    return header + bytes(xor) + bytes(mask)


def main() -> None:
    print("rasterising...")
    src = render_supersampled()
    images: list[tuple[int, bytes]] = []
    for size in SIZES:
        pixels = downsample(src, SS, size)
        images.append((size, bmp_icon_image(pixels, size)))
        if size == 256:
            (ROOT / "app.png").write_bytes(png_bytes(pixels, size, size))

    # ICONDIR + entries + image payloads.
    offset = 6 + 16 * len(images)
    entries = bytearray()
    payload = bytearray()
    for size, data in images:
        w = 0 if size == 256 else size
        h = 0 if size == 256 else size
        entries.extend(struct.pack("<BBBBHHII", w, h, 0, 0, 1, 32, len(data), offset))
        payload.extend(data)
        offset += len(data)
    ico = struct.pack("<HHH", 0, 1, len(images)) + entries + payload
    (ROOT / "app.ico").write_bytes(ico)
    print(f"wrote app.ico ({len(ico)} bytes) and app.png")


if __name__ == "__main__":
    main()
