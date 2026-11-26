"""Taskline for Windows: an always-on-top progress bar, a task panel and a tray icon.

The bar mirrors the Omarchy widget ("▰▰▰▱▱▱▱▱▱▱  1/4"): left click opens the
panel, drag moves it, right click shows the menu. The panel adds, completes,
edits and deletes tasks with a due date and a priority, sorts them by due date
or priority, and shows today's progress as a donut. On start (at most once a
day) it looks for a newer release and installs it; see updater.py.
"""

from __future__ import annotations

import argparse
import json
import os
import queue
import sys
import tempfile
import threading
import tkinter as tk
import traceback
from datetime import date, timedelta
from pathlib import Path
from tkinter import filedialog, messagebox, ttk
from tkinter import font as tkfont

from . import __version__, i18n, priority_icons, system, theme, updater
from . import calendar_model as cal
from .datepicker import DatePicker
from .model import SORT_DUE, SORT_PRIORITY, SORTS, ReadOnlyError, Task, TaskStore, progress_bar

REFRESH_MS = 60_000
POLL_MS = 150
DRAG_THRESHOLD = 4
UPDATE_DELAY_MS = 4_000
NOTICE_MS = 6_000
SORT_LABELS = {SORT_DUE: "Sort: due date", SORT_PRIORITY: "Sort: priority"}
THEME_CHOICES = (("light", "Light"), ("dark", "Dark"), ("system", "System"))
TIP_DELAY_MS = 450


class Tooltip:
    def __init__(self, widget: tk.Widget, text_fn) -> None:
        self.widget = widget
        self.text_fn = text_fn
        self.tip: tk.Toplevel | None = None
        self._job = None
        widget.bind("<Enter>", self.schedule, add="+")
        widget.bind("<Leave>", self.hide, add="+")
        widget.bind("<ButtonPress>", self.hide, add="+")

    def schedule(self, _event=None) -> None:
        self._cancel()
        self._job = self.widget.after(TIP_DELAY_MS, self.show)

    def _cancel(self) -> None:
        if self._job is not None:
            self.widget.after_cancel(self._job)
            self._job = None

    def show(self, _event=None) -> None:
        self._job = None
        text = self.text_fn()
        if not text or self.tip or not self.widget.winfo_exists():
            return
        # Themed like the rest of the app (surface, border, small text).
        self.tip = tk.Toplevel(self.widget)
        self.tip.overrideredirect(True)
        self.tip.attributes("-topmost", True)
        self.tip.configure(bg=theme.BORDER)
        tk.Label(
            self.tip, text=text, justify="left", bg=theme.SURFACE, fg=theme.FG,
            font=(theme.FONT, 9), padx=10, pady=5,
        ).pack(padx=1, pady=1)
        x = self.widget.winfo_rootx()
        y = self.widget.winfo_rooty() + self.widget.winfo_height() + 4
        self.tip.geometry(f"+{x}+{y}")

    def hide(self, _event=None) -> None:
        self._cancel()
        if self.tip:
            self.tip.destroy()
            self.tip = None


def reduced_motion() -> bool:
    """Windows' "Show animations" setting is off (SPI_GETCLIENTAREAANIMATION)."""
    if sys.platform != "win32":
        return False
    import ctypes

    enabled = ctypes.c_bool(True)
    try:
        ctypes.windll.user32.SystemParametersInfoW(0x1042, 0, ctypes.byref(enabled), 0)
    except (AttributeError, OSError):
        return False
    return not enabled.value


class ThemeSwitch(tk.Canvas):
    """Pill switch: sun on the left (light), moon on the right (dark), sliding knob.

    Click, Space or Enter flips it; the knob slides (instantly when Windows
    animations are off) and then `on_toggle(light)` runs.
    """

    W, H, PAD = 56, 26, 3

    def __init__(self, master: tk.Misc, light: bool, on_toggle) -> None:
        super().__init__(master, width=self.W, height=self.H, bg=master.cget("bg"), highlightthickness=2,
                         highlightbackground=master.cget("bg"), highlightcolor=theme.ACCENT, takefocus=1,
                         cursor="hand2")
        self.light = light
        self.on_toggle = on_toggle
        self.knob_x = self._target()
        self.bind("<Button-1>", lambda _e: self.toggle())
        self.bind("<space>", lambda _e: self.toggle())
        self.bind("<Return>", lambda _e: self.toggle())
        self.draw()

    def _target(self) -> float:
        return self.PAD if self.light else self.W - self.H + self.PAD

    def draw(self) -> None:
        self.delete("all")
        w, h, r = self.W, self.H, self.H / 2
        track = theme.HOVER
        self.create_oval(0, 0, h, h, fill=track, outline="")
        self.create_oval(w - h, 0, w, h, fill=track, outline="")
        self.create_rectangle(r, 0, w - r, h, fill=track, outline="")
        size = h - 2 * self.PAD
        self.create_oval(self.knob_x, self.PAD, self.knob_x + size, self.PAD + size, fill=theme.ACCENT, outline="")
        self.create_text(r, r, text="☀", fill=theme.BG if self.light else theme.MUTED, font=(theme.SYMBOL_FONT, 9))
        self.create_text(w - r, r, text="☾", fill=theme.MUTED if self.light else theme.BG,
                         font=(theme.SYMBOL_FONT, 9))

    def toggle(self) -> None:
        self.light = not self.light
        self._slide(0 if reduced_motion() else 8)

    def _slide(self, steps: int) -> None:
        target = self._target()
        if steps <= 0:
            self.knob_x = target
            self.draw()
            self.on_toggle(self.light)
            return
        self.knob_x += (target - self.knob_x) / steps
        self.draw()
        self.after(16, lambda: self._slide(steps - 1))


