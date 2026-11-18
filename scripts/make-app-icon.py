#!/usr/bin/env python3
"""Build the macOS app icon (AppIcon.icns) from the Hebnix logo.

The mark comes from hebnix_rs/crates/hebnix-app/assets/hebnix.png - the same
logo the Windows exe and the Linux launcher use. This script keeps that art,
wraps it in a macOS-style rounded-square icon (dark navy gradient, white
logo), writes a 1024x1024 master PNG, generates every size `iconutil` needs
and emits AppIcon.icns.

Run from anywhere:

    python3 scripts/make-app-icon.py [output-dir]

Requires Pillow (`python3 -m pip install pillow`) plus the stock macOS
`sips`/`iconutil` tools.
"""

import subprocess
import sys
from pathlib import Path

try:
    from PIL import Image, ImageDraw, ImageFilter
except ImportError:
    sys.exit("Pillow is required: python3 -m pip install pillow")

ROOT = Path(__file__).resolve().parent.parent
ASSETS = ROOT / "hebnix_rs/crates/hebnix-app/assets"
LOGO = ASSETS / "hebnix.png"
RUNTIME_PNG = ASSETS / "hebnix-mac.png"
DEFAULT_OUT = ROOT / "macos/Hebnix"

SIZE = 1024
RADIUS = 232          # macOS squircle corner radius at 1024px
LOGO_WIDTH = 640      # logo width inside the 1024px master
GLOW_RADIUS = 460


def gradient_background() -> "Image.Image":
    """Deep indigo -> slate vertical gradient, same palette as the app."""
    image = Image.new("RGBA", (SIZE, SIZE))
    pixels = image.load()
    for y in range(SIZE):
        t = y / (SIZE - 1)
        r = int(24 + (58 - 24) * t)
        g = int(27 + (64 - 27) * t)
        b = int(44 + (104 - 44) * t)
        for x in range(SIZE):
            pixels[x, y] = (r, g, b, 255)
    return image


def soft_glow() -> "Image.Image":
    """A soft accent halo behind the logo so it reads at small sizes."""
    glow = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    draw = ImageDraw.Draw(glow)
    draw.ellipse(
        [SIZE / 2 - GLOW_RADIUS, SIZE / 2 - GLOW_RADIUS,
         SIZE / 2 + GLOW_RADIUS, SIZE / 2 + GLOW_RADIUS],
        fill=(70, 74, 214, 255),
    )
    return glow.filter(ImageFilter.GaussianBlur(170))


def white_logo() -> "Image.Image":
    """Turn the logo's alpha channel into solid white artwork."""
    source = Image.open(LOGO).convert("RGBA")
    scale = LOGO_WIDTH / source.width
    size = (round(source.width * scale), round(source.height * scale))
    alpha = source.getchannel("A").resize(size, Image.LANCZOS)
    logo = Image.new("RGBA", size, (255, 255, 255, 0))
    logo.putalpha(alpha)
    return logo


def make_master() -> "Image.Image":
    master = Image.alpha_composite(gradient_background(), soft_glow())

    logo = white_logo()
    x = (SIZE - logo.width) // 2
    y = (SIZE - logo.height) // 2
    master.alpha_composite(logo, dest=(x, y))

    # Clip to the macOS rounded-square silhouette.
    mask = Image.new("L", (SIZE, SIZE), 0)
    ImageDraw.Draw(mask).rounded_rectangle(
        [0, 0, SIZE - 1, SIZE - 1], radius=RADIUS, fill=255
    )
    master.putalpha(mask)
    return master


def main() -> int:
    out_dir = Path(sys.argv[1]) if len(sys.argv) > 1 else DEFAULT_OUT
    out_dir.mkdir(parents=True, exist_ok=True)

    master = make_master()
    master_path = out_dir / "AppIcon-1024.png"
    master.save(master_path)

    # eframe replaces the Dock icon at runtime via NSApplication, so give it
    # the same artwork as a plain PNG (the .icns is only read at launch).
    master.resize((512, 512), Image.LANCZOS).save(RUNTIME_PNG)

    iconset = out_dir / "AppIcon.iconset"
    if iconset.exists():
        for old in iconset.iterdir():
            old.unlink()
    iconset.mkdir(exist_ok=True)
    for size in (16, 32, 64, 128, 256, 512):
        for scale, suffix in ((1, ""), (2, "@2x")):
            px = size * scale
            target = iconset / f"icon_{size}x{size}{suffix}.png"
            master.resize((px, px), Image.LANCZOS).save(target)

    icns = out_dir / "AppIcon.icns"
    subprocess.run(["iconutil", "-c", "icns", str(iconset), "-o", str(icns)], check=True)
    print(f"wrote {icns}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
