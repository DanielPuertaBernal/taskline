# End-to-end update compatibility on a real Windows machine (CI):
#  1. The published Taskline v1.2.0 (Python) finds this build as a newer release on a
#     local fake GitHub API, verifies it against SHA256SUMS.txt, swaps it in with its
#     own updater and starts it. The new exe must read v1.2's settings and tasks.
#  2. This build then updates itself the same way to a fake v99.0.0.
# Only demo data in a temporary folder is used.
param(
    [string]$Exe = "target/release/taskline.exe",
    [string]$Old = "",
    [int]$Port = 8765
)
$ErrorActionPreference = "Stop"
$new = (Resolve-Path $Exe).Path
$root = Join-Path ([IO.Path]::GetTempPath()) ("taskline-upgrade-" + [guid]::NewGuid().ToString("N"))
$app = New-Item -ItemType Directory -Force (Join-Path $root "app")
$data = New-Item -ItemType Directory -Force (Join-Path $root "data")
$srv = New-Item -ItemType Directory -Force (Join-Path $root "srv")
$installed = Join-Path $app "taskline.exe"

function Hash($path) { (Get-FileHash -Algorithm SHA256 $path).Hash.ToLower() }

function Wait-Until([scriptblock]$condition, [int]$seconds, [string]$what) {
    $deadline = (Get-Date).AddSeconds($seconds)
    while ((Get-Date) -lt $deadline) {
        if (& $condition) { return }
        Start-Sleep -Milliseconds 500
    }
    throw "timed out waiting for: $what"
}

function Publish([string]$tag, [string]$exePath) {
    Copy-Item -Force $exePath (Join-Path $srv "taskline.exe")
    "$(Hash (Join-Path $srv 'taskline.exe'))  taskline.exe" | Set-Content -NoNewline -Encoding ascii (Join-Path $srv "SHA256SUMS.txt")
    $base = "http://127.0.0.1:$Port"
    @{
        tag_name = $tag
        assets = @(
            @{ name = "taskline.exe"; browser_download_url = "$base/taskline.exe" },
            @{ name = "SHA256SUMS.txt"; browser_download_url = "$base/SHA256SUMS.txt" }
        )
    } | ConvertTo-Json -Depth 4 | ForEach-Object { [IO.File]::WriteAllText((Join-Path $srv "api.json"), $_) }
}

# The real v1.2.0 exe from the release, unless one is given.
if ($Old -eq "") {
    $Old = Join-Path $root "taskline-v1.2.0.exe"
    Invoke-WebRequest -UseBasicParsing -OutFile $Old "https://github.com/DanielPuertaBernal/taskline/releases/download/v1.2.0/taskline.exe"
}
Copy-Item $Old $installed
$oldHash = Hash $installed

# v1.2 data: tasks, schema sidecar and settings as v1.2 wrote them (demo tasks).
$fixtures = Join-Path $PSScriptRoot "..\crates\taskline-core\tests\fixtures\v1.2"
Copy-Item (Join-Path $fixtures "tasks.json"), (Join-Path $fixtures "tasks.meta.json") $data
'{"x": 40, "y": 40, "sort": "priority", "theme": "light", "auto_update": true, "last_update_check": "2000-01-01"}' |
    Set-Content -NoNewline -Encoding ascii (Join-Path $data "settings.json")
$tasksHash = Hash (Join-Path $data "tasks.json")

