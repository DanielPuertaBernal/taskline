"""Tokyo Night palettes: Night (dark, the default Omarchy theme) and Day (light).

`apply()` switches the module-level colors before any window is built; the
mode follows Windows' app theme unless settings.json says "theme": "light"
or "dark".
"""

import sys

# MUTED is the text-safe comment color, TRACK the decorative one (donut,
# scrollbar). Day text colors are darkened just enough for WCAG AA (4.5:1)
# on BG and SURFACE; tests/test_calendar.py checks it.
DARK = {
    "BG": "#1a1b26", "BG_DARK": "#16161e", "SURFACE": "#24283b", "HOVER": "#292e42", "BORDER": "#414868",
    "FG": "#c0caf5", "MUTED": "#8990b5", "TRACK": "#565f89", "ACCENT": "#7aa2f7", "CYAN": "#7dcfff",
    "GREEN": "#9ece6a", "YELLOW": "#e0af68", "ORANGE": "#ff9e64", "RED": "#f7768e", "PURPLE": "#bb9af7",
}
LIGHT = {
    "BG": "#e1e2e7", "BG_DARK": "#d0d5e3", "SURFACE": "#d0d5e3", "HOVER": "#c4c8da", "BORDER": "#a8aecb",
    "FG": "#3257ad", "MUTED": "#505985", "TRACK": "#a8aecb", "ACCENT": "#1357b5", "CYAN": "#006283",
    "GREEN": "#4a6230", "YELLOW": "#705631", "ORANGE": "#8d4900", "RED": "#b6093b", "PURPLE": "#7519ec",
}

FONT = "Segoe UI"
SYMBOL_FONT = "Segoe UI Symbol"

MODE = "dark"
BG = BG_DARK = SURFACE = HOVER = BORDER = FG = MUTED = TRACK = ""
ACCENT = CYAN = GREEN = YELLOW = ORANGE = RED = PURPLE = ""
DUE_COLORS: dict[str, str] = {}
PRIORITY_COLORS: dict[str, str] = {}


def contrast(a: str, b: str) -> float:
    """WCAG contrast ratio of two #rrggbb colors."""

    def luminance(color: str) -> float:
        channels = [int(color[i:i + 2], 16) / 255 for i in (1, 3, 5)]
        lin = [c / 12.92 if c <= 0.03928 else ((c + 0.055) / 1.055) ** 2.4 for c in channels]
        return 0.2126 * lin[0] + 0.7152 * lin[1] + 0.0722 * lin[2]

    high, low = sorted((luminance(a), luminance(b)), reverse=True)
    return (high + 0.05) / (low + 0.05)


def apply(mode: str) -> None:
    """Switch every color to the dark or light palette."""
    global MODE, DUE_COLORS, PRIORITY_COLORS
    MODE = "light" if mode == "light" else "dark"
    palette = LIGHT if MODE == "light" else DARK
    globals().update(palette)
    # Due states and priority markers.
    DUE_COLORS = {"overdue": palette["RED"], "today": palette["ORANGE"], "upcoming": palette["CYAN"]}
    PRIORITY_COLORS = {"H": palette["RED"], "M": palette["YELLOW"], "L": palette["GREEN"]}


def system_prefers_light() -> bool:
    if sys.platform != "win32":
        return False
    try:
        import winreg

        with winreg.OpenKey(
            winreg.HKEY_CURRENT_USER, r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize"
        ) as key:
            return winreg.QueryValueEx(key, "AppsUseLightTheme")[0] == 1
    except OSError:
        return False


def resolve_mode(settings: dict) -> str:
    choice = settings.get("theme", "system")
    if choice in ("light", "dark"):
        return choice
    return "light" if system_prefers_light() else "dark"


apply("dark")
