"""Write a PyInstaller --version-file from taskline.__version__ (exe Properties > Details)."""

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
version = re.search(r'__version__ = "([^"]+)"', (ROOT / "taskline" / "__init__.py").read_text()).group(1)
parts = tuple(int(p) for p in version.split(".")) + (0,)
numbers = ", ".join(str(p) for p in parts[:4])

TEMPLATE = f"""VSVersionInfo(
  ffi=FixedFileInfo(filevers=({numbers}), prodvers=({numbers}), mask=0x3f, flags=0x0, OS=0x40004,
                    fileType=0x1, subtype=0x0, date=(0, 0)),
  kids=[
    StringFileInfo([StringTable('040904B0', [
      StringStruct('CompanyName', 'Daniel Puerta'),
      StringStruct('FileDescription', 'Taskline - daily task progress bar'),
      StringStruct('FileVersion', '{version}'),
      StringStruct('InternalName', 'taskline'),
      StringStruct('LegalCopyright', '(c) 2026 Daniel Puerta, MIT License'),
      StringStruct('OriginalFilename', 'taskline.exe'),
      StringStruct('ProductName', 'Taskline'),
      StringStruct('ProductVersion', '{version}')])]),
    VarFileInfo([VarStruct('Translation', [1033, 1200])])
  ]
)
"""

out = Path(sys.argv[1] if len(sys.argv) > 1 else "version_info.txt")
out.write_text(TEMPLATE, encoding="utf-8")
print(f"{out}: {version}")