class Panel:
    """The task manager window."""

    def __init__(self, app: TasklineApp) -> None:
        self.app = app
        self.editing: str | None = None
        self.new_priority: str | None = None
        self.texts = i18n.calendar_texts()
        self.images: dict[tuple, object] = {}  # PhotoImages must stay referenced
        self.picker_field: tk.Entry | None = None
        self.picker_edit = False
        self.win = tk.Toplevel(app.root)
        self.win.title("Taskline")
        self.win.configure(bg=theme.BG)
        self.win.geometry("760x520")
        self.win.minsize(560, 360)
        self.win.protocol("WM_DELETE_WINDOW", self.hide)
        self.win.bind("<Escape>", self._escape)
        self.win.withdraw()
        if app.icon_photo is not None:
            self.win.iconphoto(False, app.icon_photo)
        self.picker = DatePicker(self.win, texts=self.texts)

        self.font = tkfont.Font(family=theme.FONT, size=11)
        self.font_done = tkfont.Font(family=theme.FONT, size=11, overstrike=1)

        # ---- summary column
        summary = tk.Frame(self.win, bg=theme.BG, width=230)
        summary.pack(side="left", fill="y", padx=(24, 12), pady=24)
        summary.pack_propagate(False)
        tk.Label(
            summary, text="Today's tasks", bg=theme.BG, fg=theme.FG,
            font=(theme.FONT, 18, "bold"), anchor="w",
        ).pack(fill="x")
        self.pie = tk.Canvas(summary, width=180, height=180, bg=theme.BG, highlightthickness=0)
        self.pie.pack(pady=12, anchor="w")
        self.done_label = tk.Label(summary, bg=theme.BG, fg=theme.ACCENT, font=(theme.FONT, 12), anchor="w")
        self.done_label.pack(fill="x")
        self.pending_label = tk.Label(summary, bg=theme.BG, fg=theme.MUTED, font=(theme.FONT, 12), anchor="w")
        self.pending_label.pack(fill="x", pady=(4, 0))
        self.sort_button = tk.Label(
            summary, bg=theme.SURFACE, fg=theme.FG, font=(theme.FONT, 10), padx=10, pady=4, cursor="hand2",
        )
        self.sort_button.pack(anchor="w", pady=(14, 0))
        self.sort_button.bind("<Button-1>", lambda _e: self.app.toggle_sort())
        # Theme: Light/Dark switch, plus "Auto" to follow Windows' app theme.
        themes = tk.Frame(summary, bg=theme.BG)
        themes.pack(anchor="w", pady=(10, 0))
        box = tk.Frame(themes, bg=theme.SURFACE, padx=10, pady=4)
        box.pack(side="left")
        tk.Label(box, text="Theme", bg=theme.SURFACE, fg=theme.FG, font=(theme.FONT, 10)).pack(side="left")
        self.theme_switch = ThemeSwitch(box, light=theme.MODE == "light", on_toggle=self._switch_theme)
        self.theme_switch.pack(side="left", padx=(8, 0))
        auto = app.theme_var.get() == "system"
        self.auto_chip = tk.Label(
            themes, text="Auto", font=(theme.FONT, 9), padx=8, pady=2, cursor="hand2",
            bg=theme.ACCENT if auto else theme.SURFACE, fg=theme.BG if auto else theme.MUTED,
        )
        self.auto_chip.pack(side="left", padx=(8, 0))
        self.auto_chip.bind("<Button-1>", lambda _e: self.app.post(lambda: self.app.set_theme("system")))
        Tooltip(self.auto_chip, lambda: "Follow the Windows app theme")
        tk.Label(
            summary,
            text="Enter adds · Esc closes\nDue: calendar, today, +3d\npriority:H · project:Name",
            bg=theme.BG, fg=theme.MUTED, font=(theme.FONT, 9), justify="left", anchor="w", pady=4,
        ).pack(fill="x", side="bottom")

        # ---- list column
        main = tk.Frame(self.win, bg=theme.BG)
        main.pack(side="left", fill="both", expand=True, padx=(12, 24), pady=24)
        self.banner = tk.Label(
            main, bg=theme.RED, fg=theme.BG, font=(theme.FONT, 10, "bold"), justify="left", anchor="w",
            padx=10, pady=6, wraplength=440,
        )
        add_row = tk.Frame(main, bg=theme.BG)
        add_row.pack(fill="x")
        self.add_row = add_row
        self.priority_button = tk.Label(
            add_row, bg=theme.SURFACE, fg=theme.MUTED, font=(theme.FONT, 11, "bold"), width=36, height=36,
            cursor="hand2",
        )
        self.priority_button.pack(side="right", padx=(6, 0))
        self.priority_button.bind("<Button-1>", lambda _e: self._priority_menu(
            self.priority_button, self.new_priority, self._set_new_priority))
        Tooltip(self.priority_button, lambda: "Priority: " + (self.texts.priority_name(self.new_priority)
                                                             or self.texts.priorities[3]))
        self.due_entry = tk.Entry(
            add_row, bg=theme.SURFACE, fg=theme.FG, insertbackground=theme.FG, relief="flat", width=11,
            font=(theme.FONT, 12), highlightthickness=1, highlightbackground=theme.SURFACE,
            highlightcolor=theme.ACCENT,
        )
        self.due_entry.pack(side="right", padx=(6, 0), ipady=8)
        self.due_entry.bind("<Return>", self._due_return)
        self.due_entry.bind("<KP_Enter>", self._due_return)
        self.due_entry.bind("<KeyPress>", lambda e: self._picker_key(e, self.due_entry, False), add="+")
        self.due_entry.bind("<KeyRelease>", lambda _e: self._follow(self.due_entry, False), add="+")
        self.due_entry.bind("<Button-1>", lambda _e: self.win.after_idle(self._open_due_picker), add="+")
        self.entry = tk.Entry(
            add_row, bg=theme.SURFACE, fg=theme.FG, insertbackground=theme.FG, relief="flat",
            font=(theme.FONT, 12), highlightthickness=1, highlightbackground=theme.SURFACE,
            highlightcolor=theme.ACCENT,
        )
        self.entry.pack(side="left", fill="x", expand=True, ipady=8)
        self.entry.bind("<Return>", self._add)
        self.placeholder = "New task and Enter…"
        self.due_placeholder = "due"
        self._show_placeholder()
        self._show_placeholder(self.due_entry, self.due_placeholder)
        self.entry.bind("<FocusIn>", lambda _e: self._clear_placeholder())
        self.entry.bind("<FocusOut>", lambda _e: self._show_placeholder())
        self.due_entry.bind("<FocusIn>", self._due_focus_in)
        self.due_entry.bind("<FocusOut>", self._due_focus_out)
        self.error = tk.Label(main, bg=theme.BG, fg=theme.RED, font=(theme.FONT, 9), anchor="w")
        self.error.pack(fill="x")
        self._paint_priority()

        holder = tk.Frame(main, bg=theme.BG)
        holder.pack(fill="both", expand=True, pady=(4, 0))
        self.canvas = tk.Canvas(holder, bg=theme.BG, highlightthickness=0)
        style = ttk.Style(self.win)
        style.theme_use("clam")
        style.configure(
            "Taskline.Vertical.TScrollbar", background=theme.SURFACE, troughcolor=theme.BG,
            bordercolor=theme.BG, lightcolor=theme.SURFACE, darkcolor=theme.SURFACE,
            arrowcolor=theme.TRACK, gripcount=0,
        )
        style.map("Taskline.Vertical.TScrollbar", background=[("active", theme.HOVER)])
        self.scroll = ttk.Scrollbar(
            holder, orient="vertical", command=self.canvas.yview, style="Taskline.Vertical.TScrollbar"
        )
        self.canvas.configure(yscrollcommand=self.scroll.set)
        self.scroll.pack(side="right", fill="y")
        self.canvas.pack(side="left", fill="both", expand=True)
        self.rows = tk.Frame(self.canvas, bg=theme.BG)
        self.rows_id = self.canvas.create_window((0, 0), window=self.rows, anchor="nw")
        self.rows.bind("<Configure>", lambda _e: self.canvas.configure(scrollregion=self.canvas.bbox("all")))
        self.canvas.bind("<Configure>", lambda e: self.canvas.itemconfigure(self.rows_id, width=e.width))
        self.win.bind("<MouseWheel>", lambda e: self.canvas.yview_scroll(int(-e.delta / 120), "units"))

    # ---------------------------------------------------------- visibility --
    @property
    def visible(self) -> bool:
        return self.win.state() != "withdrawn"

    def show(self) -> None:
        self.refresh()
        self.win.deiconify()
        self.win.lift()
        self.win.attributes("-topmost", True)
        self.win.after(200, lambda: self.win.attributes("-topmost", False))
        self.win.focus_force()
        self.entry.focus_set()

    def hide(self) -> None:
        self.editing = None
        self.picker.close()
        self.win.withdraw()

    def toggle(self) -> None:
        self.hide() if self.visible else self.show()

    def _escape(self, _event=None) -> None:
        if self.picker.visible:
            self.picker.close()
        elif self.editing:
            self.editing = None
            self.refresh()
        else:
            self.hide()

    # --------------------------------------------------------------- input --
    def _show_placeholder(self, entry: tk.Entry | None = None, text: str | None = None) -> None:
        entry = entry or self.entry
        if not entry.get():
            entry.insert(0, text or self.placeholder)
            entry.configure(fg=theme.MUTED)

    def _clear_placeholder(self, entry: tk.Entry | None = None, text: str | None = None) -> None:
        entry = entry or self.entry
        if entry.get() == (text or self.placeholder) and entry.cget("fg") == theme.MUTED:
            entry.delete(0, "end")
            entry.configure(fg=theme.FG)

    @staticmethod
    def _value(entry: tk.Entry, placeholder: str) -> str:
        text = entry.get().strip()
        return "" if text == placeholder and entry.cget("fg") == theme.MUTED else text

    def icon(self, priority: str | None, size: int = 18):
        """PhotoImage of the priority icon in the theme's color (None for no priority)."""
        if not priority:
            return None
        key = (priority, theme.PRIORITY_COLORS[priority], size)
        if key not in self.images:
            from PIL import ImageTk

            self.images[key] = ImageTk.PhotoImage(priority_icons.render(priority, key[1], size), master=self.win)
        return self.images[key]

    def calendar_image(self, size: int = 18):
        key = ("calendar", theme.ACCENT, size)
        if key not in self.images:
            from PIL import ImageTk

            self.images[key] = ImageTk.PhotoImage(priority_icons.calendar_icon(theme.ACCENT, size), master=self.win)
        return self.images[key]

    def _set_new_priority(self, priority: str | None) -> None:
        self.new_priority = priority
        self._paint_priority()

    def _paint_priority(self) -> None:
        image = self.icon(self.new_priority, 20)
        if image is not None:
            self.priority_button.configure(image=image, text="")
        else:
            self.priority_button.configure(image="", text="–", width=3, height=1, pady=8)
        if image is not None:
            self.priority_button.configure(width=36, height=36, pady=0)

    def _menu(self) -> tk.Menu:
        return tk.Menu(self.win, tearoff=False, bg=theme.SURFACE, fg=theme.FG, activebackground=theme.ACCENT,
                       activeforeground=theme.BG, bd=0, relief="flat")

    def _priority_menu(self, anchor: tk.Widget, current: str | None, on_pick) -> None:
        """High / Medium / Low with their icons, plus No priority."""
        self.picker.close()
        menu = self._menu()
        for priority, name in zip(("H", "M", "L"), self.texts.priorities, strict=False):
            menu.add_command(label=f"  {name}", image=self.icon(priority, 16), compound="left",
                             command=lambda p=priority: on_pick(p),
                             font=(theme.FONT, 10, "bold" if current == priority else "normal"))
        menu.add_separator()
        menu.add_command(label=self.texts.priorities[3], command=lambda: on_pick(None),
                         font=(theme.FONT, 10, "bold" if not current else "normal"))
        menu.tk_popup(anchor.winfo_rootx(), anchor.winfo_rooty() + anchor.winfo_height() + 2)

    def _switch_theme(self, light: bool) -> None:
        # The switch has finished its animation; rebuilding the UI is next.
        self.app.post(lambda: self.app.set_theme("light" if light else "dark"))

    # ------------------------------------------------------------ calendar --
    def _due_focus_in(self, _event=None) -> None:
        self._clear_placeholder(self.due_entry, self.due_placeholder)
        self._open_due_picker()

    def _due_focus_out(self, _event=None) -> None:
        self._show_placeholder(self.due_entry, self.due_placeholder)
        if not self.picker_edit:
            self.win.after(150, self._maybe_close_picker)

    def _open_due_picker(self) -> None:
        if self.picker.visible and not self.picker_edit:
            return
        self._open_picker(self.due_entry, self.due_entry, False)

    def _field_date(self, field: tk.Entry, edit: bool):
        store = self.app.store
        if edit:
            return cal.due_in_text(field.get(), store.clock(), store.tz)
        return cal.quick_date(self._value(field, self.due_placeholder), store.clock(), store.tz)

    def _open_picker(self, field: tk.Entry, anchor: tk.Widget, edit: bool) -> None:
        self.picker_field, self.picker_edit = field, edit
        self.picker.open(anchor, self._field_date(field, edit), cal.due_days(self.app.store), self._picked)

    def _picked(self, day) -> None:
        field = self.picker_field
        if field is None or not field.winfo_exists():
            return
        if self.picker_edit:
            text = cal.set_due_token(field.get(), day)
        else:
            self._clear_placeholder(field, self.due_placeholder)
            text = day.isoformat() if day else ""
        field.delete(0, "end")
        field.insert(0, text)
        field.focus_set()
        field.icursor("end")

    def _picker_key(self, event, field: tk.Entry, edit: bool):
        if not self.picker.visible or self.picker_field is not field or event.keysym in ("Return", "KP_Enter"):
            return None
        return "break" if self.picker.handle_key(event.keysym) else None

    def _follow(self, field: tk.Entry, edit: bool) -> None:
        if self.picker.visible and self.picker_field is field:
            self.picker.follow(self._field_date(field, edit))

    def _due_return(self, _event=None):
        if self.picker.visible and self.picker_field is self.due_entry:
            text = self._value(self.due_entry, self.due_placeholder)
            parses = self._field_date(self.due_entry, False) is not None
            if self.picker.handle_key("Return", text, parses):
                return "break"
        self._add()
        return "break"

    def _maybe_close_picker(self) -> None:
        if not self.picker.visible:
            return
        focus = self.win.focus_get()
        if focus is self.picker_field or self.picker.contains(focus):
            return
        x, y = self.win.winfo_pointerxy()
        if self.picker.contains(self.win.winfo_containing(x, y)):
            return  # a click on the calendar is in progress
        self.picker.close()

    def show_error(self, message: str) -> None:
        self.error.configure(text=message)
        if message:
            self.win.after(5000, lambda: self.error.configure(text=""))

    def _add(self, _event=None) -> None:
        text = self._value(self.entry, self.placeholder)
        if not text:
            return
        try:
            self.app.store.add(text, due=self._value(self.due_entry, self.due_placeholder) or None,
                               priority=self.new_priority)
        except (ValueError, ReadOnlyError) as exc:
            self.show_error(str(exc))
            return
        self.show_error("")
        self.entry.delete(0, "end")
        self.due_entry.delete(0, "end")
        self._show_placeholder(self.due_entry, self.due_placeholder)
        self.new_priority = None
        self._paint_priority()
        self.entry.focus_set()
        self.app.refresh()

    # -------------------------------------------------------------- render --
    def refresh(self) -> None:
        if self.picker_edit:
            self.picker.close()  # the edit field is rebuilt below
        done, total = self.app.store.progress()
        self._draw_pie(done, total)
        self.done_label.configure(text=f"● Done: {done}")
        self.pending_label.configure(text=f"● Pending: {total - done}")
        self.sort_button.configure(text="⇅ " + SORT_LABELS[self.app.sort])
        store = self.app.store
        if store.read_only:
            self.banner.configure(text="Read-only: " + (store.load_error or "")
                                  + "\nFix or restore the file, then restart Taskline.")
            self.banner.pack(fill="x", pady=(0, 8), before=self.add_row)
        else:
            self.banner.pack_forget()
        for child in self.rows.winfo_children():
            child.destroy()
        tasks = store.today(self.app.sort)
        if not tasks:
            tk.Label(
                self.rows, text="No tasks yet. Add the first one above.", bg=theme.BG,
                fg=theme.MUTED, font=(theme.FONT, 11),
            ).pack(pady=40)
        for task in tasks:
            self._row(task)

    def _draw_pie(self, done: int, total: int) -> None:
        c = self.pie
        c.delete("all")
        ratio = done / total if total else 0
        c.create_oval(4, 4, 176, 176, fill=theme.TRACK, outline="")
        if ratio >= 1:
            c.create_oval(4, 4, 176, 176, fill=theme.ACCENT, outline="")
        elif ratio > 0:
            c.create_arc(4, 4, 176, 176, start=90, extent=-360 * ratio, fill=theme.ACCENT, outline="")
        hole = 90 * 0.58
        c.create_oval(90 - hole, 90 - hole, 90 + hole, 90 + hole, fill=theme.BG, outline="")
        c.create_text(90, 90, text=f"{round(ratio * 100)}%", fill=theme.FG, font=(theme.FONT, 26, "bold"))

    def _row(self, task: Task) -> None:
        row = tk.Frame(self.rows, bg=theme.BG)
        row.pack(fill="x", pady=2)

        def hover(on: bool) -> None:
            color = theme.HOVER if on else theme.BG
            for w in (row, *row.winfo_children()):
                if w.winfo_exists() and not isinstance(w, tk.Entry):
                    w.configure(bg=color)

        row.bind("<Enter>", lambda _e: hover(True))
        row.bind("<Leave>", lambda _e: hover(False))

        check = self._icon_button(
            row, "☑" if task.is_done else "☐", theme.ACCENT if task.is_done else theme.FG,
            lambda: self._toggle(task.uuid),
        )
        check.pack(side="left", padx=(8, 6))
        image = self.icon(task.priority)
        # Always an image: without one, Tk reads width/height as characters.
        marker = tk.Label(row, bg=theme.BG, image=image or self.blank_image(), width=20, height=20)
        if image is not None:
            Tooltip(marker, lambda: "Priority: " + self.texts.priority_name(task.priority))
        marker.pack(side="left", padx=(0, 6))

        delete = self._icon_button(row, "✕", theme.RED, lambda: self._delete(task.uuid))
        delete.pack(side="right", padx=(8, 8))
        edit = self._icon_button(row, "✎", theme.FG, lambda: self._start_edit(task.uuid))
        edit.pack(side="right", padx=4)
        if self.editing == task.uuid:
            # Calendar and priority for the task being edited.
            cal_button = tk.Label(row, image=self.calendar_image(), bg=theme.BG, cursor="hand2")
            cal_button.pack(side="right", padx=4)
            Tooltip(cal_button, lambda: "Due date")
            prio_button = tk.Label(row, bg=theme.BG, cursor="hand2", image=self.blank_image(), width=20, height=20)
            prio_button.pack(side="right", padx=4)
            Tooltip(prio_button, lambda: "Priority")
        due_text = self.app.store.due_label(task)
        if due_text and self.editing != task.uuid:
            state = self.app.store.due_state(task)
            tk.Label(
                row, text=due_text, bg=theme.BG, font=(theme.FONT, 10),
                fg=theme.DUE_COLORS.get(state, theme.MUTED),
            ).pack(side="right", padx=(8, 6))

        if self.editing == task.uuid:
            field = tk.Entry(
                row, bg=theme.SURFACE, fg=theme.FG, insertbackground=theme.FG, relief="flat",
                font=self.font,
            )
            field.insert(0, self.app.store.edit_text(task))
            field.pack(side="left", fill="x", expand=True, ipady=4)

            def edit_return(_e=None):
                if self.picker.visible and self.picker_field is field and self.picker.handle_key("Return"):
                    return "break"
                self._save_edit(task.uuid, field.get())
                return "break"

            def paint_priority(_e=None) -> None:
                current = cal.priority_in_text(field.get())
                prio_button.configure(image=self.icon(current) or self.icon_placeholder())

            def set_priority(p) -> None:
                text = cal.set_priority_token(field.get(), p)
                field.delete(0, "end")
                field.insert(0, text)
                field.focus_set()
                field.icursor("end")
                paint_priority()

            field.bind("<Return>", edit_return)
            field.bind("<KeyPress>", lambda e: self._picker_key(e, field, True), add="+")
            field.bind("<KeyRelease>", lambda _e: (self._follow(field, True), paint_priority()), add="+")
            cal_button.bind("<Button-1>", lambda _e: self.picker.close() if self.picker.visible
                            else self._open_picker(field, cal_button, True))
            prio_button.bind("<Button-1>", lambda _e: self._priority_menu(
                prio_button, cal.priority_in_text(field.get()), set_priority))
            paint_priority()
            field.after(10, lambda: (field.focus_set(), field.select_range(0, "end")))
        else:
            label = tk.Label(
                row, text=task.description + (f"   · {task.project}" if task.project else ""),
                bg=theme.BG, fg=theme.MUTED if task.is_done else theme.FG,
                font=self.font_done if task.is_done else self.font, anchor="w",
            )
            label.pack(side="left", fill="x", expand=True, ipady=6)
            label.bind("<Double-Button-1>", lambda _e: self._start_edit(task.uuid))

    def blank_image(self, size: int = 20):
        key = ("blank", size)
        if key not in self.images:
            self.images[key] = tk.PhotoImage(master=self.win, width=size, height=size)
        return self.images[key]

    def icon_placeholder(self):
        """The Medium icon in the muted color: "no priority yet" in the edit row."""
        key = ("placeholder", theme.MUTED)
        if key not in self.images:
            from PIL import ImageTk

            self.images[key] = ImageTk.PhotoImage(priority_icons.render("M", theme.MUTED, 18), master=self.win)
        return self.images[key]

    def _icon_button(self, parent: tk.Widget, text: str, color: str, command) -> tk.Label:
        label = tk.Label(parent, text=text, bg=theme.BG, fg=color, font=(theme.SYMBOL_FONT, 13), cursor="hand2")
        label.bind("<Button-1>", lambda _e: command())
        return label

    # ------------------------------------------------------------- actions --
    def _guarded(self, action) -> bool:
        try:
            action()
        except (ValueError, ReadOnlyError) as exc:
            self.show_error(str(exc))
            return False
        return True

    def _toggle(self, task_uuid: str) -> None:
        self._guarded(lambda: self.app.store.toggle(task_uuid))
        self.app.refresh()

    def _delete(self, task_uuid: str) -> None:
        self._guarded(lambda: self.app.store.delete(task_uuid))
        if self.editing == task_uuid:
            self.editing = None
        self.app.refresh()

    def _start_edit(self, task_uuid: str) -> None:
        self.editing = None if self.editing == task_uuid else task_uuid
        self.refresh()

    def _save_edit(self, task_uuid: str, text: str) -> None:
        if text.strip() and not self._guarded(lambda: self.app.store.edit(task_uuid, text)):
            return  # keep the field open so the typo can be fixed
        self.editing = None
        self.app.refresh()


