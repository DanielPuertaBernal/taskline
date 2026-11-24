"""Render assets/social-preview.png (1280x640) from assets/logo.png. Needs Pillow."""

from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parents[1]
W, H = 1280, 640


def font(size, bold=False):
    names = (["JetBrainsMonoNerdFont-Bold.ttf", "DejaVuSans-Bold.ttf"] if bold else []) + [
        "JetBrainsMonoNerdFont-Regular.ttf", "DejaVuSans.ttf"]
    for name in names:
        for base in ("/usr/share/fonts/TTF", "/usr/share/fonts/truetype/dejavu"):
            try:
                return ImageFont.truetype(f"{base}/{name}", size)
            except OSError:
                pass
    return ImageFont.load_default()


im = Image.new("RGB", (W, H), "#1a1b26")
logo = Image.open(ROOT / "assets/logo.png").convert("RGBA").resize((300, 300), Image.LANCZOS)
im.paste(logo, (110, 170), logo)
d = ImageDraw.Draw(im)
d.text((470, 200), "Taskline", fill="#c0caf5", font=font(96, True))
d.text((474, 330), "Your day, as one line.", fill="#7aa2f7", font=font(40))
x, y = 476, 412
for i in range(10):
    color = "#7aa2f7" if i < 3 else None
    d.rounded_rectangle((x, y, x + 30, y + 22), radius=5, fill=color, outline="#7aa2f7" if i < 3 else "#565f89", width=2)
    x += 38
d.text((x + 20, 400), "1/4", fill="#c0caf5", font=font(40))
d.text((474, 470), "Omarchy · Windows · MIT", fill="#565f89", font=font(30))
im.save(ROOT / "assets/social-preview.png")
