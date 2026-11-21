"""Generates the app backdrop: a dark navy sky with soft clouds and an
ordered-dither texture. Writes raw RGBA with a width/height header."""
from PIL import Image, ImageFilter
import random, struct

W, H = 1200, 750
random.seed(11)
nw, nh = 56, 34
noise = Image.new("L", (nw, nh))
noise.putdata([random.randint(0, 255) for _ in range(nw * nh)])
clouds = noise.resize((W, H), Image.BICUBIC).filter(ImageFilter.GaussianBlur(16))
bayer = [
    [0, 32, 8, 40, 2, 34, 10, 42], [48, 16, 56, 24, 50, 18, 58, 26],
    [12, 44, 4, 36, 14, 46, 6, 38], [60, 28, 52, 20, 62, 30, 54, 22],
    [3, 35, 11, 43, 1, 33, 9, 41], [51, 19, 59, 27, 49, 17, 57, 25],
    [15, 47, 7, 39, 13, 45, 5, 37], [63, 31, 55, 23, 61, 29, 53, 21],
]
img = Image.new("RGB", (W, H))
px = img.load()
cp = clouds.load()

def cl(v):
    return 0 if v < 0 else 255 if v > 255 else int(v)

for y in range(H):
    t = y / (H - 1)
    r = 6 + (14 - 6) * t
    g = 9 + (38 - 9) * t
    b = 20 + (64 - 20) * t
    for x in range(W):
        c = (cp[x, y] / 255.0) ** 2
        lift = c * 46 * (1.0 - 0.45 * t)
        d = (bayer[y % 8][x % 8] / 64.0 - 0.5) * 9
        px[x, y] = (cl(r + lift + d), cl(g + lift * 1.15 + d), cl(b + lift * 1.35 + d))

img = img.convert("RGBA")
with open("crates/perch-app/assets/backdrop.bin", "wb") as f:
    f.write(struct.pack("<II", W, H))
    f.write(img.tobytes())