class TasklineApp:
    def __init__(self, store: TaskStore, with_tray: bool = True) -> None:
        self.store = store
        self.settings = system.load_settings()
        self.segments = int(self.settings.get("segments", 10))
        sort = self.settings.get("sort")
        self.sort = sort if sort in SORTS else SORT_DUE
        self.notice_until = 0
        self.events: queue.Queue = queue.Queue()
        self.tray = None

        self.root = tk.Tk()
        self.root.withdraw()
        self.root.title("Taskline")
        self.icon_photo = self._make_icon_photo()
        self.theme_var = tk.StringVar(value=self.settings.get("theme", "system"))
        if self.theme_var.get() not in dict(THEME_CHOICES):
            self.theme_var.set("system")
        self.autostart_var = tk.BooleanVar(value=system.autostart_enabled())
        self.auto_update_var = tk.BooleanVar(value=self.settings.get("auto_update", True) is not False)
        self._drag_origin: tuple[int, int, int, int] | None = None
        self._dragged = False

        self._build_ui()
        self.refresh()
        self._place_bar()
        if with_tray:
            self._start_tray()
            self._show_update_notice()
            self.root.after(UPDATE_DELAY_MS, self.check_for_updates)
        if store.read_only:
            self.root.after(500, self._warn_read_only)
        self.root.after(POLL_MS, self._poll)
        self.root.after(REFRESH_MS, self._tick)

    def _build_ui(self) -> None:
        """Bar, menu and panel, in the current theme (rebuilt when it changes)."""
        # ---- the bar: a borderless, always-on-top strip
        self.bar = tk.Toplevel(self.root)
        self.bar.overrideredirect(True)
        self.bar.attributes("-topmost", True)
        self.bar.configure(bg=theme.BG)
        self.bar_label = tk.Label(
            self.bar, bg=theme.BG, fg=theme.FG, font=(theme.SYMBOL_FONT, 11), padx=12, pady=4, cursor="hand2",
        )
        self.bar_label.pack()
        Tooltip(self.bar_label, self._tooltip_text)
        self.bar_label.bind("<ButtonPress-1>", self._press)
        self.bar_label.bind("<B1-Motion>", self._drag)
        self.bar_label.bind("<ButtonRelease-1>", self._release)
        self.bar_label.bind("<Button-3>", self._context_menu)

        self.menu = tk.Menu(self.root, tearoff=False, bg=theme.SURFACE, fg=theme.FG,
                            activebackground=theme.ACCENT, activeforeground=theme.BG)
        self.menu.add_command(label="Open tasks", command=self.toggle_panel)
        self.menu.add_separator()
        self.menu.add_checkbutton(label="Start with Windows", variable=self.autostart_var,
                                  command=self._autostart_changed)
        self.menu.add_command(label="Import Taskwarrior JSON…", command=self.import_json)
        self.menu.add_command(label="Export Taskwarrior JSON…", command=self.export_json)
        self.menu.add_command(label="Reset bar position", command=self.reset_position)
        theme_menu = tk.Menu(self.menu, tearoff=False, bg=theme.SURFACE, fg=theme.FG,
                             activebackground=theme.ACCENT, activeforeground=theme.BG)
        for value, label in THEME_CHOICES:
            theme_menu.add_radiobutton(label=label, value=value, variable=self.theme_var,
                                       command=lambda v=value: self.set_theme(v))
        self.menu.add_cascade(label="Theme", menu=theme_menu)
        self.menu.add_separator()
        self.menu.add_checkbutton(label="Update automatically", variable=self.auto_update_var,
                                  command=self._auto_update_changed)
        self.menu.add_command(label="Check for updates now", command=lambda: self.check_for_updates(manual=True))
        self.menu.add_command(label="About Taskline", command=self.about)
        self.menu.add_separator()
        self.menu.add_command(label="Quit", command=self.quit)

        self.panel = Panel(self)

    def set_theme(self, choice: str) -> None:
        """Light, Dark or System (follows Windows' app theme); saved in settings.json."""
        choice = choice if choice in dict(THEME_CHOICES) else "system"
        self.theme_var.set(choice)
        self.settings["theme"] = choice
        system.save_settings(self.settings)
        theme.apply(theme.resolve_mode(self.settings))
        visible = self.panel.visible
        geometry = self.panel.win.geometry()
        self.panel.picker.destroy()
        self.panel.win.destroy()
        self.menu.destroy()
        self.bar.destroy()
        self._build_ui()
        self.refresh()
        self._place_bar()
        if visible:
            self.panel.win.geometry(geometry)
            self.panel.show()
        if self.tray:
            self.tray.update_menu()

    # -------------------------------------------------------------- helpers --
    def _make_icon_photo(self):
        try:
            from PIL import ImageTk

            from .icon import logo

            photo = ImageTk.PhotoImage(logo(64))
            self.root.iconphoto(True, photo)
            return photo
        except Exception:  # icon is cosmetic; never fail startup over it
            return None

    def _tooltip_text(self) -> str:
        pending = [t for t in self.store.today(self.sort) if not t.is_done][:5]
        if not pending:
            return "All done for today"
        lines = []
        for t in pending:
            due = self.store.due_label(t)
            lines.append(f"• {t.description}" + (f"  ({due})" if due else ""))
        return "Pending:\n" + "\n".join(lines)

    def _warn_read_only(self) -> None:
        messagebox.showwarning(
            "Taskline",
            f"{self.store.load_error}\n\nTaskline opened read-only so nothing is overwritten. "
            "Restore the file (backups are next to it as tasks.json.bak-*) and restart Taskline.",
        )

    def _place_bar(self) -> None:
        self.bar.update_idletasks()
        x, y = self.settings.get("x"), self.settings.get("y")
        if not isinstance(x, int) or not isinstance(y, int):
            x = (self.bar.winfo_screenwidth() - self.bar.winfo_reqwidth()) // 2
            y = 6
        self.bar.geometry(f"+{x}+{y}")

    def reset_position(self) -> None:
        self.settings.pop("x", None)
        self.settings.pop("y", None)
        system.save_settings(self.settings)
        self._place_bar()

    # ----------------------------------------------------------- bar input --
    def _press(self, event) -> None:
        self._drag_origin = (event.x_root, event.y_root, self.bar.winfo_x(), self.bar.winfo_y())
        self._dragged = False

    def _drag(self, event) -> None:
        if not self._drag_origin:
            return
        sx, sy, wx, wy = self._drag_origin
        dx, dy = event.x_root - sx, event.y_root - sy
        if abs(dx) > DRAG_THRESHOLD or abs(dy) > DRAG_THRESHOLD:
            self._dragged = True
        if self._dragged:
            self.bar.geometry(f"+{wx + dx}+{wy + dy}")

    def _release(self, _event) -> None:
        if self._dragged:
            self.settings["x"], self.settings["y"] = self.bar.winfo_x(), self.bar.winfo_y()
            system.save_settings(self.settings)
        else:
            self.toggle_panel()
        self._drag_origin = None
        self._dragged = False

    def _context_menu(self, event) -> None:
        self.autostart_var.set(system.autostart_enabled())
        self.menu.tk_popup(event.x_root, event.y_root)

    # ------------------------------------------------------------- actions --
    def toggle_panel(self) -> None:
        self.panel.toggle()

    def toggle_sort(self) -> None:
        self.sort = SORT_PRIORITY if self.sort == SORT_DUE else SORT_DUE
        self.settings["sort"] = self.sort
        system.save_settings(self.settings)
        self.refresh()

    def about(self) -> None:
        messagebox.showinfo(
            "About Taskline",
            f"Taskline {__version__}\n\nYour day, as one line.\n"
            f"https://github.com/{updater.REPO}\n\nTasks: {self.store.path}",
        )

    # ------------------------------------------------------------- updates --
    def _auto_update_changed(self) -> None:
        self.settings["auto_update"] = bool(self.auto_update_var.get())
        system.save_settings(self.settings)

    def check_for_updates(self, manual: bool = False) -> None:
        """Look for a newer release on a worker thread (once a day unless `manual`)."""
        if not manual and not updater.check_due(self.settings, date.today()):
            return
        if not getattr(sys, "frozen", False) and not os.environ.get("TASKLINE_UPDATE_API"):
            if manual:
                messagebox.showinfo("Taskline", "Updates are installed by the packaged taskline.exe only.")
            return
        self.settings["last_update_check"] = date.today().isoformat()
        system.save_settings(self.settings)
        target = Path(sys.executable)

        def work() -> None:
            try:
                release = updater.find_update(__version__)
                if release is None:
                    if manual:
                        self.post(lambda: messagebox.showinfo("Taskline", f"Taskline {__version__} is up to date."))
                    return
                new = updater.download_verified(release, target.parent)
            except updater.UpdateError as exc:
                if manual:
                    message = f"Update not installed:\n{exc}"
                    self.post(lambda: messagebox.showerror("Taskline", message))
                return
            except Exception:  # an update must never crash the app
                return
            self.post(lambda: self._install_update(release, target, new))

        threading.Thread(target=work, daemon=True).start()

    def _install_update(self, release: updater.Release, target: Path, new: Path) -> None:
        try:
            script = updater.write_script(target, new, updater.running_pids())
        except OSError:
            new.unlink(missing_ok=True)
            return
        self.settings["update_notice"] = release.version
        system.save_settings(self.settings)
        updater.launch_script(script)
        self.quit()

    def _show_update_notice(self) -> None:
        version = self.settings.pop("update_notice", None)
        if version is None:
            return
        system.save_settings(self.settings)
        if version != __version__:
            return  # the update was rolled back
        message = f"Updated to v{__version__}"
        self.notice_until = 1
        self.bar_label.configure(text=f"✓ {message}", fg=theme.GREEN)
        if self.tray:
            try:
                self.tray.icon.notify(message, "Taskline")
            except Exception:
                pass

        def clear() -> None:
            self.notice_until = 0
            self.refresh()

        self.root.after(NOTICE_MS, clear)

    def _autostart_changed(self) -> None:
        self.set_autostart(self.autostart_var.get())

    def set_autostart(self, enabled: bool) -> None:
        try:
            system.set_autostart(enabled)
        except OSError as exc:
            messagebox.showerror("Taskline", f"Could not change the startup setting:\n{exc}")
        self.autostart_var.set(system.autostart_enabled())
        if self.tray:
            self.tray.update_menu()

    def import_json(self) -> None:
        path = filedialog.askopenfilename(
            title="Import Taskwarrior export", filetypes=[("JSON", "*.json"), ("All files", "*.*")]
        )
        if not path:
            return
        try:
            count = self.store.import_file(path)
        except (OSError, ValueError) as exc:
            messagebox.showerror("Taskline", f"Could not import {path}:\n{exc}")
            return
        self.refresh()
        messagebox.showinfo("Taskline", f"Imported {count} task(s).")

    def export_json(self) -> None:
        path = filedialog.asksaveasfilename(
            title="Export as Taskwarrior JSON", defaultextension=".json",
            initialfile="taskline-export.json", filetypes=[("JSON", "*.json")],
        )
        if path:
            self.store.export_file(path)

    def refresh(self) -> None:
        done, total = self.store.progress()
        if self.tray:
            self.tray.update(done, total)
        if self.panel.visible:
            self.panel.refresh()
        if self.notice_until:
            return
        self.bar_label.configure(
            text=progress_bar(done, total, self.segments),
            fg=theme.ACCENT if total > 0 and done == total else theme.FG,
        )

    def _tick(self) -> None:
        # Covers the day rollover and edits made by another process.
        self.store.reload_if_changed()
        self.refresh()
        self.root.after(REFRESH_MS, self._tick)

    def _poll(self) -> None:
        try:
            while True:
                action = self.events.get_nowait()
                action()
        except queue.Empty:
            pass
        self.root.after(POLL_MS, self._poll)

    def post(self, action) -> None:
        """Run `action` on the Tk thread (used by the tray thread)."""
        self.events.put(action)

    def toggle_bar(self) -> None:
        if self.bar.state() == "withdrawn":
            self.bar.deiconify()
            self.bar.attributes("-topmost", True)
        else:
            self.bar.withdraw()

    def _start_tray(self) -> None:
        try:
            from .tray import Tray
        except Exception:
            return
        self.tray = Tray(self)
        threading.Thread(target=self.tray.run, daemon=True).start()
        done, total = self.store.progress()
        self.tray.update(done, total)

    def quit(self) -> None:
        if self.tray:
            self.tray.stop()
        self.root.destroy()

    def run(self) -> None:
        self.root.mainloop()


