"""Windows integration: data paths, settings, start-with-Windows, single instance.

Everything degrades to a no-op on other systems so the app (and its tests)
can run on Linux during development.
"""

from __future__ import annotations

import json
import os
import sys
from pathlib import Path

APP_NAME = "Taskline"
RUN_KEY = r"Software\Microsoft\Windows\CurrentVersion\Run"
IS_WINDOWS = sys.platform == "win32"


def data_dir() -> Path:
    override = os.environ.get("TASKLINE_DATA_DIR")
    if override:
        return Path(override)
    appdata = os.environ.get("APPDATA")
    if appdata:
        return Path(appdata) / "taskline"
    return Path.home() / ".local" / "share" / "taskline"


def tasks_path() -> Path:
    return data_dir() / "tasks.json"


def settings_path() -> Path:
    return data_dir() / "settings.json"


def load_settings() -> dict:
    try:
        value = json.loads(settings_path().read_text(encoding="utf-8"))
        return value if isinstance(value, dict) else {}
    except (OSError, ValueError):
        return {}


def save_settings(settings: dict) -> None:
    path = settings_path()
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_suffix(".tmp")
    tmp.write_text(json.dumps(settings, indent=1), encoding="utf-8")
    os.replace(tmp, path)


def launch_command() -> str:
    if getattr(sys, "frozen", False):
        return f'"{sys.executable}"'
    pythonw = Path(sys.executable).with_name("pythonw.exe")
    exe = pythonw if pythonw.exists() else Path(sys.executable)
    return f'"{exe}" -m taskline'


def autostart_enabled() -> bool:
    if not IS_WINDOWS:
        return False
    import winreg

    try:
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, RUN_KEY) as key:
            winreg.QueryValueEx(key, APP_NAME)
            return True
    except OSError:
        return False


def set_autostart(enabled: bool) -> None:
    if not IS_WINDOWS:
        return
    import winreg

    with winreg.OpenKey(winreg.HKEY_CURRENT_USER, RUN_KEY, 0, winreg.KEY_SET_VALUE) as key:
        if enabled:
            winreg.SetValueEx(key, APP_NAME, 0, winreg.REG_SZ, launch_command())
        else:
            try:
                winreg.DeleteValue(key, APP_NAME)
            except FileNotFoundError:
                pass


_mutex = None


def acquire_single_instance() -> bool:
    """Return False when another Taskline is already running."""
    global _mutex
    if not IS_WINDOWS:
        return True
    import ctypes

    kernel32 = ctypes.windll.kernel32
    _mutex = kernel32.CreateMutexW(None, False, "Local\\TasklineSingleInstance")
    return kernel32.GetLastError() != 183  # ERROR_ALREADY_EXISTS


def enable_dpi_awareness() -> None:
    if not IS_WINDOWS:
        return
    import ctypes

    try:
        ctypes.windll.shcore.SetProcessDpiAwareness(1)
    except (AttributeError, OSError):
        pass
