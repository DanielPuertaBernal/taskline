import hashlib
import json
import subprocess
import sys
import time
import urllib.error
from datetime import date
from pathlib import Path

import pytest

from taskline import updater

EXE = b"MZ fake taskline 1.2.0"
EXE_SHA = hashlib.sha256(EXE).hexdigest()
API = "https://api.example/releases/latest"


def release_json(tag="v1.2.0", assets=("taskline.exe", "SHA256SUMS.txt"), **extra):
    return json.dumps({
        "tag_name": tag,
        "assets": [{"name": n, "browser_download_url": f"https://dl.example/{tag}/{n}"} for n in assets],
        **extra,
    }).encode()


class FakeHttp:
    """Serves a fake GitHub release; records every URL requested."""

    def __init__(self, routes):
        self.routes = routes
        self.calls = []

    def __call__(self, url):
        self.calls.append(url)
        value = self.routes.get(url)
        if isinstance(value, Exception):
            raise value
        if value is None:
            raise urllib.error.HTTPError(url, 404, "Not Found", {}, None)
        return value


def fake_release(sums=None, exe=EXE, tag="v1.2.0"):
    return FakeHttp({
        API: release_json(tag),
        f"https://dl.example/{tag}/SHA256SUMS.txt": (
            sums if sums is not None else f"{EXE_SHA}  taskline.exe\n{'0' * 64}  taskline-omarchy.tar.gz\n".encode()
        ),
        f"https://dl.example/{tag}/taskline.exe": exe,
    })


@pytest.mark.parametrize(
    ("latest", "current", "newer"),
    [("v1.1.0", "1.0.0", True), ("1.0.10", "1.0.9", True), ("v1.0.0", "1.0.0", False),
     ("v0.9.9", "1.0.0", False), ("v2.0.0-rc.1", "1.0.0", False), ("garbage", "1.0.0", False)],
)
def test_is_newer_uses_semver(latest, current, newer):
    assert updater.is_newer(latest, current) is newer


def test_check_due_once_per_day_and_respects_opt_out():
    today = date(2026, 11, 26)
    assert updater.check_due({}, today)
    assert not updater.check_due({"last_update_check": "2026-11-26"}, today)
    assert updater.check_due({"last_update_check": "2026-11-25"}, today)
    assert not updater.check_due({"auto_update": False}, today)


def test_latest_release_reads_assets():
    release = updater.latest_release(fake_release(), API)
    assert release == updater.Release(
        "1.2.0", "v1.2.0", "https://dl.example/v1.2.0/taskline.exe", "https://dl.example/v1.2.0/SHA256SUMS.txt"
    )


@pytest.mark.parametrize(
    "response",
    [
        urllib.error.URLError("offline"),
        urllib.error.HTTPError(API, 403, "rate limit exceeded", {}, None),
        TimeoutError("timed out"),
        b"<html>not json",
        release_json(assets=("taskline.exe",)),  # no SHA256SUMS.txt: never install unverified
        release_json(tag="nightly"),
        release_json(prerelease=True),
    ],
)
def test_latest_release_is_silent_on_failures(response):
    assert updater.latest_release(FakeHttp({API: response}), API) is None


def test_find_update_only_when_newer(monkeypatch):
    monkeypatch.setenv("TASKLINE_UPDATE_API", API)
    assert updater.find_update("1.1.0", fake_release()).version == "1.2.0"
    assert updater.find_update("1.2.0", fake_release()) is None


def test_download_verified_writes_checked_file(tmp_path):
    http = fake_release()
    release = updater.latest_release(http, API)
    path = updater.download_verified(release, tmp_path, http)
    assert path == tmp_path / "taskline.exe.new"
    assert path.read_bytes() == EXE
    assert list(tmp_path.iterdir()) == [path]


def test_checksum_mismatch_aborts_and_leaves_nothing(tmp_path):
    http = fake_release(exe=b"tampered binary")
    release = updater.latest_release(http, API)
    with pytest.raises(updater.UpdateError, match="checksum mismatch"):
        updater.download_verified(release, tmp_path, http)
    assert list(tmp_path.iterdir()) == []


def test_missing_checksum_entry_aborts_before_downloading_exe(tmp_path):
    http = fake_release(sums=f"{'1' * 64}  taskline-omarchy.tar.gz\n".encode())
    release = updater.latest_release(http, API)
    with pytest.raises(updater.UpdateError, match="no checksum"):
        updater.download_verified(release, tmp_path, http)
    assert not any(url.endswith("taskline.exe") for url in http.calls)
    assert list(tmp_path.iterdir()) == []


def test_failed_download_aborts(tmp_path):
    http = fake_release()
    http.routes["https://dl.example/v1.2.0/taskline.exe"] = urllib.error.URLError("reset")
    release = updater.latest_release(http, API)
    with pytest.raises(updater.UpdateError):
        updater.download_verified(release, tmp_path, http)
    assert list(tmp_path.iterdir()) == []


def test_parse_sums_accepts_binary_marker():
    assert updater.parse_sums(f"{'A' * 64} *taskline.exe\nnot a line\n") == {"taskline.exe": "a" * 64}


def test_write_script_waits_backs_up_and_relaunches(tmp_path):
    script = updater.write_script(tmp_path / "taskline.exe", tmp_path / "taskline.exe.new", [111, 222], tmp_path)
    text = script.read_text()
    assert 'tasklist /FI "PID eq 111"' in text and 'tasklist /FI "PID eq 222"' in text
    assert 'move /Y "%TARGET%" "%TARGET%.bak"' in text
    assert 'move /Y "%NEW%" "%TARGET%"' in text
    assert text.count('start "" "%TARGET%"') == 2  # relaunch after success and after a rollback
    assert f'set "TARGET={tmp_path / "taskline.exe"}"' in text


def test_write_script_escapes_percent_in_paths(tmp_path):
    script = updater.write_script(tmp_path / "100%" / "taskline.exe", tmp_path / "n", [1], tmp_path)
    assert "100%%" in script.read_text()


@pytest.mark.skipif(sys.platform != "win32", reason="runs the real batch script with cmd.exe")
def test_script_replaces_exe_after_process_exits(tmp_path):
    marker = tmp_path / "relaunched.txt"
    target = tmp_path / "app.cmd"
    new = tmp_path / "app.cmd.new"
    target.write_text("@echo old>\"%~dp0relaunched.txt\"\r\n")
    new.write_text("@echo new>\"%~dp0relaunched.txt\"\r\n")
    running = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(4)"])
    script = updater.write_script(target, new, [running.pid], tmp_path)
    updater.launch_script(script)
    time.sleep(1.5)
    assert target.read_text().startswith("@echo old"), "must not replace while the process runs"
    running.wait()
    deadline = time.time() + 40
    while time.time() < deadline and not (marker.exists() and marker.read_text().strip() == "new"):
        time.sleep(0.5)
    assert target.read_text().startswith("@echo new")
    assert Path(str(target) + ".bak").read_text().startswith("@echo old")
    assert not new.exists()
    assert marker.read_text().strip() == "new"
    assert "updated" in (tmp_path / "taskline-update.log").read_text()
