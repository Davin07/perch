"""Generates the app backdrop. Writes raw RGBA with a width/height header.

The effect is computed at half resolution and nearest-upscaled, so the dither
reads as ~2px blocks; the bloom is thresholded against a Bayer + white-noise
field, which makes its edge dissolve into speckles instead of blur.

Variants:
  nightfall  indigo/violet bloom rising from the bottom-right, star dust
  aurora     wavy dithered band of teal/indigo across the lower third
  horizon    dark ridge with a thin luminous atmosphere line, star field above
"""
import argparse
import struct

import numpy as np
from PIL import Image, ImageFilter

W, H = 1200, 750
FIELD_W, FIELD_H = W // 2, H // 2
BASE = np.array((7, 8, 15), dtype=np.float32)

BAYER = np.array([
    [0, 32, 8, 40, 2, 34, 10, 42], [48, 16, 56, 24, 50, 18, 58, 26],
    [12, 44, 4, 36, 14, 46, 6, 38], [60, 28, 52, 20, 62, 30, 54, 22],
    [3, 35, 11, 43, 1, 33, 9, 41], [51, 19, 59, 27, 49, 17, 57, 25],
    [15, 47, 7, 39, 13, 45, 5, 37], [63, 31, 55, 23, 61, 29, 53, 21],
], dtype=np.float32) / 64.0


def smoothstep(a, b, v):
    t = np.clip((v - a) / (b - a), 0.0, 1.0)
    return t * t * (3 - 2 * t)


