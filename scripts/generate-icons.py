#!/usr/bin/env python3
"""
Generates multi-resolution icons (.png, .icns, .ico) for Vy.
Uses standard library (math, struct, zlib) and macOS system tools (sips, iconutil).
"""

import math
import os
import struct
import subprocess
import zlib

ASSETS_DIR = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "assets"))
os.makedirs(ASSETS_DIR, exist_ok=True)


def clamp(val, low=0.0, high=1.0):
    return max(low, min(high, val))


def mix(c1, c2, t):
    t = clamp(t)
    return tuple(c1[i] * (1.0 - t) + c2[i] * t for i in range(len(c1)))


def render_vy_icon(size):
    """
    Renders the Vy squircle icon with anti-aliasing into an RGBA byte buffer.
    """
    width = size
    height = size
    scale = size / 512.0

    raw = bytearray()

    # Geometry parameters in 512x512 space
    squircle_r = 104.0
    pad = 32.0
    sq_size = 448.0

    # Colors
    bg_top = (0x1e / 255.0, 0x1e / 255.0, 0x24 / 255.0, 1.0)
    bg_bot = (0x0f / 255.0, 0x0f / 255.0, 0x12 / 255.0, 1.0)

    v_left_top = (0x38 / 255.0, 0xbd / 255.0, 0xf8 / 255.0, 1.0)  # sky-400
    v_left_bot = (0x02 / 255.0, 0x84 / 255.0, 0xc7 / 255.0, 1.0)  # sky-600

    v_right_top = (0x81 / 255.0, 0x8c / 255.0, 0xf8 / 255.0, 1.0)  # indigo-400
    v_right_bot = (0x4f / 255.0, 0x46 / 255.0, 0xe5 / 255.0, 1.0)  # indigo-600

    cursor_color = (0x38 / 255.0, 0xbd / 255.0, 0xf8 / 255.0, 0.95)

    def dist_rounded_rect(px, py, rx, ry, rw, rh, rad):
        # Center coords
        cx = rx + rw / 2.0
        cy = ry + rh / 2.0
        dx = abs(px - cx) - (rw / 2.0 - rad)
        dy = abs(py - cy) - (rh / 2.0 - rad)
        ax = max(0.0, dx)
        ay = max(0.0, dy)
        outside_dist = math.sqrt(ax * ax + ay * ay)
        inside_dist = min(0.0, max(dx, dy))
        return outside_dist + inside_dist - rad

    def point_in_triangle(px, py, p0, p1, p2):
        s = (
            (p0[0] - p2[0]) * (py - p2[1]) - (p0[1] - p2[1]) * (px - p2[0])
        )
        t = (
            (p1[0] - p0[0]) * (py - p0[1]) - (p1[1] - p0[1]) * (px - p0[0])
        )
        if (s < 0.0) != (t < 0.0) and s != 0.0 and t != 0.0:
            return False
        d = (
            (p2[0] - p1[0]) * (py - p1[1]) - (p2[1] - p1[1]) * (px - p1[0])
        )
        return (d == 0.0) or ((d < 0.0) == (s + t <= 0.0))

    def point_in_quad(px, py, p0, p1, p2, p3):
        return point_in_triangle(px, py, p0, p1, p2) or point_in_triangle(
            px, py, p0, p2, p3
        )

    # Coordinates in 512x512 space
    # Left branch of V:
    # 144,140 -> 220,140 -> 256,320 -> 210,320
    q1 = (144.0, 140.0)
    q2 = (220.0, 140.0)
    q3 = (256.0, 320.0)
    q4 = (210.0, 320.0)

    # Right branch of V:
    # 368,140 -> 292,140 -> 236,376 -> 276,376
    r1 = (368.0, 140.0)
    r2 = (292.0, 140.0)
    r3 = (236.0, 376.0)
    r4 = (276.0, 376.0)

    # Cursor box:
    cur_x, cur_y, cur_w, cur_h, cur_rad = 300.0, 324.0, 44.0, 24.0, 6.0

    # Oversample 2x2 for clean anti-aliasing
    subpixels = [(-0.25, -0.25), (0.25, -0.25), (-0.25, 0.25), (0.25, 0.25)]

    for y in range(height):
        raw.append(0)  # filter type
        for x in range(width):
            acc_r, acc_g, acc_b, acc_a = 0.0, 0.0, 0.0, 0.0

            for ox, oy in subpixels:
                px = (x + 0.5 + ox) / scale
                py = (y + 0.5 + oy) / scale

                # Squircle background SDF
                d_sq = dist_rounded_rect(
                    px, py, pad, pad, sq_size, sq_size, squircle_r
                )

                if d_sq > 1.0:
                    continue  # completely transparent

                alpha_sq = clamp(0.5 - d_sq)

                # Gradient background
                t_bg = (py - pad) / sq_size
                bg_col = mix(bg_top, bg_bot, t_bg)

                pixel_col = bg_col

                # Right branch of V
                if point_in_quad(px, py, r1, r2, r3, r4):
                    t_vr = (py - 140.0) / (376.0 - 140.0)
                    pixel_col = mix(v_right_top, v_right_bot, t_vr)

                # Left branch of V (drawn on top)
                if point_in_quad(px, py, q1, q2, q3, q4):
                    t_vl = (py - 140.0) / (320.0 - 140.0)
                    pixel_col = mix(v_left_top, v_left_bot, t_vl)

                # Cursor
                d_cur = dist_rounded_rect(
                    px, py, cur_x, cur_y, cur_w, cur_h, cur_rad
                )
                if d_cur <= 0.0:
                    pixel_col = cursor_color

                acc_r += pixel_col[0] * alpha_sq
                acc_g += pixel_col[1] * alpha_sq
                acc_b += pixel_col[2] * alpha_sq
                acc_a += pixel_col[3] * alpha_sq

            r = int(clamp(acc_r / 4.0) * 255.0)
            g = int(clamp(acc_g / 4.0) * 255.0)
            b = int(clamp(acc_b / 4.0) * 255.0)
            a = int(clamp(acc_a / 4.0) * 255.0)

            raw.extend([r, g, b, a])

    def chunk(tag, data):
        return (
            struct.pack(">I", len(data))
            + tag
            + data
            + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
        )

    png = b"\x89PNG\r\n\x1a\n"
    png += chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(bytes(raw), level=6))
    png += chunk(b"IEND", b"")
    return png