# ------------------------------------------------------------------ entry --
def _demo_store(directory: Path) -> TaskStore:
    store = TaskStore(directory / "tasks.json", app_version=__version__)
    first = store.add("Review pull requests project:Work", priority="H")
    store.add("Send the invoice project:Work", due="2020-01-01", priority="H")  # overdue
    store.add("Write the weekly report project:Work", due="today", priority="M")
    store.add("Buy groceries project:Home", due="tomorrow", priority="L")
    store.add("30 minutes of reading", due="+5d")
    store.complete(first.uuid)
    return store


def _selftest_updater(directory: Path) -> list[str]:
    """Serve a fake release over local HTTP and run the bundled updater against it."""
    import hashlib
    import http.server

    payload = b"MZ selftest build"
    files = {
        "/api": json.dumps({"tag_name": "v99.0.0", "assets": [
            {"name": n, "browser_download_url": f"URL/{n}"} for n in ("taskline.exe", "SHA256SUMS.txt")
        ]}).encode(),
        "/taskline.exe": payload,
        "/SHA256SUMS.txt": f"{hashlib.sha256(payload).hexdigest()}  taskline.exe\n".encode(),
    }

    class Handler(http.server.BaseHTTPRequestHandler):
        def do_GET(self):  # noqa: N802
            body = files.get(self.path)
            self.send_response(200 if body else 404)
            self.end_headers()
            self.wfile.write(body or b"")

        def log_message(self, *_args):
            pass

    server = http.server.HTTPServer(("127.0.0.1", 0), Handler)
    base = f"http://127.0.0.1:{server.server_port}"
    files["/api"] = files["/api"].replace(b"URL", base.encode())
    threading.Thread(target=server.serve_forever, daemon=True).start()
    try:
        release = updater.latest_release(updater.http_get, base + "/api")
        assert release and updater.is_newer(release.version, __version__), release
        new = updater.download_verified(release, directory)
        assert new.read_bytes() == payload
        files["/taskline.exe"] = b"tampered"
        try:
            updater.download_verified(release, directory)
            raise AssertionError("a tampered download was accepted")
        except updater.UpdateError:
            pass
        script = updater.write_script(directory / "taskline.exe", new, updater.running_pids(), directory)
        assert "move /Y" in script.read_text()
    finally:
        server.shutdown()
    return ["updater: fake v99.0.0 found, verified, tampered copy rejected"]


