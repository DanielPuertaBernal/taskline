"""Month calendar popover for the due date fields.

Monday first, month and day names in the Windows display language, today
outlined, the chosen day filled, a dot on days that already have pending
tasks, prev/next month arrows and quick chips. The entry that opened it keeps
keyboard focus and forwards keys through handle_key(), so typed shortcuts
(today, +3d, 2026-12-01) keep working and move the calendar's cursor.
"""

from __future__ import annotations

import tkinter as tk
from collections.abc import Callable
from datetime import date, timedelta

from . import calendar_model as cal
from . import i18n, theme

CELL_W, CELL_H = 36, 30
CHIP_DAYS = (0, 1, 7, None)  # today, tomorrow, +1 week, no date


class DatePicker:
    def __init__(self, master: tk.Misc, today: Callable[[], date] = date.today,
                 texts: i18n.CalendarTexts | None = None) -> None:
        self.master = master
        self.today = today
        self.texts = texts or i18n.calendar_texts()
        self.win: tk.Toplevel | None = None
        self.on_pick: Callable[[date | None], None] | None = None
        self.due_days: dict[date, int] = {}
        self.selected: date | None = None
        self.cursor: date = today()
        self.year, self.month = self.cursor.year, self.cursor.month
        self.cells: list[tk.Canvas] = []

    # ---------------------------------------------------------- lifecycle --
    @property
    def visible(self) -> bool:
        return self.win is not None and self.win.winfo_exists() and self.win.state() != "withdrawn"

    def open(self, anchor: tk.Widget, selected: date | None, due_days: dict[date, int],
             on_pick: Callable[[date | None], None]) -> None:
        self.on_pick = on_pick
        self.due_days = due_days
        self.selected = selected
        self.cursor = selected or self.today()
        self.year, self.month = self.cursor.year, self.cursor.month
        if self.win is None or not self.win.winfo_exists():
            self._build()
        self._render()
        self.win.deiconify()
        self.win.lift()
        self._place(anchor)

    def close(self) -> None:
        if self.win is not None and self.win.winfo_exists():
            self.win.withdraw()

    def destroy(self) -> None:
        if self.win is not None and self.win.winfo_exists():
            self.win.destroy()
        self.win = None

    def contains(self, widget: tk.Misc | None) -> bool:
        return widget is not None and self.win is not None and str(widget).startswith(str(self.win))

    # ----------------------------------------------------------- building --
    def _build(self) -> None:
        win = tk.Toplevel(self.master)
        win.withdraw()
        win.overrideredirect(True)
        win.attributes("-topmost", True)
        win.configure(bg=theme.BORDER)
        body = tk.Frame(win, bg=theme.BG, padx=10, pady=8)
        body.pack(padx=1, pady=1)

        header = tk.Frame(body, bg=theme.BG)
        header.pack(fill="x")
        self.prev = self._label(header, "‹", lambda: self.page(-1), size=14)
        self.prev.pack(side="left")
        self.next = self._label(header, "›", lambda: self.page(1), size=14)
        self.next.pack(side="right")
        self.title = tk.Label(header, bg=theme.BG, fg=theme.FG, font=(theme.FONT, 11, "bold"))
        self.title.pack(side="top", pady=2)

        grid = tk.Frame(body, bg=theme.BG)
        grid.pack(pady=(4, 6))
        for col, name in enumerate(self.texts.weekdays):
            tk.Label(grid, text=name, bg=theme.BG, fg=theme.MUTED, font=(theme.FONT, 8), width=4).grid(
                row=0, column=col)
        self.cells = []
        for index in range(42):
            canvas = tk.Canvas(grid, width=CELL_W, height=CELL_H, bg=theme.BG, highlightthickness=0,
                               cursor="hand2")
            canvas.grid(row=1 + index // 7, column=index % 7)
            canvas.bind("<Button-1>", lambda _e, i=index: self._click(i))
            self.cells.append(canvas)

        chips = tk.Frame(body, bg=theme.BG)
        chips.pack(fill="x")
        for text, days in zip(self.texts.chips, CHIP_DAYS, strict=True):
            chip = tk.Label(chips, text=text, bg=theme.SURFACE, fg=theme.MUTED if days is None else theme.FG,
                            font=(theme.FONT, 9), padx=8, pady=2, cursor="hand2")
            chip.pack(side="left", padx=(0, 4))
            chip.bind("<Button-1>", lambda _e, d=days: self.choose_chip(d))
        self.win = win

    def _label(self, parent: tk.Widget, text: str, command, size: int = 11) -> tk.Label:
        label = tk.Label(parent, text=text, bg=theme.BG, fg=theme.FG, font=(theme.FONT, size), cursor="hand2",
                         padx=6)
        label.bind("<Button-1>", lambda _e: command())
        return label

    def _place(self, anchor: tk.Widget) -> None:
        win = self.win
        win.update_idletasks()
        width, height = win.winfo_reqwidth(), win.winfo_reqheight()
        x = anchor.winfo_rootx() + anchor.winfo_width() - width
        y = anchor.winfo_rooty() + anchor.winfo_height() + 4
        if y + height > anchor.winfo_screenheight():
            y = anchor.winfo_rooty() - height - 4
        win.geometry(f"+{max(0, x)}+{max(0, y)}")

    # ------------------------------------------------------------ drawing --
    def grid_days(self) -> list[date]:
        return cal.month_grid(self.year, self.month)

    def _render(self) -> None:
        if self.win is None:
            return
        self.title.configure(text=self.texts.month_title(self.year, self.month))
        today = self.today()
        for canvas, day in zip(self.cells, self.grid_days(), strict=True):
            canvas.delete("all")
            selected, cursor = day == self.selected, day == self.cursor
            if selected:
                canvas.create_rectangle(2, 1, CELL_W - 2, CELL_H - 1, fill=theme.ACCENT, outline="")
            elif cursor:
                canvas.create_rectangle(2, 1, CELL_W - 2, CELL_H - 1, fill=theme.HOVER, outline="")
            if day == today and not selected:
                canvas.create_rectangle(2, 1, CELL_W - 3, CELL_H - 2, outline=theme.ACCENT, width=2)
            color = theme.BG if selected else theme.FG if day.month == self.month else theme.MUTED
            weight = "bold" if selected or day == today else "normal"
            canvas.create_text(CELL_W / 2, CELL_H / 2 - 2, text=str(day.day), fill=color,
                               font=(theme.FONT, 10, weight))
            if self.due_days.get(day):
                dot = theme.BG if selected else theme.ORANGE
                canvas.create_oval(CELL_W / 2 - 2, CELL_H - 7, CELL_W / 2 + 2, CELL_H - 3, fill=dot, outline="")

    # ------------------------------------------------------------ actions --
    def page(self, delta: int) -> None:
        self.cursor = cal.move_month(self.cursor, delta)
        self.year, self.month = cal.shift_month(self.year, self.month, delta)
        self._render()

    def move(self, days: int) -> None:
        self.cursor += timedelta(days=days)
        self.year, self.month = self.cursor.year, self.cursor.month
        self._render()

    def follow(self, day: date | None) -> None:
        """Move the cursor to what is being typed (when it is a date)."""
        if day is None or not self.visible:
            return
        self.cursor = day
        self.year, self.month = day.year, day.month
        self._render()

    def _click(self, index: int) -> None:
        self.choose(self.grid_days()[index])

    def choose_chip(self, days: int | None) -> None:
        self.choose(None if days is None else self.today() + timedelta(days=days))

    def choose(self, day: date | None) -> None:
        self.selected = day
        self.close()
        if self.on_pick:
            self.on_pick(day)

    def handle_key(self, keysym: str, field_text: str = "", parses: bool = True) -> bool:
        """Keys forwarded by the entry. Returns True when the key was used.

        Return picks the cursor day, unless the field holds text the calendar
        cannot read (`parses` False): then the entry submits it as typed.
        """
        if not self.visible:
            return False
        moves = {"Left": -1, "Right": 1, "Up": -7, "Down": 7}
        if keysym in moves:
            self.move(moves[keysym])
        elif keysym in ("Prior", "Next"):
            self.page(-1 if keysym == "Prior" else 1)
        elif keysym == "Escape":
            self.close()
        elif keysym in ("Return", "KP_Enter"):
            if field_text.strip() and not parses:
                self.close()
                return False
            self.choose(self.cursor)
        else:
            return False
        return True