def build_ico(png_dict):
    """
    Constructs an ICO file from a mapping of size -> png_bytes.
    Sizes: 16, 32, 48, 64, 128, 256
    """
    entries = []
    image_data = bytearray()
    offset = 6 + 16 * len(png_dict)

    for s, data in png_dict.items():
        w = 0 if s == 256 else s
        h = 0 if s == 256 else s
        entries.append(
            struct.pack(
                "<BBBBHHII",
                w,
                h,
                0,
                0,
                1,
                32,
                len(data),
                offset + len(image_data),
            )
        )
        image_data.extend(data)

    header = struct.pack("<HHH", 0, 1, len(png_dict))
    return header + b"".join(entries) + bytes(image_data)


def main():
    print("Generating Vy icon at 512x512...")
    png_512 = render_vy_icon(512)
    with open(os.path.join(ASSETS_DIR, "icon.png"), "wb") as f:
        f.write(png_512)
    print("Saved assets/icon.png")

    # Generate multi-sizes for iconset and ICO
    sizes = [16, 32, 48, 64, 128, 256]
    pngs = {512: png_512}
    for s in sizes:
        print(f"Generating Vy icon at {s}x{s}...")
        png_s = render_vy_icon(s)
        pngs[s] = png_s
        with open(os.path.join(ASSETS_DIR, f"icon_{s}x{s}.png"), "wb") as f:
            f.write(png_s)

    # Build icon.ico
    print("Building assets/icon.ico...")
    ico_bytes = build_ico({s: pngs[s] for s in [16, 32, 48, 64, 128, 256]})
    with open(os.path.join(ASSETS_DIR, "icon.ico"), "wb") as f:
        f.write(ico_bytes)
    print("Saved assets/icon.ico")

    # Build icon.icns using macOS iconutil if available
    iconset_dir = os.path.join(ASSETS_DIR, "icon.iconset")
    os.makedirs(iconset_dir, exist_ok=True)

    # iconset name conventions
    specs = [
        (16, "icon_16x16.png"),
        (32, "icon_16x16@2x.png"),
        (32, "icon_32x32.png"),
        (64, "icon_32x32@2x.png"),
        (128, "icon_128x128.png"),
        (256, "icon_128x128@2x.png"),
        (256, "icon_256x256.png"),
        (512, "icon_256x256@2x.png"),
        (512, "icon_512x512.png"),
    ]

    for s, name in specs:
        with open(os.path.join(iconset_dir, name), "wb") as f:
            f.write(pngs[s])

    try:
        icns_path = os.path.join(ASSETS_DIR, "icon.icns")
        subprocess.run(
            ["iconutil", "-c", "icns", iconset_dir, "-o", icns_path],
            check=True,
        )
        print(f"Saved assets/icon.icns via iconutil")
        # Clean up iconset folder
        for f in os.listdir(iconset_dir):
            os.remove(os.path.join(iconset_dir, f))
        os.rmdir(iconset_dir)
    except Exception as e:
        print(f"Warning: iconutil failed: {e}")

    print("All icon assets generated successfully.")


if __name__ == "__main__":
    main()

