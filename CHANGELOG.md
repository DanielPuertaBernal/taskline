# Changelog

All notable changes to Taskline. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses [Semantic Versioning](https://semver.org/).

## [1.2.0] - 2026-11-26

### Added
- Month calendar date picker on both platforms: opens when the date field is clicked or focused (and from the calendar button while editing a task). Monday first, Spanish names on Omarchy and the Windows display language on Windows, today outlined, the chosen day filled, prev/next month arrows, chips (Hoy / Mañana / +1 semana / Sin fecha), keyboard navigation (arrows, PageUp/PageDown, Enter, Esc) and a dot on days that already have pending tasks. Typed shortcuts (hoy, +3d, YYYY-MM-DD) still work and move the calendar.
- Priority icons (Lucide chevrons-up / equal / chevron-down, ISC) instead of the colored marker, a priority menu with the icons for adding and editing, and themed tooltips with the priority name. Task data still stores H / M / L.
- Light theme (Tokyo Night Day) alongside dark (Tokyo Night), with a **Tema / Theme** switch in the panel and **Auto** to follow the system (Omarchy theme or Windows app theme). Also in the Windows bar and tray menus. Saved in `~/.config/taskline/omarchy.conf` and `%APPDATA%\taskline\settings.json`, never in the task data.
- Omarchy IPC: `danso.tasks-panel calendar` and `danso.tasks-panel setTheme <light|dark|system>`.
- CI checks QML syntax with qmllint.

### Changed
- Text colors in both palettes meet WCAG AA (4.5:1); a few Tokyo Night Day colors are slightly darker than the original palette for that.
- Native yellow tooltips replaced by tooltips in the panel's own style.

### Notes
- The Omarchy bar widget keeps following the Omarchy bar's colors so it matches the rest of the bar; the panel theme applies to the panel.

## [1.1.0] - 2026-11-26

### Added
- Due dates on both platforms: quick input `today`/`hoy`, `tomorrow`/`mañana`, `+3d`, `+2w`, `YYYY-MM-DD` (Omarchy also accepts anything Taskwarrior understands). Overdue dates show in red, today in orange, upcoming in cyan.
- Priorities H / M / L with a colored marker. Omarchy uses Taskwarrior's native `due:` and `priority:`; the Windows app stores the same fields in its Taskwarrior-format JSON.
- Sorting by due date (soonest first, no date last) or by priority (H > M > L > none, ties by due date). The choice is remembered.
- Automatic updates on launch, at most once a day, verified against the release's `SHA256SUMS.txt` before anything is installed. Windows replaces the exe through a detached helper that keeps `taskline.exe.bak` and relaunches; Omarchy runs the release's `install.sh`, restarts the shell and notifies. Both can be turned off.
- Windows: **About Taskline**, **Update automatically** and **Check for updates now** menu items; version metadata in the exe.
- Windows: `tasks.meta.json` records the data schema version; migrations back the file up to `tasks.json.bak-<version>-<date>` first (last five kept).

### Changed
- Windows: saves are flushed to disk before the atomic rename. A task file that cannot be read or migrated is no longer an error that stops the app: Taskline opens read-only and leaves the file untouched.
- Omarchy: `install.sh` keeps plugin and hook backups in `~/.local/state/taskline/backups` and restarts the shell when it upgrades the plugin.
- Pending tasks are sorted by due date by default instead of by urgency.

### Fixed
- Omarchy: plugin backups left inside `~/.config/omarchy/plugins` could be loaded by the shell instead of the installed plugin, and hook backups next to the hook could be run by Taskwarrior. Existing backups are moved out on install.

### Upgrading
- v1.0.0 has no updater: install v1.1.0 once by hand (see the README). Later versions install themselves.

## [1.0.0] - 2026-11-24

### Added
- Omarchy bar widget and task panel on top of Taskwarrior 3, with an on-exit hook for instant refresh, and `install.sh` / `uninstall.sh`.
- Windows app: always-on-top bar, task panel, tray icon, Start with Windows, Taskwarrior JSON import/export.
- CI and a release workflow that publishes `taskline.exe`, `taskline-omarchy.tar.gz`, `install.sh` and `SHA256SUMS.txt`.

[1.2.0]: https://github.com/DanielPuertaBernal/taskline/compare/v1.1.0...v1.2.0
[1.1.0]: https://github.com/DanielPuertaBernal/taskline/compare/v1.0.0...v1.1.0
[1.0.0]: https://github.com/DanielPuertaBernal/taskline/releases/tag/v1.0.0
