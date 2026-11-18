"""Draws the Soundy icon set: a dark glass tile with an emerald waveform.

The artwork is drawn at 4x and downsampled, so every size stays crisp without
depending on a vector rasteriser. Run from the repository root:

    python scripts/generate-soundy-icons.py
"""

from __future__ import annotations

import struct
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter

ROOT = Path(__file__).resolve().parent.parent
ASSETS = ROOT / "assets"

SCALE = 4
SIZE = 512
S = SIZE * SCALE

BACKGROUND_TOP = (11, 13, 15, 255)
BACKGROUND_BOTTOM = (20, 24, 26, 255)
EMERALD_TOP = (110, 231, 183, 255)
EMERALD_BOTTOM = (5, 150, 105, 255)
GLOW = (16, 185, 129, 110)

# The same geometry the SVG uses, multiplied by SCALE.
BARS = [
    (74, 200, 44, 112),
    (154, 170, 44, 172),
    (234, 138, 44, 236),
    (314, 166, 44, 180),
    (394, 188, 44, 136),
]


def bar_radius(height: int) -> int:
    return 22 * SCALE


def vertical_gradient(size: tuple[int, int], top: tuple[int, ...], bottom: tuple[int, ...]) -> Image.Image:
    width, height = size
    image = Image.new("RGBA", size)
    draw = ImageDraw.Draw(image)
    for y in range(height):
        t = y / max(height - 1, 1)
        colour = tuple(round(a + (b - a) * t) for a, b in zip(top, bottom))
        draw.line([(0, y), (width, y)], fill=colour)
    return image


def rounded_mask(size: tuple[int, int], radius: int) -> Image.Image:
    mask = Image.new("L", size, 0)
    ImageDraw.Draw(mask).rounded_rectangle([0, 0, size[0] - 1, size[1] - 1], radius=radius, fill=255)
    return mask


def draw_bars(fill: Image.Image | None = None, colour: tuple[int, ...] | None = None) -> Image.Image:
    layer = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    draw = ImageDraw.Draw(layer)
    for x, y, width, height in BARS:
        box = [x * SCALE, y * SCALE, (x + width) * SCALE, (y + height) * SCALE]
        if fill is not None:
            mask = Image.new("L", (S, S), 0)
            ImageDraw.Draw(mask).rounded_rectangle(box, radius=bar_radius(height), fill=255)
            layer.paste(fill, (0, 0), mask)
        else:
            draw.rounded_rectangle(box, radius=bar_radius(height), fill=colour)
    return layer


def render() -> Image.Image:
    background = vertical_gradient((S, S), BACKGROUND_TOP, BACKGROUND_BOTTOM)
    tile = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    tile.paste(background, (0, 0), rounded_mask((S, S), 112 * SCALE))

    # A soft emerald halo behind the bars gives the tile its glass depth.
    halo = draw_bars(colour=GLOW).filter(ImageFilter.GaussianBlur(48 * SCALE))
    tile.alpha_composite(halo)

    bars = draw_bars(fill=vertical_gradient((S, S), EMERALD_TOP, EMERALD_BOTTOM))
    tile.alpha_composite(bars)

    # A thin rim and a top highlight read as a polished edge.
    rim = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    rim_draw = ImageDraw.Draw(rim)
    inset = 2 * SCALE
    rim_draw.rounded_rectangle(
        [inset, inset, S - 1 - inset, S - 1 - inset],
        radius=(112 - 2) * SCALE,
        outline=(255, 255, 255, 26),
        width=2 * SCALE,
    )
    tile.alpha_composite(rim.filter(ImageFilter.GaussianBlur(0.5 * SCALE)))

    highlight = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    ImageDraw.Draw(highlight).rounded_rectangle(
        [18 * SCALE, 14 * SCALE, S - 18 * SCALE, 122 * SCALE],
        radius=96 * SCALE,
        fill=(255, 255, 255, 10),
    )
    tile.alpha_composite(highlight.filter(ImageFilter.GaussianBlur(18 * SCALE)))

    return tile.resize((SIZE, SIZE), Image.Resampling.LANCZOS)


def write_png(image: Image.Image, path: Path, size: int) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    image.resize((size, size), Image.Resampling.LANCZOS).save(path)


