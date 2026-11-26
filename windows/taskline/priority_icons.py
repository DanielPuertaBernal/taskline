"""Priority icons: the bundled Lucide SVGs (assets/icons, ISC license) drawn
with Pillow, since Tk cannot display SVG. The icons are simple stroked
polylines, so a tiny parser for `<path d="m.. l..">` and `<line>` is enough.
"""

from __future__ import annotations

import re
from functools import lru_cache
from pathlib import Path

from PIL import Image, ImageDraw

ICON_DIR = Path(__file__).with_name("assets") / "icons"
FILES = {"H": "priority-high.svg", "M": "priority-medium.svg", "L": "priority-low.svg"}

_NUMBER = re.compile(r"[-+]?(?:\d+\.?\d*|\.\d+)")
_COMMAND = re.compile(r"([MmLl])([^MmLl]*)")


def _path_polylines(d: str) -> list[list[tuple[float, float]]]:
    lines: list[list[tuple[float, float]]] = []
    x = y = 0.0
    for command, args in _COMMAND.findall(d):
        numbers = [float(n) for n in _NUMBER.findall(args)]
        pairs = list(zip(numbers[::2], numbers[1::2], strict=False))  # an odd trailing number is ignored
        relative = command.islower()
        for index, (dx, dy) in enumerate(pairs):
            x, y = (x + dx, y + dy) if relative else (dx, dy)
            if command in "Mm" and index == 0:
                lines.append([(x, y)])
            else:  # implicit lineto after a moveto, or an explicit one
                lines[-1].append((x, y))
    return lines


def parse_svg(text: str) -> list[list[tuple[float, float]]]:
    """Polylines (in the 24x24 viewBox) of every <path> and <line> element."""
    polylines = []
    for d in re.findall(r'<path[^>]*\sd="([^"]+)"', text):
        polylines += _path_polylines(d)
    for attrs in re.findall(r"<line([^>]*)>", text):
        values = dict(re.findall(r'(x1|x2|y1|y2)="([^"]+)"', attrs))
        polylines.append([(float(values["x1"]), float(values["y1"])), (float(values["x2"]), float(values["y2"]))])
    return polylines


@lru_cache(maxsize=64)
def render(priority: str, color: str, size: int = 18) -> Image.Image | None:
    """The priority's icon in `color`, or None when there is no priority."""
    name = FILES.get(priority or "")
    if name is None:
        return None
    polylines = parse_svg((ICON_DIR / name).read_text(encoding="utf-8"))
    scale = 8  # draw large and downsample for smooth edges
    big = size * scale
    unit = big / 24
    width = max(1, round(2 * unit))  # stroke-width="2"
    # Transparent pixels carry the icon color so downsampling leaves no dark fringe.
    rgb = Image.new("RGB", (1, 1), color).getpixel((0, 0))
    image = Image.new("RGBA", (big, big), (*rgb, 0))
    draw = ImageDraw.Draw(image)
    for line in polylines:
        points = [(px * unit, py * unit) for px, py in line]
        draw.line(points, fill=color, width=width, joint="curve")
        for px, py in (points[0], points[-1]):  # stroke-linecap="round"
            draw.ellipse((px - width / 2, py - width / 2, px + width / 2, py + width / 2), fill=color)
    return image.resize((size, size), Image.BOX)


@lru_cache(maxsize=16)
def calendar_icon(color: str, size: int = 18) -> Image.Image:
    """A small calendar glyph (rounded page, header line, two rings), like Lucide's."""
    scale = 8
    big = size * scale
    unit = big / 24
    width = max(1, round(2 * unit))
    rgb = Image.new("RGB", (1, 1), color).getpixel((0, 0))
    image = Image.new("RGBA", (big, big), (*rgb, 0))
    draw = ImageDraw.Draw(image)
    draw.rounded_rectangle((3 * unit, 4 * unit, 21 * unit, 22 * unit), radius=2 * unit, outline=color, width=width)
    draw.line((3 * unit, 10 * unit, 21 * unit, 10 * unit), fill=color, width=width)
    for x in (8, 16):
        draw.line((x * unit, 2 * unit, x * unit, 6 * unit), fill=color, width=width)
    return image.resize((size, size), Image.BOX)
