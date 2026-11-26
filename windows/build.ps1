# Build windows/dist/taskline.exe (one file, no console window).
$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot
python -m pip install --upgrade pip
python -m pip install "pystray>=0.19.5" "Pillow>=10.0" "pyinstaller>=6.10"
python tools/make_icon.py taskline.ico ../assets/logo.png
python tools/make_version_file.py version_info.txt
python -m PyInstaller --noconfirm --clean --onefile --windowed --name taskline `
  --icon taskline.ico --version-file version_info.txt --hidden-import pystray._win32 --collect-submodules taskline `
  --add-data "taskline/assets/logo.png;taskline/assets" `
  --add-data "taskline/assets/icons;taskline/assets/icons" run_taskline.py
if (-not (Test-Path dist/taskline.exe)) { throw "taskline.exe was not produced" }
Get-Item dist/taskline.exe | Select-Object Name, Length
$info = (Get-Item dist/taskline.exe).VersionInfo
Write-Host "FileVersion: $($info.FileVersion)  ProductVersion: $($info.ProductVersion)"
$expected = (Select-String -Path taskline/__init__.py -Pattern '__version__ = "([^"]+)"').Matches[0].Groups[1].Value
if ($info.ProductVersion -ne $expected) { throw "exe ProductVersion '$($info.ProductVersion)' is not $expected" }
