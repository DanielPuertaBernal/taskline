"""System tray icon (pystray). Callbacks run on the tray thread, so every
action is handed to the Tk thread through TasklineApp.post()."""

from __future__ import annotations

from typing import TYPE_CHECKING

import pystray

from . import system
from .icon import logo

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
            pystray.MenuItem("Theme", pystray.Menu(*[
                pystray.MenuItem(label, self._set_theme(value), checked=self._is_theme(value), radio=True)
                for value, label in (("light", "Light"), ("dark", "Dark"), ("system", "System"))
            ])),
            pystray.MenuItem(
                "Update automatically",
                lambda: app.post(lambda: (app.auto_update_var.set(not app.auto_update_var.get()),
                                          app._auto_update_changed())),
                checked=lambda _item: app.settings.get("auto_update", True) is not False,
            ),
            pystray.MenuItem("Check for updates now", lambda: app.post(lambda: app.check_for_updates(manual=True))),
            pystray.MenuItem("About Taskline", lambda: app.post(app.about)),
            pystray.Menu.SEPARATOR,
            pystray.MenuItem("Quit", lambda: app.post(app.quit)),
        )
        self.icon = pystray.Icon("taskline", logo(64), "Taskline", menu)

    def _set_theme(self, value: str):
        return lambda: self.app.post(lambda: self.app.set_theme(value))

    def _is_theme(self, value: str):
        return lambda _item: self.app.theme_var.get() == value

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
        self.icon.title = f"Taskline — {done}/{total} done today"

    def update_menu(self) -> None:
        self.icon.update_menu()