def selftest(out: Path, screenshots: Path | None) -> int:
    """Exercise the real UI without user input; used by CI against the built .exe."""
    lines = [f"taskline {__version__}"]
    try:
        with tempfile.TemporaryDirectory() as tmp:
            import os

            os.environ["TASKLINE_DATA_DIR"] = tmp
            theme.apply("dark")
            store = _demo_store(Path(tmp))
            app = TasklineApp(store, with_tray=False)
            from .tray import Tray  # noqa: F401  # the tray backend must be bundled

            app.root.update()
            lines.append(f"bar: {app.bar_label.cget('text')}")
            assert store.progress() == (1, 5), store.progress()
            app.panel.show()
            app.root.update()
            app.panel._clear_placeholder()
            app.panel.entry.insert(0, "Added by selftest")
            app.panel._clear_placeholder(app.panel.due_entry, app.panel.due_placeholder)
            app.panel.due_entry.insert(0, "+2d")
            app.panel._set_new_priority("H")
            assert app.panel.priority_button.cget("image"), "the priority button shows the icon"
            app.panel._add()
            app.root.update()
            assert store.progress() == (1, 6), store.progress()
            lines.append(f"after add: {app.bar_label.cget('text')}")
            saved = json.loads((Path(tmp) / "tasks.json").read_text(encoding="utf-8"))
            assert len(saved) == 6
            added = next(t for t in saved if t["description"] == "Added by selftest")
            assert added["priority"] == "H" and added["due"], added
            order = [t.description for t in store.today(app.sort) if not t.is_done]
            assert order[0] == "Send the invoice", order
            app.toggle_sort()
            assert app.sort == "priority" and system.load_settings()["sort"] == "priority"
            order = [t.description for t in store.today(app.sort) if not t.is_done]
            assert order[:2] == ["Send the invoice", "Added by selftest"], order
            app.toggle_sort()
            lines.append("due/priority/sort: OK")
            lines += _selftest_calendar(app, store)
            lines += _selftest_theme(app)
            lines += _selftest_updater(Path(tmp))
            if screenshots:
                _screenshots(app, screenshots)
                lines.append(f"screenshots: {screenshots}")
            app.quit()
        lines.append("OK")
        code = 0
    except Exception:
        lines.append(traceback.format_exc())
        lines.append("FAILED")
        code = 1
    out.write_text("\n".join(lines) + "\n", encoding="utf-8")
    return code


