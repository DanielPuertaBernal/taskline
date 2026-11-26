# Runs the built exe's selftest: demo tasks in a temporary folder, real windows, both
# themes captured, the updater against a local fake release, and (in CI) the Run key.
param(
    [string]$Exe = "target/release/taskline.exe",
    [string]$Out = "dist"
)
$ErrorActionPreference = "Stop"
$exe = (Resolve-Path $Exe).Path
New-Item -ItemType Directory -Force $Out | Out-Null
$out = (Resolve-Path $Out).Path
$report = Join-Path $out "selftest.txt"
Remove-Item -Force -ErrorAction SilentlyContinue $report
$process = Start-Process $exe -ArgumentList "--selftest", "`"$report`"", "--screenshots", "`"$out\screenshots`"" -PassThru
if (-not $process.WaitForExit(180000)) {
    Stop-Process -Id $process.Id -Force
    throw "selftest timed out"
}
Get-Content $report
if ($process.ExitCode -ne 0) { throw "selftest failed with exit code $($process.ExitCode)" }
$size = (Get-Item $exe).Length
"taskline.exe: {0:N0} bytes ({1:N1} MB)" -f $size, ($size / 1MB) | Tee-Object -FilePath (Join-Path $out "size.txt")
