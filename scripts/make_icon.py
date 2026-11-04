"""Draws the app icon, an A4 portrait sheet with a bookmark on a rounded square,
to assets/icon.png (1024 px, for nsh.json) and assets/icon.ico (Windows)."""
import os

from PIL import Image, ImageDraw, ImageFilter

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "..", "assets")
S = 4  # drawn at 4x, then reduced for smooth edges
N = 1024 * S


def px(v):
    return int(round(v * S))


def gradient(size, top, bottom):
    w, h = size
    g = Image.new("RGBA", (1, h))
    for y in range(h):
        t = y / (h - 1)
        g.putpixel((0, y), tuple(int(top[i] + (bottom[i] - top[i]) * t) for i in range(3)) + (255,))
    return g.resize((w, h))


def rounded_mask(size, box, radius):
    m = Image.new("L", size, 0)
    ImageDraw.Draw(m).rounded_rectangle(box, radius=radius, fill=255)
    return m


def sheet(w, h, fold, fill, fold_fill):
    """A sheet whose top right corner is folded down."""
    img = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    r = px(18)
    d.rounded_rectangle((0, 0, w - 1, h - 1), radius=r, fill=fill)
    # Cut the corner, then lay the fold over it.
    d.polygon([(w - fold, 0), (w, 0), (w, fold)], fill=(0, 0, 0, 0))
    d.polygon([(w - fold, 0), (w - fold, fold), (w, fold)], fill=fold_fill)
    return img


def main():
    os.makedirs(OUT, exist_ok=True)
    icon = Image.new("RGBA", (N, N), (0, 0, 0, 0))

    # The tile: a deep blue gradient with rounded corners.
    margin, radius = px(56), px(200)
    tile = gradient((N, N), (76, 110, 245), (30, 36, 112))
    icon.paste(tile, (0, 0), rounded_mask((N, N), (margin, margin, N - margin, N - margin), radius))

    # A4 portrait: 210 x 297.
    sh = px(640)
    sw = int(sh * 210 / 297)
    fold = px(104)
    x0 = (N - sw) // 2 + px(18)
    y0 = (N - sh) // 2 + px(24)

    # The page underneath, turned a little.
    under = Image.new("RGBA", (sw, sh), (0, 0, 0, 0))
    ImageDraw.Draw(under).rounded_rectangle((0, 0, sw - 1, sh - 1), radius=px(18), fill=(186, 200, 248, 255))
    under = under.rotate(6, resample=Image.BICUBIC, expand=True)
    icon.alpha_composite(under, (x0 - px(66), y0 - px(30)))

    # The shadow of the top sheet.
    shadow = Image.new("RGBA", (N, N), (0, 0, 0, 0))
    ImageDraw.Draw(shadow).rounded_rectangle(
        (x0 + px(10), y0 + px(22), x0 + sw + px(10), y0 + sh + px(22)), radius=px(20), fill=(10, 14, 60, 120)
    )
    icon.alpha_composite(shadow.filter(ImageFilter.GaussianBlur(px(22))))

    # The top sheet with lines of text.
    top = sheet(sw, sh, fold, (255, 255, 255, 255), (214, 222, 245, 255))
    d = ImageDraw.Draw(top)
    left, right = px(62), sw - px(62)
    ink, heading = (190, 198, 220, 255), (70, 82, 140, 255)
    y = px(196)
    d.rounded_rectangle((left, y, left + px(190), y + px(30)), radius=px(15), fill=heading)
    y += px(78)
    for i, end in enumerate([1.0, 1.0, 1.0, 0.62, None, 1.0, 1.0, 1.0, 0.8]):
        if end is None:
            y += px(22)
            continue
        indent = px(36) if i in (0, 5, 10) else 0
        x_end = left + int((right - left) * end)
        d.rounded_rectangle((left + indent, y, x_end, y + px(18)), radius=px(9), fill=ink)
        y += px(36)
    icon.alpha_composite(top, (x0, y0))

    # The bookmark hanging from the top edge.
    bx, bw, bh = x0 + sw - fold - px(110), px(64), px(190)
    by = y0 - px(26)
    ribbon = [(bx, by), (bx + bw, by), (bx + bw, by + bh), (bx + bw // 2, by + bh - px(40)), (bx, by + bh)]
    ImageDraw.Draw(icon).polygon(ribbon, fill=(255, 122, 26, 255))
    ImageDraw.Draw(icon).polygon(
        [(bx, by), (bx + bw, by), (bx + bw, by + px(26)), (bx, by + px(26))], fill=(214, 88, 10, 255)
    )

    icon = icon.resize((1024, 1024), Image.LANCZOS)
    png = os.path.join(OUT, "icon.png")
    icon.save(png)
    ico = os.path.join(OUT, "icon.ico")
    icon.save(ico, sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])
    print("wrote", os.path.normpath(png), "and", os.path.normpath(ico))


if __name__ == "__main__":
    main()