class _Key:
    def __init__(self, keysym: str) -> None:
        self.keysym = keysym


def _selftest_calendar(app: TasklineApp, store: TaskStore) -> list[str]:
    """Open the date picker from the due field, drive it with keys, add a task."""
    panel = app.panel
    panel._clear_placeholder()
    panel.entry.delete(0, "end")
    panel.entry.insert(0, "Picked in the calendar")
    panel._open_picker(panel.due_entry, panel.due_entry, False)
    app.root.update()
    assert panel.picker.visible, "calendar did not open"
    today = date.today()
    assert panel.picker.cursor == today
    assert panel._picker_key(_Key("Right"), panel.due_entry, False) == "break"
    assert panel._picker_key(_Key("Down"), panel.due_entry, False) == "break"
    assert panel.picker.cursor == today + timedelta(days=8)
    assert panel._due_return() == "break" and not panel.picker.visible
    assert panel.due_entry.get() == (today + timedelta(days=8)).isoformat()
    panel._set_new_priority("M")
    panel._add()
    app.root.update()
    added = next(t for t in store.all() if t.description == "Picked in the calendar")
    assert store._local_date(added.due) == today + timedelta(days=8) and added.priority == "M", added.data
    # Typed shortcuts still work and move the cursor.
    panel._clear_placeholder(panel.due_entry, panel.due_placeholder)
    panel._open_picker(panel.due_entry, panel.due_entry, False)
    panel.due_entry.insert(0, "+3d")
    panel._follow(panel.due_entry, False)
    assert panel.picker.cursor == today + timedelta(days=3)
    panel.picker.close()
    panel.due_entry.delete(0, "end")
    store.delete(added.uuid)
    app.refresh()
    return ["calendar: keyboard pick, typed shortcut and priority icon OK"]


