"""Builds System/bezels/gb/640x480.png from the RG35XXSP bezel: black side
panels beside a 3x Game Boy picture, the original's edge shadow along the
picture's sides, and a power LED with its label on the left panel.

The LED is drawn unlit, a dark lens in its rim. slot lights it, coloured
by the battery, from System/bezels/gb/640x480.light beside the image,
which this writes too: the lens's middle and radius in the image's
pixels."""
import sys
from PIL import Image, ImageDraw, ImageFont

base_path, font_path, out_path = sys.argv[1:4]
base = Image.open(base_path).convert('RGBA')
W, H = base.size
RAMP = [147, 118, 75, 44, 23, 11, 5, 2, 1]
LEFT, RIGHT = 80, 560

out = base.copy()
px, b = out.load(), base.load()
for y in range(H):
    for x in range(W):
        r, g, bb, a = b[x, y]
        if x < LEFT or x >= RIGHT:
            px[x, y] = (0, 0, 0, 255)
            continue
        d = min(x - LEFT, RIGHT - 1 - x)
        if d < len(RAMP):
            a = max(a, RAMP[d])
        px[x, y] = (r, g, bb, a)

# The power LED, drawn at 4x and shrunk so its edge is smooth: a rim and
# an unlit lens, which slot lights.
K = 4
cx, cy = 40, 132
layer = Image.new('RGBA', (LEFT * K, 200 * K), (0, 0, 0, 0))
d = ImageDraw.Draw(layer)
rim, lit = 5.0, 4.0
d.ellipse(((cx - rim) * K, (cy - rim) * K, (cx + rim) * K, (cy + rim) * K), fill=(26, 26, 30, 255))
d.ellipse(((cx - lit) * K, (cy - lit) * K, (cx + lit) * K, (cy + lit) * K), fill=(44, 42, 44, 255))
hx, hy = cx - 1.4, cy - 1.6
d.ellipse(((hx - 1.4) * K, (hy - 1.1) * K, (hx + 1.4) * K, (hy + 1.1) * K), fill=(90, 88, 92, 160))
font = ImageFont.truetype(font_path, 9 * K)
text, track = 'POWER', 1.2 * K
widths = [font.getlength(c) for c in text]
x0 = cx * K - (sum(widths) + track * (len(text) - 1)) / 2
for c, w in zip(text, widths):
    d.text((x0, (cy + 10) * K), c, font=font, fill=(150, 150, 160, 255))
    x0 += w + track
small = layer.resize((LEFT, 200), Image.LANCZOS)
panel = out.crop((0, 0, LEFT, 200))
panel.alpha_composite(small)
out.paste(panel, (0, 0))
out.save(out_path, optimize=True)
with open(out_path[:-len('.png')] + '.light', 'w') as f:
    f.write(f'{cx} {cy} {lit}\n')
