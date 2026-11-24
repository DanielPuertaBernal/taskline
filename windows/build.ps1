# Build windows/dist/taskline.exe (one file, no console window).
$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot
python -m pip install --upgrade pip
python -m pip install "pystray>=0.19.5" "Pillow>=10.0" "pyinstaller>=6.10"
python tools/make_icon.py taskline.ico ../assets/logo.png
python -m PyInstaller --noconfirm --clean --onefile --windowed --name taskline `
  --icon taskline.ico --hidden-import pystray._win32 --collect-submodules taskline `
  --add-data "taskline/assets/logo.png;taskline/assets" run_taskline.py
if (-not (Test-Path dist/taskline.exe)) { throw "taskline.exe was not produced" }
Get-Item dist/taskline.exe | Select-Object Name, Length
