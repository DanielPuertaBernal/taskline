<div align="center">

<img src="assets/logo.png" alt="Taskline logo" width="128" height="128">

# Taskline

**Your day, as one line.** A tiny progress bar that shows how many of today's tasks you have finished — right in your Omarchy bar or floating on your Windows desktop.

[![Latest release](https://img.shields.io/github/v/release/DanielPuertaBernal/taskline?style=flat-square&color=7aa2f7)](https://github.com/DanielPuertaBernal/taskline/releases/latest)
[![Downloads](https://img.shields.io/github/downloads/DanielPuertaBernal/taskline/total?style=flat-square&color=9ece6a)](https://github.com/DanielPuertaBernal/taskline/releases)
[![CI](https://img.shields.io/github/actions/workflow/status/DanielPuertaBernal/taskline/ci.yml?branch=main&style=flat-square&label=CI)](https://github.com/DanielPuertaBernal/taskline/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-bb9af7?style=flat-square)](LICENSE)
![Platforms](https://img.shields.io/badge/platforms-Omarchy%20%7C%20Windows-c0caf5?style=flat-square)

[**Install on Omarchy**](#-install-on-omarchy) · [**Download for Windows**](https://github.com/DanielPuertaBernal/taskline/releases/latest/download/taskline.exe) · [Screenshots](#-screenshots) · [FAQ](#-faq)

<br>

<img src="assets/screenshots/omarchy-bar.png" alt="Taskline in the Omarchy bar: three of ten segments filled, 1/4" width="380">

</div>

---

## ✨ Why Taskline

| | |
|---|---|
| 📊 **Progress at a glance** | `▰▰▰▱▱▱▱▱▱▱ 1/4` — tasks completed today against everything still open. No app to open, no dashboard to check. |
| ⚡ **One click to manage** | Click the bar to open a panel: add, complete, edit or delete tasks and see today's progress as a donut. |
| 🌙 **Tokyo Night look** | Matches the default Omarchy theme on Linux, and brings the same style to Windows. |
| 🐧 **Built on Taskwarrior** | On Omarchy it reads and writes your real [Taskwarrior 3](https://taskwarrior.org) data, and refreshes instantly through a Taskwarrior hook. |
| 🪟 **Native-feeling on Windows** | A borderless, always-on-top bar, a tray icon and an optional *Start with Windows* switch, in a single `.exe`. |
| 🔁 **Portable data** | The Windows app stores tasks in the `task export` JSON format and can import/export Taskwarrior files. |
| 🔒 **Local only** | No accounts, no cloud, no telemetry. Your tasks never leave your machine. |

## 🐧 Install on Omarchy

**One line:**

```bash
curl -fsSL https://raw.githubusercontent.com/DanielPuertaBernal/taskline/main/install.sh | bash
```

**Or from a release:** download [`taskline-omarchy.tar.gz`](https://github.com/DanielPuertaBernal/taskline/releases/latest/download/taskline-omarchy.tar.gz), then:

```bash
tar -xzf taskline-omarchy.tar.gz && ./taskline-omarchy/install.sh
```

You need Taskwarrior 3 (`omarchy pkg add task`). Then try it:

```bash
task add Write the weekly report
task 1 done
```

<details>
<summary>What the installer does</summary>

| Step | Detail |
|---|---|
| Plugin | Copies `danso.tasks` to `~/.config/omarchy/plugins/danso.tasks` (a different existing copy is moved to `*.bak.taskline.<timestamp>`). |
| Taskwarrior hook | Installs `on-exit.omarchy-bar` into the `hooks.location` from your taskrc (`~/.taskrc` or `~/.config/task/taskrc`). Adds `hooks.location` only if it is missing. |
| Bar widget | Adds `danso.tasks` to the `right` section of `~/.config/omarchy/shell.json` (set `TASKLINE_SECTION` to change it), after backing the file up. |
| Floating panel | Adds a Hyprland rule so the task panel floats like Omarchy's floating terminal (only if `~/.config/hypr/hyprland.lua` exists and has no rule yet). |

It is idempotent — run it again any time to update — and it never touches your Taskwarrior data.

</details>

## 🪟 Install on Windows

1. Download [**taskline.exe**](https://github.com/DanielPuertaBernal/taskline/releases/latest/download/taskline.exe) from the latest release.
2. Run it. The bar appears at the top of your screen and an icon appears in the tray.
3. Right-click the bar or the tray icon → **Start with Windows** to launch it at login.

| Action | How |
|---|---|
| Open the task panel | Click the bar, or click the tray icon |
| Move the bar | Drag it (the position is remembered) |
| Menu | Right-click the bar: Start with Windows, import/export Taskwarrior JSON, reset position, quit |
| Add a task | Type in the panel and press Enter (`project:Name` groups it) |
| Complete / edit / delete | ☐ / ✎ (or double-click) / ✕ on each row |

Tasks live in `%APPDATA%\taskline\tasks.json`. Windows SmartScreen may warn about an unsigned download; choose **More info → Run anyway**.

## 📸 Screenshots

| Omarchy — bar widget | Omarchy — task panel |
|---|---|
| <img src="assets/screenshots/omarchy-bar.png" alt="Omarchy bar widget" width="380"> | <img src="assets/screenshots/omarchy-panel.png" alt="Omarchy task panel with a 25% donut" width="480"> |

| Windows — bar | Windows — task panel |
|---|---|
| <img src="assets/screenshots/windows-bar.png" alt="Windows bar" width="300"> | <img src="assets/screenshots/windows-panel.png" alt="Windows task panel" width="480"> |

<sub>Omarchy screenshots are from a real desktop, with the owner's task names blurred. Windows screenshots are captured automatically by CI on `windows-latest` from the built `taskline.exe`, using demo tasks.</sub>

## 🛠️ Build from source

**Omarchy package** (what the release ships):

```bash
bash tests/install_test.sh        # installer tests in a temporary HOME
scripts/package-omarchy.sh        # -> dist/taskline-omarchy.tar.gz
```

**Windows app** (Python 3.10+, Tkinter, pystray, Pillow, PyInstaller):

```powershell
cd windows
./build.ps1                       # -> windows/dist/taskline.exe
./selftest.ps1                    # drives the real UI and checks the result
```

Run it without building, on any OS with Tk: `cd windows && pip install pystray pillow && python -m taskline`. Tests: `cd windows && pip install pytest && python -m pytest`.

Releases are automatic: pushing a `v*` tag builds `taskline.exe` on Windows and the Omarchy tarball on Linux, and attaches both to a GitHub Release.

## 🧹 Uninstall

**Omarchy:**

```bash
curl -fsSL https://raw.githubusercontent.com/DanielPuertaBernal/taskline/main/uninstall.sh | bash
```

This removes the plugin, the hook, the bar entry and the window rule (backing up edited files). Your tasks and taskrc are kept.

**Windows:** right-click the bar → turn off **Start with Windows** → **Quit**, then delete `taskline.exe`. To also delete your tasks, remove `%APPDATA%\taskline`.

## ❓ FAQ

<details>
<summary><b>What does "1/4" count exactly?</b></summary>

Tasks completed **today** over today's completed plus every task still **pending**. Tomorrow the count starts fresh; tasks you finished yesterday no longer count.
</details>

<details>
<summary><b>Does the Windows app need Taskwarrior?</b></summary>

No. Taskwarrior is not native on Windows, so the app keeps its own file in the same format as `task export`. Use **Import/Export Taskwarrior JSON** from the bar menu to move tasks between machines.
</details>

<details>
<summary><b>The Omarchy widget does not show up.</b></summary>

The widget hides itself while there are no tasks for today. Add one with `task add …`. If it is still missing, run `omarchy-shell shell rescanPlugins` and check that `task --version` prints 3.x.
</details>

<details>
<summary><b>Is anything sent over the network?</b></summary>

No. The only network access is the installer downloading the release when you run it through `curl`.
</details>

## 🗺️ Roadmap

- [ ] Code-signed Windows build
- [ ] Translations for the Omarchy panel (it is in Spanish today)
- [ ] Optional sync with a Taskwarrior server
- [ ] macOS menu bar version

Ideas and bug reports are welcome in [Issues](https://github.com/DanielPuertaBernal/taskline/issues).

---

<div align="center">

Created by **[Daniel Puerta](https://github.com/DanielPuertaBernal)** · [danielpuertabernal.github.io](https://danielpuertabernal.github.io)

Released under the [MIT License](LICENSE) · © 2026 Daniel Puerta

</div>
