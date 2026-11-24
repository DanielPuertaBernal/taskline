# Run the built exe in selftest mode: it drives the real UI against a temp data
# dir, checks the task model end to end, and saves screenshots.
$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot
$report = Join-Path $PWD "dist/selftest.txt"
$shots = Join-Path $PWD "dist/screenshots"
$p = Start-Process -FilePath "dist/taskline.exe" -ArgumentList @("--selftest", "`"$report`"", "--screenshots", "`"$shots`"") -Wait -PassThru
Get-Content $report
if ($p.ExitCode -ne 0) { throw "selftest failed with exit code $($p.ExitCode)" }