def _selftest_theme(app: TasklineApp) -> list[str]:
    """Flip the switch to light and back: the whole UI is rebuilt in the new palette."""
    app.set_theme("light")
    app.root.update()
    assert theme.MODE == "light" and app.panel.win.cget("bg") == theme.LIGHT["BG"]
    assert system.load_settings()["theme"] == "light"
    app.panel.theme_switch.light = False
    app.panel.theme_switch._slide(0)  # the switch's own toggle path, without the animation
    for _ in range(5):
        app.root.update()
        app.root.after(20)
    app.root.update()
    assert theme.MODE == "dark" and system.load_settings()["theme"] == "dark", theme.MODE
    return ["theme: light/dark switch rebuilds the UI and is saved"]


def _grab(widget_windows, path: Path) -> None:
    from PIL import ImageGrab

    boxes = [(w.winfo_rootx(), w.winfo_rooty(), w.winfo_rootx() + w.winfo_width(), w.winfo_rooty() + w.winfo_height())
             for w in widget_windows]
    ImageGrab.grab((min(b[0] for b in boxes), min(b[1] for b in boxes), max(b[2] for b in boxes),
                    max(b[3] for b in boxes))).save(path)


def _settle(app: TasklineApp) -> None:
    for _ in range(10):
        app.root.update()
        app.root.after(50)


