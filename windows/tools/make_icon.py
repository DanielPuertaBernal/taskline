"""Write the .ico used by PyInstaller.

    python tools/make_icon.py out.ico [logo.png]

Uses the project logo when given and present, otherwise the drawn tray icon.
"""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from PIL import Image  # noqa: E402

from taskline.icon import write_ico  # noqa: E402

out = sys.argv[1] if len(sys.argv) > 1 else "taskline.ico"
logo = Path(sys.argv[2]) if len(sys.argv) > 2 else None
if logo and logo.is_file():
    image = Image.open(logo).convert("RGBA")
    side = max(image.size)
    square = Image.new("RGBA", (side, side), (0, 0, 0, 0))
    square.paste(image, ((side - image.width) // 2, (side - image.height) // 2))
    square.save(out, sizes=[(s, s) for s in (16, 24, 32, 48, 64, 128, 256)])
    print(f"icon from {logo}")
else:
    write_ico(out)
    print("icon drawn (no logo found)")