def low_noise(seed, blur, octaves=((0.55, 26, 14), (0.30, 64, 5), (0.15, 150, 2))):
    """Low-frequency value noise in 0..1 at field resolution."""
    out = np.zeros((FIELD_H, FIELD_W), dtype=np.float32)
    for weight, cells, b in octaves:
        rng = np.random.default_rng(seed + cells)
        small = Image.fromarray((rng.random((cells, max(2, cells * FIELD_H // FIELD_W))) * 255).astype(np.uint8))
        big = small.resize((FIELD_W, FIELD_H), Image.BICUBIC).filter(ImageFilter.GaussianBlur(b))
        out += weight * np.asarray(big, dtype=np.float32) / 255.0
    return out


def ramp(g, stops):
    out = np.zeros(g.shape + (3,), dtype=np.float32)
    for i in range(len(stops) - 1):
        a, ca = stops[i]
        b, cb = stops[i + 1]
        t = np.clip((g - a) / (b - a + 1e-6), 0.0, 1.0)
        mask = (g >= a) & (g <= b)
        for c in range(3):
            out[..., c] = np.where(mask, ca[c] + (cb[c] - ca[c]) * t, out[..., c])
    out[g < stops[0][0]] = stops[0][1]
    out[g > stops[-1][0]] = stops[-1][1]
    return out


def threshold(seed):
    rng = np.random.default_rng(seed)
    white = rng.random((FIELD_H, FIELD_W)).astype(np.float32)
    bayer = np.tile(BAYER, (FIELD_H // 8 + 1, FIELD_W // 8 + 1))[:FIELD_H, :FIELD_W]
    return 0.55 * white + 0.45 * bayer


def paint(field, stops, th):
    """Colour the part of the field above the dither threshold; base elsewhere."""
    col = ramp(field, stops)
    mask = (field > th)[..., None].astype(np.float32)
    return BASE + (col - BASE) * mask


def add_stars(img, mask, seed, count):
    rng = np.random.default_rng(seed)
    xs = rng.integers(0, W, count)
    ys = rng.integers(0, H, count)
    keep = mask[ys, xs] > rng.random(count)
    xs, ys = xs[keep], ys[keep]
    bright = rng.uniform(24, 132, xs.size) ** 1.25 / 132 ** 0.25
    for x, y, b in zip(xs, ys, bright):
        img[y, x] += np.array((0.82, 0.86, 1.0), dtype=np.float32) * b
        if b > 100:
            img[y, x] = np.minimum(255, img[y, x] + 24)
    return img


def build(variant):
    x = (np.arange(FIELD_W, dtype=np.float32) / (FIELD_W - 1))[None, :]
    y = (np.arange(FIELD_H, dtype=np.float32) / (FIELD_H - 1))[:, None]
    noise = low_noise(11, 0)
    th = threshold(7)
    grain = np.random.default_rng(3).random((H, W)).astype(np.float32) - 0.5

    if variant == "nightfall":
        s = 0.80 * x + 0.62 * y + (noise - 0.5) * 0.06
        field = smoothstep(0.94, 1.32, s)
        rim = np.exp(-((s - 1.08) / 0.04) ** 2) * 0.22
        field = np.clip(field * 0.88 + rim, 0, 0.92)
        stops = [(0.0, (18, 17, 42)), (0.34, (34, 28, 82)), (0.60, (66, 42, 126)),
                 (0.80, (112, 54, 158)), (0.92, (160, 72, 174)), (1.0, (196, 104, 184))]
    elif variant == "aurora":
        yc = 0.62 + (noise - 0.5) * 0.10
        band = np.exp(-((y - yc) / 0.17) ** 2)
        band *= 0.55 + 0.45 * low_noise(29, 0)
        field = np.clip(band ** 1.35, 0, 1)
        stops = [(0.0, (10, 26, 40)), (0.35, (18, 56, 88)), (0.60, (34, 64, 132)),
                 (0.80, (76, 72, 170)), (1.0, (132, 112, 200))]
    elif variant == "horizon":
        ridge = 0.745 + 0.022 * np.sin(x * 7.0 + 1.2) + (noise - 0.5) * 0.012
        above = (y < ridge)
        line = np.exp(-((y - ridge) / 0.0045) ** 2) * 0.95
        glow = np.exp(-((y - ridge) / 0.14) ** 2) * 0.42
        field = np.clip((line + glow) * above, 0, 1)
        field = np.where(y > ridge, 0.0, field)
        stops = [(0.0, (12, 14, 34)), (0.30, (26, 30, 78)), (0.55, (54, 44, 120)),
                 (0.78, (104, 60, 152)), (1.0, (208, 122, 188))]
    else:
        raise SystemExit(f"unknown variant {variant}")

    field_img = paint(field, stops, th)
    img = np.asarray(
        Image.fromarray(np.clip(field_img, 0, 255).astype(np.uint8)).resize((W, H), Image.NEAREST),
        dtype=np.float32,
    )
    dark = 1.0 - smoothstep(0.0, 0.30, np.asarray(
        Image.fromarray((field * 255).astype(np.uint8)).resize((W, H), Image.BILINEAR),
        dtype=np.float32,
    ) / 255.0)
    if variant == "horizon":
        # keep the land itself black; stars only in the sky
        yf = (np.arange(H, dtype=np.float32) / (H - 1))[:, None]
        land = (yf > 0.745).astype(np.float32) * np.ones((1, W), dtype=np.float32)
        dark = dark * (1.0 - land)
    img = add_stars(img, dark * 0.92, 5, 650 if variant == "nightfall" else 520)
    img += grain[..., None] * 2.2
    return np.clip(img, 0, 255).astype(np.uint8)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--variant", default="nightfall", choices=["nightfall", "aurora", "horizon"])
    ap.add_argument("--out", default="crates/perch-app/assets/backdrop.bin")
    ap.add_argument("--preview", default="")
    args = ap.parse_args()

    rgb = build(args.variant)
    rgba = np.dstack([rgb, np.full((H, W), 255, np.uint8)])
    with open(args.out, "wb") as f:
        f.write(struct.pack("<II", W, H))
        f.write(rgba.tobytes())
    if args.preview:
        Image.fromarray(rgb).save(args.preview)


if __name__ == "__main__":
    main()
