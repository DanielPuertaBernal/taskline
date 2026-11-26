"""Self-update from GitHub Releases.

Flow: ask the GitHub API for the latest release, compare versions, download
taskline.exe next to the running exe and verify its SHA256 against the
release's SHA256SUMS.txt. Only a verified file is ever installed: a small
batch script, started detached, waits for this process to exit, renames the
current exe to taskline.exe.bak, moves the new one into place (restoring the
backup if that fails) and starts Taskline again.

Every network failure (offline, timeouts, rate limits) is reported as "no
update" so the app never bothers the user about it.
"""

from __future__ import annotations

import hashlib
import json
import os
import re
import subprocess
import sys
import tempfile
import urllib.error
import urllib.request
from collections.abc import Callable, Iterable
from dataclasses import dataclass
from datetime import date
from pathlib import Path

REPO = "DanielPuertaBernal/taskline"
API_URL = f"https://api.github.com/repos/{REPO}/releases/latest"
EXE_ASSET = "taskline.exe"
SUMS_ASSET = "SHA256SUMS.txt"
TIMEOUT = 15
MAX_DOWNLOAD = 200 * 1024 * 1024

Getter = Callable[[str], bytes]

_VERSION = re.compile(r"^v?(\d+)\.(\d+)\.(\d+)$")


class UpdateError(Exception):
    """The update could not be prepared safely; nothing was installed."""


@dataclass(frozen=True)
class Release:
    version: str
    tag: str
    exe_url: str
    sums_url: str


def parse_version(text: str) -> tuple[int, int, int] | None:
    """`v1.2.3` or `1.2.3` -> (1, 2, 3). Pre-releases and junk -> None."""
    match = _VERSION.match(text.strip())
    return tuple(int(part) for part in match.groups()) if match else None  # type: ignore[return-value]


def is_newer(latest: str, current: str) -> bool:
    new, old = parse_version(latest), parse_version(current)
    return new is not None and old is not None and new > old


def check_due(settings: dict, today: date) -> bool:
    """Auto-update is on (the default) and nothing was checked today."""
    return settings.get("auto_update", True) is not False and settings.get("last_update_check") != today.isoformat()


def http_get(url: str) -> bytes:
    request = urllib.request.Request(
        url, headers={"User-Agent": "taskline-updater", "Accept": "application/vnd.github+json, */*"}
    )
    with urllib.request.urlopen(request, timeout=TIMEOUT) as response:  # noqa: S310 (https URLs only)
        data = response.read(MAX_DOWNLOAD + 1)
    if len(data) > MAX_DOWNLOAD:
        raise UpdateError(f"{url} is larger than expected")
    return data


def latest_release(get: Getter = http_get, url: str | None = None) -> Release | None:
    """The latest published release, or None when it cannot be read."""
    try:
        payload = json.loads(get(url or os.environ.get("TASKLINE_UPDATE_API") or API_URL))
    except (OSError, urllib.error.URLError, ValueError, UpdateError):
        return None
    if not isinstance(payload, dict) or payload.get("draft") or payload.get("prerelease"):
        return None
    tag = str(payload.get("tag_name") or "")
    if parse_version(tag) is None:
        return None
    assets = {
        a.get("name"): a.get("browser_download_url")
        for a in payload.get("assets") or []
        if isinstance(a, dict)
    }
    if not assets.get(EXE_ASSET) or not assets.get(SUMS_ASSET):
        return None
    return Release(tag.lstrip("v"), tag, assets[EXE_ASSET], assets[SUMS_ASSET])


def parse_sums(text: str) -> dict[str, str]:
    """Parse `sha256sum` output: "<hex>  <name>" (binary marker `*` allowed)."""
    sums = {}
    for line in text.splitlines():
        parts = line.strip().split(None, 1)
        if len(parts) == 2 and re.fullmatch(r"[0-9a-fA-F]{64}", parts[0]):
            sums[parts[1].lstrip("*").strip()] = parts[0].lower()
    return sums


