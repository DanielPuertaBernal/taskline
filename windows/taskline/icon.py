"""Tray and application icons drawn with Pillow (no binary assets in the repo)."""

from __future__ import annotations

from pathlib import Path

from PIL import Image, ImageDraw

from . import theme

LOGO = Path(__file__).with_name("assets") / "logo.png"


def logo(size: int = 64) -> Image.Image:
    """The app logo, or the drawn icon if the bundled PNG is missing."""
    try:
        return Image.open(LOGO).convert("RGBA").resize((size, size), Image.LANCZOS)
    except OSError:
        return render(size=size)


def render(done: int = 1, total: int = 3, size: int = 64) -> Image.Image:
    """A rounded dark tile with three progress segments filled by ratio."""
    image = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    draw = ImageDraw.Draw(image)
    radius = size // 5
    draw.rounded_rectangle((0, 0, size - 1, size - 1), radius=radius, fill=theme.BG)
    segments = 3
    ratio = done / total if total else 0
    filled = min(segments, int(ratio * segments + 0.5))
    pad = size // 6
    gap = max(1, size // 16)
    seg_w = (size - 2 * pad - (segments - 1) * gap) / segments
    top, bottom = size * 0.38, size * 0.62
    for i in range(segments):
        x0 = pad + i * (seg_w + gap)
        color = theme.ACCENT if i < filled else theme.MUTED
        draw.rounded_rectangle((x0, top, x0 + seg_w, bottom), radius=max(1, size // 32), fill=color)
    return image


def write_ico(path: str) -> None:
    sizes = [16, 24, 32, 48, 64, 128, 256]
    render(size=256).save(path, sizes=[(s, s) for s in sizes])
