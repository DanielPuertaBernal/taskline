"""System tray icon (pystray). Callbacks run on the tray thread, so every
action is handed to the Tk thread through TasklineApp.post()."""

from __future__ import annotations

from typing import TYPE_CHECKING

import pystray

from . import system
from .icon import render

if TYPE_CHECKING:
    from .app import TasklineApp


class Tray:
    def __init__(self, app: TasklineApp) -> None:
        self.app = app
        self._state = (None, None)
        menu = pystray.Menu(
            pystray.MenuItem("Open tasks", lambda: app.post(app.panel.show), default=True),
            pystray.MenuItem("Show/hide bar", lambda: app.post(app.toggle_bar)),
            pystray.Menu.SEPARATOR,
            pystray.MenuItem(
                "Start with Windows",
                lambda: app.post(lambda: app.set_autostart(not system.autostart_enabled())),
                checked=lambda _item: system.autostart_enabled(),
            ),
            pystray.Menu.SEPARATOR,
            pystray.MenuItem("Quit", lambda: app.post(app.quit)),
        )
        self.icon = pystray.Icon("taskline", render(size=64), "Taskline", menu)

    def run(self) -> None:
        self.icon.run()

    def stop(self) -> None:
        try:
            self.icon.stop()
        except Exception:
            pass

    def update(self, done: int, total: int) -> None:
        if (done, total) == self._state:
            return
        self._state = (done, total)
        self.icon.icon = render(done, total, size=64)
        self.icon.title = f"Taskline — {done}/{total} done today"

    def update_menu(self) -> None:
        self.icon.update_menu()