def write_icns(image: Image.Image, path: Path) -> None:
    """Writes a PNG-based ICNS without a macOS host."""
    chunks = [
        (b"icp4", 16),
        (b"icp5", 32),
        (b"icp6", 64),
        (b"ic07", 128),
        (b"ic08", 256),
        (b"ic09", 512),
        (b"ic10", 1024),
    ]
    payload = bytearray()
    for code, size in chunks:
        import io

        buffer = io.BytesIO()
        image.resize((size, size), Image.Resampling.LANCZOS).save(buffer, format="PNG")
        data = buffer.getvalue()
        payload += code + struct.pack(">I", len(data) + 8) + data
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(b"icns" + struct.pack(">I", len(payload) + 8) + payload)


SVG = """<svg xmlns="http://www.w3.org/2000/svg" width="512" height="512" viewBox="0 0 512 512">
  <defs>
    <linearGradient id="tile" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="#0b0d0f"/>
      <stop offset="1" stop-color="#14181a"/>
    </linearGradient>
    <linearGradient id="wave" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="#6ee7b7"/>
      <stop offset="1" stop-color="#059669"/>
    </linearGradient>
    <radialGradient id="halo" cx="0.5" cy="0.52" r="0.5">
      <stop offset="0" stop-color="#10b981" stop-opacity="0.45"/>
      <stop offset="1" stop-color="#10b981" stop-opacity="0"/>
    </radialGradient>
  </defs>
  <rect width="512" height="512" rx="112" fill="url(#tile)"/>
  <ellipse cx="256" cy="266" rx="196" ry="150" fill="url(#halo)"/>
  <g fill="url(#wave)">
    <rect x="74" y="200" width="44" height="112" rx="22"/>
    <rect x="154" y="170" width="44" height="172" rx="22"/>
    <rect x="234" y="138" width="44" height="236" rx="22"/>
    <rect x="314" y="166" width="44" height="180" rx="22"/>
    <rect x="394" y="188" width="44" height="136" rx="22"/>
  </g>
  <rect x="2" y="2" width="508" height="508" rx="110" fill="none" stroke="#ffffff" stroke-opacity="0.10" stroke-width="2"/>
</svg>
"""


def main() -> None:
    icon = render()

    for folder in (
        ASSETS / "branding",
        ASSETS / "windows",
        ASSETS / "macos",
        ASSETS / "linux",
        ASSETS / "tray",
    ):
        folder.mkdir(parents=True, exist_ok=True)

    write_png(icon, ASSETS / "branding" / "soundy-1024.png", 1024)
    write_png(icon, ASSETS / "branding" / "soundy-512.png", 512)
    (ASSETS / "branding" / "soundy.svg").write_text(SVG, encoding="utf-8")
    (ASSETS / "windows" / "soundy.svg").write_text(SVG, encoding="utf-8")
    (ASSETS / "macos" / "soundy.svg").write_text(SVG, encoding="utf-8")
    (ASSETS / "linux" / "soundy.svg").write_text(SVG, encoding="utf-8")

    ico = [16, 20, 24, 32, 40, 48, 64, 80, 96, 128, 256]
    icon.save(
        ASSETS / "windows" / "soundy.ico",
        format="ICO",
        sizes=[(size, size) for size in ico],
    )

    for size in (16, 22, 24, 32, 48, 64, 128, 256, 512):
        write_png(icon, ASSETS / "linux" / "icons" / "hicolor" / f"{size}x{size}" / "apps" / "soundy.png", size)

    write_png(icon, ASSETS / "macos" / "soundy-1024.png", 1024)
    write_icns(icon, ASSETS / "macos" / "soundy.icns")

    # macOS template artwork is the waveform alone, in black with alpha; AppKit tints it
    # for the menu bar. The same geometry keeps the tray mark recognisably Soundy.
    tray = draw_bars(colour=(0, 0, 0, 255))
    write_png(tray, ASSETS / "tray" / "template-18.png", 18)
    write_png(tray, ASSETS / "tray" / "template-36.png", 36)

    print("wrote Soundy icon set")


if __name__ == "__main__":
    main()