def _panel_with_calendar(app: TasklineApp, directory: Path, name: str) -> None:
    app.panel.win.geometry("760x520+40+120")
    app.panel.show()
    _settle(app)
    panel = app.panel
    panel._open_picker(panel.due_entry, panel.due_entry, False)
    _settle(app)
    _grab([panel.win, panel.picker.win], directory / name)
    panel.picker.close()


def _screenshots(app: TasklineApp, directory: Path) -> None:
    from PIL import ImageGrab

    directory.mkdir(parents=True, exist_ok=True)
    # Remove the task added by the selftest so the screenshot shows the demo.
    for task in app.store.today():
        if task.description == "Added by selftest":
            app.store.delete(task.uuid)
    app.refresh()
    app.bar.geometry("+40+40")
    app.panel.win.geometry("760x520+40+120")
    app.panel.refresh()
    for _ in range(10):
        app.root.update()
        app.root.after(50)
    bar = app.bar
    ImageGrab.grab((bar.winfo_rootx(), bar.winfo_rooty(), bar.winfo_rootx() + bar.winfo_width(),
                    bar.winfo_rooty() + bar.winfo_height())).save(directory / "windows-bar.png")
    win = app.panel.win
    ImageGrab.grab((win.winfo_rootx(), win.winfo_rooty(), win.winfo_rootx() + win.winfo_width(),
                    win.winfo_rooty() + win.winfo_height())).save(directory / "windows-panel.png")
    _panel_with_calendar(app, directory, "windows-panel-dark.png")
    app.set_theme("light")
    _panel_with_calendar(app, directory, "windows-panel-light.png")
    app.set_theme("dark")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(prog="taskline", description="Daily task progress bar")
    parser.add_argument("--version", action="version", version=f"taskline {__version__}")
    parser.add_argument("--selftest", metavar="REPORT", type=Path,
                        help="run a scripted UI check and write the result to REPORT")
    parser.add_argument("--screenshots", metavar="DIR", type=Path,
                        help="with --selftest, save screenshots of the bar and panel")
    args = parser.parse_args(argv)

    system.enable_dpi_awareness()
    theme.apply(theme.resolve_mode(system.load_settings()))
    if args.selftest:
        return selftest(args.selftest, args.screenshots)
    if not system.acquire_single_instance():
        return 0
    store = TaskStore(system.tasks_path(), app_version=__version__)
    TasklineApp(store).run()
    return 0


if __name__ == "__main__":
    sys.exit(main())
