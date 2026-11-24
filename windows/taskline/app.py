"""Taskline for Windows: an always-on-top progress bar, a task panel and a tray icon.

The bar mirrors the Omarchy widget ("▰▰▰▱▱▱▱▱▱▱  1/4"): left click opens the
panel, drag moves it, right click shows the menu. The panel adds, completes,
edits and deletes tasks and shows today's progress as a donut.
"""

from __future__ import annotations

import argparse
import json
import queue
import sys
import tempfile
import threading
import tkinter as tk
import traceback
from pathlib import Path
from tkinter import filedialog, messagebox, ttk
from tkinter import font as tkfont

from . import __version__, system, theme
from .model import Task, TaskStore, progress_bar

REFRESH_MS = 60_000
POLL_MS = 150
DRAG_THRESHOLD = 4


class Tooltip:
    def __init__(self, widget: tk.Widget, text_fn) -> None:
        self.widget = widget
        self.text_fn = text_fn
        self.tip: tk.Toplevel | None = None
        widget.bind("<Enter>", self.show, add="+")
        widget.bind("<Leave>", self.hide, add="+")
        widget.bind("<ButtonPress>", self.hide, add="+")

    def show(self, _event=None) -> None:
        text = self.text_fn()
        if not text or self.tip:
            return
        self.tip = tk.Toplevel(self.widget)
        self.tip.overrideredirect(True)
        self.tip.attributes("-topmost", True)
        tk.Label(
            self.tip, text=text, justify="left", bg=theme.SURFACE, fg=theme.FG,
            font=(theme.FONT, 9), padx=10, pady=6,
        ).pack()
        x = self.widget.winfo_rootx()
        y = self.widget.winfo_rooty() + self.widget.winfo_height() + 4
        self.tip.geometry(f"+{x}+{y}")

    def hide(self, _event=None) -> None:
        if self.tip:
            self.tip.destroy()
            self.tip = None