def download_verified(release: Release, directory: Path, get: Getter = http_get) -> Path:
    """Download the release exe into `directory` and verify it.

    Returns the path of the verified file (`taskline.exe.new`). Raises
    UpdateError, leaving nothing behind, when the checksum is missing or does
    not match, or when a download fails.
    """
    try:
        sums = parse_sums(get(release.sums_url).decode("utf-8", "replace"))
    except (OSError, urllib.error.URLError) as exc:
        raise UpdateError(f"could not download {SUMS_ASSET}: {exc}") from exc
    expected = sums.get(EXE_ASSET)
    if not expected:
        raise UpdateError(f"{SUMS_ASSET} has no checksum for {EXE_ASSET}")
    try:
        data = get(release.exe_url)
    except (OSError, urllib.error.URLError) as exc:
        raise UpdateError(f"could not download {EXE_ASSET}: {exc}") from exc
    actual = hashlib.sha256(data).hexdigest()
    if actual != expected:
        raise UpdateError(f"checksum mismatch for {EXE_ASSET}: expected {expected}, got {actual}")
    target = directory / (EXE_ASSET + ".new")
    try:
        fd, tmp = tempfile.mkstemp(dir=directory, prefix=".taskline.", suffix=".download")
        with os.fdopen(fd, "wb") as handle:
            handle.write(data)
        os.replace(tmp, target)
    except OSError as exc:
        raise UpdateError(f"could not write the update to {directory}: {exc}") from exc
    return target


def _bat_quote(path: Path) -> str:
    # Inside a batch file a literal % must be doubled.
    return str(path).replace("%", "%%")


def write_script(target: Path, new: Path, pids: Iterable[int], directory: Path | None = None) -> Path:
    """Write the batch file that swaps `new` into `target` once `pids` exit."""
    target, new = Path(target), Path(new)
    lines = [
        "@echo off",
        "setlocal",
        f'set "TARGET={_bat_quote(target)}"',
        f'set "NEW={_bat_quote(new)}"',
        f'set "LOG={_bat_quote(target.with_name("taskline-update.log"))}"',
    ]
    for index, pid in enumerate(int(p) for p in pids):
        # Wait up to ~60 s for each process; ping is the sleep that works
        # without a console (timeout.exe refuses redirected input).
        lines += [
            f"set /a TRIES{index}=0",
            f":wait{index}",
            f'tasklist /FI "PID eq {pid}" /NH 2>nul | find " {pid} " >nul || goto waited{index}',
            f"set /a TRIES{index}+=1",
            f"if %TRIES{index}% geq 60 goto waited{index}",
            "ping -n 2 127.0.0.1 >nul",
            f"goto wait{index}",
            f":waited{index}",
        ]
    lines += [
        "set /a MOVES=0",
        ":backup",
        'if exist "%TARGET%.bak" del /F /Q "%TARGET%.bak" >nul 2>&1',
        'move /Y "%TARGET%" "%TARGET%.bak" >nul 2>&1 && goto install',
        "set /a MOVES+=1",
        "if %MOVES% geq 20 goto fail",
        "ping -n 2 127.0.0.1 >nul",
        "goto backup",
        ":install",
        'move /Y "%NEW%" "%TARGET%" >nul 2>&1 && goto done',
        'move /Y "%TARGET%.bak" "%TARGET%" >nul 2>&1',
        ":fail",
        'echo %DATE% %TIME% update failed, kept the current version>>"%LOG%"',
        'if exist "%NEW%" del /F /Q "%NEW%" >nul 2>&1',
        'start "" "%TARGET%"',
        "goto end",
        ":done",
        'echo %DATE% %TIME% updated>>"%LOG%"',
        'start "" "%TARGET%"',
        ":end",
        # Delete this script; the trailing goto keeps cmd from reading it afterwards.
        '(goto) 2>nul & del "%~f0"',
    ]
    fd, path = tempfile.mkstemp(dir=directory, prefix="taskline-update-", suffix=".cmd")
    with os.fdopen(fd, "w", encoding="mbcs" if sys.platform == "win32" else "utf-8", newline="\r\n") as handle:
        handle.write("\n".join(lines) + "\n")
    return Path(path)


def launch_script(script: Path) -> None:
    """Start the batch file detached, with a hidden console, so it outlives us."""
    flags = 0
    if sys.platform == "win32":
        flags = subprocess.CREATE_NO_WINDOW | subprocess.CREATE_NEW_PROCESS_GROUP
    subprocess.Popen(  # noqa: S603
        ["cmd.exe", "/d", "/c", str(script)],
        creationflags=flags,
        close_fds=True,
        stdin=subprocess.DEVNULL,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        cwd=str(script.parent),
    )


def running_pids() -> list[int]:
    """This process, plus the PyInstaller onefile bootloader that holds the exe open."""
    pids = [os.getpid()]
    if getattr(sys, "_MEIPASS", None) and os.getppid() > 0:
        pids.append(os.getppid())
    return pids


def find_update(current: str, get: Getter = http_get) -> Release | None:
    release = latest_release(get)
    return release if release and is_newer(release.version, current) else None