# A tiny static file server for the fake release (PowerShell's HttpListener).
$server = Start-Job -ArgumentList $srv.FullName, $Port -ScriptBlock {
    param($root, $port)
    $listener = [System.Net.HttpListener]::new()
    $listener.Prefixes.Add("http://127.0.0.1:$port/")
    $listener.Start()
    while ($listener.IsListening) {
        $context = $listener.GetContext()
        $name = $context.Request.Url.AbsolutePath.TrimStart("/")
        $file = Join-Path $root $name
        if ($name -and (Test-Path -LiteralPath $file -PathType Leaf)) {
            $bytes = [IO.File]::ReadAllBytes($file)
            $context.Response.StatusCode = 200
            $context.Response.ContentLength64 = $bytes.Length
            $context.Response.OutputStream.Write($bytes, 0, $bytes.Length)
        } else {
            $context.Response.StatusCode = 404
        }
        $context.Response.Close()
    }
}
try {
    Start-Sleep -Seconds 2
    $env:TASKLINE_DATA_DIR = $data
    $env:TASKLINE_UPDATE_API = "http://127.0.0.1:$Port/api.json"

    # ---- 1. v1.2.0 (Python) -> this build
    $version = (Select-String -Path (Join-Path $PSScriptRoot "..\Cargo.toml") -Pattern '^version = "(.+)"').Matches[0].Groups[1].Value
    Publish "v$version" $new
    Write-Host "v1.2.0 -> v$version"
    Start-Process $installed | Out-Null
    Wait-Until { (Test-Path $installed) -and ((Hash $installed) -eq (Hash $new)) } 120 "v1.2.0 to install the new exe"
    Wait-Until { (Test-Path "$installed.bak") -and ((Hash "$installed.bak") -eq $oldHash) } 30 "taskline.exe.bak to be v1.2.0"
    Wait-Until { (Get-Content -Raw (Join-Path $app "taskline-update.log")) -match "updated" } 30 "the update log"
    # The new build started and took the notice v1.2 left for it.
    Wait-Until {
        $s = Get-Content -Raw (Join-Path $data "settings.json") | ConvertFrom-Json
        -not ($s.PSObject.Properties.Name -contains "update_notice")
    } 60 "the new build to read settings.json"
    $settings = Get-Content -Raw (Join-Path $data "settings.json") | ConvertFrom-Json
    if ($settings.theme -ne "light" -or $settings.sort -ne "priority" -or $settings.x -ne 40) {
        throw "settings from v1.2 were not kept: $(Get-Content -Raw (Join-Path $data 'settings.json'))"
    }
    if ((Hash (Join-Path $data "tasks.json")) -ne $tasksHash) { throw "tasks.json changed on upgrade" }
    if (Get-ChildItem $data -Filter "tasks.json.bak-*") { throw "an unnecessary migration backup was made" }
    $running = Get-Process taskline -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $installed }
    if (-not $running) { throw "the new build is not running after the update" }
    Write-Host "OK: v1.2.0 updated itself to the Rust build; tasks and settings kept"
    $running | Stop-Process -Force
    Start-Sleep -Seconds 2

    # ---- 2. this build -> a fake v99.0.0 (same bytes), through its own updater
    Copy-Item -Force $new (Join-Path $root "next.exe")
    Publish "v99.0.0" (Join-Path $root "next.exe")
    '{"auto_update": true, "last_update_check": "2000-01-01"}' | Set-Content -NoNewline -Encoding ascii (Join-Path $data "settings.json")
    Remove-Item -Force "$installed.bak"
    Start-Process $installed | Out-Null
    Wait-Until { Test-Path "$installed.bak" } 120 "the Rust updater to swap the exe"
    Wait-Until { ((Get-Content (Join-Path $app "taskline-update.log")) -match "updated").Count -ge 2 } 30 "the second update log line"
    Wait-Until { @(Get-Process taskline -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $installed }).Count -ge 1 } 30 "the relaunch"
    $settings = Get-Content -Raw (Join-Path $data "settings.json") | ConvertFrom-Json
    if ($settings.last_update_check -ne (Get-Date -Format "yyyy-MM-dd")) { throw "the update check was not recorded" }
    Write-Host "OK: the Rust build updated itself (verified download, .bak kept, relaunched)"
}
finally {
    Get-Process taskline -ErrorAction SilentlyContinue | Where-Object { $_.Path -like "$root*" } | Stop-Process -Force
    Stop-Job $server -ErrorAction SilentlyContinue
    Remove-Job $server -Force -ErrorAction SilentlyContinue
    Remove-Item Env:TASKLINE_DATA_DIR, Env:TASKLINE_UPDATE_API -ErrorAction SilentlyContinue
}