class Panel:
    """The task manager window."""

    def __init__(self, app: TasklineApp) -> None:
        self.app = app
        self.editing: str | None = None
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
        self.pie = tk.Canvas(summary, width=200, height=200, bg=theme.BG, highlightthickness=0)
        self.pie.pack(pady=18, anchor="w")
        self.done_label = tk.Label(summary, bg=theme.BG, fg=theme.ACCENT, font=(theme.FONT, 12), anchor="w")
        self.done_label.pack(fill="x")
        self.pending_label = tk.Label(summary, bg=theme.BG, fg=theme.MUTED, font=(theme.FONT, 12), anchor="w")
        self.pending_label.pack(fill="x", pady=(4, 0))
        tk.Label(
            summary, text="Enter adds · Esc closes\nproject:Name groups a task",
            bg=theme.BG, fg=theme.MUTED, font=(theme.FONT, 9), justify="left", anchor="w",
        ).pack(fill="x", side="bottom")

        # ---- list column
        main = tk.Frame(self.win, bg=theme.BG)
        main.pack(side="left", fill="both", expand=True, padx=(12, 24), pady=24)
        self.entry = tk.Entry(
            main, bg=theme.SURFACE, fg=theme.FG, insertbackground=theme.FG, relief="flat",
            font=(theme.FONT, 12), highlightthickness=1, highlightbackground=theme.SURFACE,
            highlightcolor=theme.ACCENT,
        )
        self.entry.pack(fill="x", ipady=8)
        self.entry.bind("<Return>", self._add)
        self.placeholder = "New task and Enter…"
        self._show_placeholder()
        self.entry.bind("<FocusIn>", self._clear_placeholder)
        self.entry.bind("<FocusOut>", lambda _e: self._show_placeholder())

        holder = tk.Frame(main, bg=theme.BG)
        holder.pack(fill="both", expand=True, pady=(12, 0))
        self.canvas = tk.Canvas(holder, bg=theme.BG, highlightthickness=0)
        style = ttk.Style(self.win)
        style.theme_use("clam")
        style.configure(
            "Taskline.Vertical.TScrollbar", background=theme.SURFACE, troughcolor=theme.BG,
            bordercolor=theme.BG, lightcolor=theme.SURFACE, darkcolor=theme.SURFACE,
            arrowcolor=theme.MUTED, gripcount=0,
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
        self.win.withdraw()

    def toggle(self) -> None:
        self.hide() if self.visible else self.show()

    def _escape(self, _event=None) -> None:
        if self.editing:
            self.editing = None
            self.refresh()
        else:
            self.hide()

    # --------------------------------------------------------------- input --
    def _show_placeholder(self) -> None:
        if not self.entry.get():
            self.entry.insert(0, self.placeholder)
            self.entry.configure(fg=theme.MUTED)

    def _clear_placeholder(self, _event=None) -> None:
        if self.entry.get() == self.placeholder and self.entry.cget("fg") == theme.MUTED:
            self.entry.delete(0, "end")
            self.entry.configure(fg=theme.FG)

    def _add(self, _event=None) -> None:
        text = self.entry.get().strip()
        if not text or (text == self.placeholder and self.entry.cget("fg") == theme.MUTED):
            return
        try:
            self.app.store.add(text)
        except ValueError:
            return
        self.entry.delete(0, "end")
        self.app.refresh()

    # -------------------------------------------------------------- render --
    def refresh(self) -> None:
        done, total = self.app.store.progress()
        self._draw_pie(done, total)
        self.done_label.configure(text=f"● Done: {done}")
        self.pending_label.configure(text=f"● Pending: {total - done}")
        for child in self.rows.winfo_children():
            child.destroy()
        tasks = self.app.store.today()
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
        c.create_oval(4, 4, 196, 196, fill=theme.MUTED, outline="")
        if ratio >= 1:
            c.create_oval(4, 4, 196, 196, fill=theme.ACCENT, outline="")
        elif ratio > 0:
            c.create_arc(4, 4, 196, 196, start=90, extent=-360 * ratio, fill=theme.ACCENT, outline="")
        hole = 100 * 0.58
        c.create_oval(100 - hole, 100 - hole, 100 + hole, 100 + hole, fill=theme.BG, outline="")
        c.create_text(100, 100, text=f"{round(ratio * 100)}%", fill=theme.FG, font=(theme.FONT, 26, "bold"))

    def _row(self, task: Task) -> None:
        row = tk.Frame(self.rows, bg=theme.BG)
        row.pack(fill="x", pady=2)

        def hover(on: bool) -> None:
            color = theme.HOVER if on else theme.BG
            for w in (row, *row.winfo_children()):
                if not isinstance(w, tk.Entry):
                    w.configure(bg=color)

        row.bind("<Enter>", lambda _e: hover(True))
        row.bind("<Leave>", lambda _e: hover(False))

        check = self._icon_button(
            row, "☑" if task.is_done else "☐", theme.ACCENT if task.is_done else theme.FG,
            lambda: self._toggle(task.uuid),
        )
        check.pack(side="left", padx=(8, 10))

        delete = self._icon_button(row, "✕", theme.RED, lambda: self._delete(task.uuid))
        delete.pack(side="right", padx=(8, 8))
        edit = self._icon_button(row, "✎", theme.FG, lambda: self._start_edit(task.uuid))
        edit.pack(side="right", padx=4)

        if self.editing == task.uuid:
            field = tk.Entry(
                row, bg=theme.SURFACE, fg=theme.FG, insertbackground=theme.FG, relief="flat",
                font=self.font,
            )
            text = task.description + (f" project:{task.project}" if task.project else "")
            field.insert(0, text)
            field.pack(side="left", fill="x", expand=True, ipady=4)
            field.bind("<Return>", lambda _e: self._save_edit(task.uuid, field.get()))
            field.after(10, lambda: (field.focus_set(), field.select_range(0, "end")))
        else:
            label = tk.Label(
                row, text=task.description + (f"   · {task.project}" if task.project else ""),
                bg=theme.BG, fg=theme.MUTED if task.is_done else theme.FG,
                font=self.font_done if task.is_done else self.font, anchor="w",
            )
            label.pack(side="left", fill="x", expand=True, ipady=6)
            label.bind("<Double-Button-1>", lambda _e: self._start_edit(task.uuid))

    def _icon_button(self, parent: tk.Widget, text: str, color: str, command) -> tk.Label:
        label = tk.Label(parent, text=text, bg=theme.BG, fg=color, font=(theme.SYMBOL_FONT, 13), cursor="hand2")
        label.bind("<Button-1>", lambda _e: command())
        return label

    # ------------------------------------------------------------- actions --
    def _toggle(self, task_uuid: str) -> None:
        self.app.store.toggle(task_uuid)
        self.app.refresh()

    def _delete(self, task_uuid: str) -> None:
        self.app.store.delete(task_uuid)
        if self.editing == task_uuid:
            self.editing = None
        self.app.refresh()

    def _start_edit(self, task_uuid: str) -> None:
        self.editing = None if self.editing == task_uuid else task_uuid
        self.refresh()

    def _save_edit(self, task_uuid: str, text: str) -> None:
        self.editing = None
        try:
            if text.strip():
                self.app.store.edit(task_uuid, text)
        except ValueError:
            pass
        self.app.refresh()


class TasklineApp:
    def __init__(self, store: TaskStore, with_tray: bool = True) -> None:
        self.store = store
        self.settings = system.load_settings()
        self.segments = int(self.settings.get("segments", 10))
        self.events: queue.Queue = queue.Queue()
        self.tray = None

        self.root = tk.Tk()
        self.root.withdraw()
        self.root.title("Taskline")
        self.icon_photo = self._make_icon_photo()

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
        self._drag_origin: tuple[int, int, int, int] | None = None
        self._dragged = False

        self.autostart_var = tk.BooleanVar(value=system.autostart_enabled())
        self.menu = tk.Menu(self.root, tearoff=False, bg=theme.SURFACE, fg=theme.FG,
                            activebackground=theme.ACCENT, activeforeground=theme.BG)
        self.menu.add_command(label="Open tasks", command=self.toggle_panel)
        self.menu.add_separator()
        self.menu.add_checkbutton(label="Start with Windows", variable=self.autostart_var,
                                  command=self._autostart_changed)
        self.menu.add_command(label="Import Taskwarrior JSON…", command=self.import_json)
        self.menu.add_command(label="Export Taskwarrior JSON…", command=self.export_json)
        self.menu.add_command(label="Reset bar position", command=self.reset_position)
        self.menu.add_separator()
        self.menu.add_command(label="Quit", command=self.quit)

        self.panel = Panel(self)
        self.refresh()
        self._place_bar()
        if with_tray:
            self._start_tray()
        self.root.after(POLL_MS, self._poll)
        self.root.after(REFRESH_MS, self._tick)

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
        pending = [t for t in self.store.today() if not t.is_done][:5]
        if not pending:
            return "All done for today"
        return "Pending:\n" + "\n".join(f"• {t.description}" for t in pending)

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
        self.bar_label.configure(
            text=progress_bar(done, total, self.segments),
            fg=theme.ACCENT if total > 0 and done == total else theme.FG,
        )
        if self.panel.visible:
            self.panel.refresh()
        if self.tray:
            self.tray.update(done, total)

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
    store = TaskStore(directory / "tasks.json")
    first = store.add("Review pull requests project:Work")
    store.add("Write the weekly report project:Work")
    store.add("Buy groceries project:Home")
    store.add("30 minutes of reading")
    store.complete(first.uuid)
    return store


def selftest(out: Path, screenshots: Path | None) -> int:
    """Exercise the real UI without user input; used by CI against the built .exe."""
    lines = [f"taskline {__version__}"]
    try:
        with tempfile.TemporaryDirectory() as tmp:
            import os

            os.environ["TASKLINE_DATA_DIR"] = tmp
            store = _demo_store(Path(tmp))
            app = TasklineApp(store, with_tray=False)
            from .tray import Tray  # noqa: F401  # the tray backend must be bundled

            app.root.update()
            lines.append(f"bar: {app.bar_label.cget('text')}")
            assert store.progress() == (1, 4), store.progress()
            app.panel.show()
            app.root.update()
            app.panel.entry.delete(0, "end")
            app.panel.entry.insert(0, "Added by selftest")
            app.panel._add()
            app.root.update()
            assert store.progress() == (1, 5), store.progress()
            lines.append(f"after add: {app.bar_label.cget('text')}")
            saved = json.loads((Path(tmp) / "tasks.json").read_text(encoding="utf-8"))
            assert len(saved) == 5
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


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(prog="taskline", description="Daily task progress bar")
    parser.add_argument("--version", action="version", version=f"taskline {__version__}")
    parser.add_argument("--selftest", metavar="REPORT", type=Path,
                        help="run a scripted UI check and write the result to REPORT")
    parser.add_argument("--screenshots", metavar="DIR", type=Path,
                        help="with --selftest, save screenshots of the bar and panel")
    args = parser.parse_args(argv)

    system.enable_dpi_awareness()
    if args.selftest:
        return selftest(args.selftest, args.screenshots)
    if not system.acquire_single_instance():
        return 0
    try:
        store = TaskStore(system.tasks_path())
    except ValueError as exc:
        messagebox.showerror("Taskline", str(exc))
        return 1
    TasklineApp(store).run()
    return 0


if __name__ == "__main__":
    sys.exit(main())
