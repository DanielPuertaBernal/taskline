from datetime import date, datetime, timedelta, timezone

import pytest

from taskline import calendar_model as cal
from taskline import i18n, theme
from taskline.model import TaskStore

NOW = datetime(2026, 11, 24, 15, 0, tzinfo=timezone.utc)


def test_month_grid_is_monday_first_six_weeks():
    grid = cal.month_grid(2026, 11)  # November 2026 starts on a Sunday
    assert len(grid) == 42
    assert grid[0] == date(2026, 10, 26)
    assert grid[6] == date(2026, 11, 1)
    assert grid[-1] == date(2026, 12, 6)
    assert all(d.weekday() == 0 for d in grid[::7])
    assert cal.month_grid(2027, 2)[0] == date(2027, 2, 1)  # starts on a Monday


@pytest.mark.parametrize(
    ("year", "month", "delta", "expected"),
    [(2026, 12, 1, (2027, 1)), (2026, 1, -1, (2025, 12)), (2026, 5, 0, (2026, 5)), (2026, 11, 14, (2028, 1))],
)
def test_shift_month(year, month, delta, expected):
    assert cal.shift_month(year, month, delta) == expected


def test_move_month_keeps_day_or_clamps():
    assert cal.move_month(date(2026, 1, 31), 1) == date(2026, 2, 28)
    assert cal.move_month(date(2026, 3, 15), -1) == date(2026, 2, 15)


@pytest.mark.parametrize(
    ("text", "expected"),
    [("hoy", date(2026, 11, 24)), ("today", date(2026, 11, 24)), ("mañana", date(2026, 11, 25)),
     ("+3d", date(2026, 11, 27)), ("+1w", date(2026, 12, 1)), ("2026-12-01", date(2026, 12, 1)),
     ("", None), ("friday", None), ("2026-13-01", None)],
)
def test_quick_date(text, expected):
    assert cal.quick_date(text, NOW, timezone.utc) == expected


def test_due_days_counts_pending_tasks(tmp_path):
    store = TaskStore(tmp_path / "t.json", clock=lambda: NOW, tz=timezone.utc)
    store.add("a due:2026-11-25")
    store.add("b due:2026-11-25")
    store.add("c due:2026-12-01")
    store.add("no date")
    done = store.add("done due:2026-11-30")
    store.complete(done.uuid)
    assert cal.due_days(store) == {date(2026, 11, 25): 2, date(2026, 12, 1): 1}


@pytest.mark.parametrize(
    ("text", "day", "expected"),
    [
        ("Plan trip project:Fun due:2026-11-27 priority:L", date(2026, 12, 3),
         "Plan trip project:Fun due:2026-12-03 priority:L"),
        ("Plan trip", date(2026, 12, 3), "Plan trip due:2026-12-03"),
        ("Plan trip due:2026-11-27 priority:L", None, "Plan trip priority:L"),
    ],
)
def test_set_due_token(text, day, expected):
    assert cal.set_due_token(text, day) == expected


# ------------------------------------------------------------------- i18n --
def test_names_follow_language():
    es = i18n.calendar_texts("es")
    assert es.month_title(2026, 11) == "noviembre 2026"
    assert es.weekdays == ("lu", "ma", "mi", "ju", "vi", "sá", "do")
    assert es.chips == ("Hoy", "Mañana", "+1 semana", "Sin fecha")
    assert [es.priority_name(p) for p in ("H", "M", "L", "", None)] == ["Alta", "Media", "Baja", "", ""]
    assert es.priorities[3] == "Sin prioridad"
    en = i18n.calendar_texts("en")
    assert en.month_title(2026, 11) == "November 2026"
    assert en.weekdays[0] == "Mo"
    assert en.chips == ("Today", "Tomorrow", "+1 week", "No date")
    assert en.priority_name("H") == "High"
    assert i18n.calendar_texts("xx") == en  # unknown languages fall back to English
    for lang in i18n.LANGUAGES:
        texts = i18n.calendar_texts(lang)
        assert len(texts.months) == 12 and len(texts.weekdays) == 7 and len(texts.chips) == 4
        assert len(texts.priorities) == 4


@pytest.mark.parametrize(
    ("tag", "expected"),
    [("es_CO", "es"), ("es-ES", "es"), ("pt_BR", "pt"), ("en_US", "en"), ("de_DE.UTF-8", "de"), ("", "en"),
     (None, "en"), ("ja_JP", "en")],
)
def test_language_from_tag(tag, expected):
    assert i18n.language_from_tag(tag) == expected


def test_language_env_override(monkeypatch):
    monkeypatch.setenv("TASKLINE_LANG", "es")
    assert i18n.current_language() == "es"


# ------------------------------------------------------------------ theme --
def test_light_and_dark_palettes():
    try:
        theme.apply("light")
        assert theme.BG == theme.LIGHT["BG"] and theme.MODE == "light"
        assert theme.DUE_COLORS["overdue"] == theme.LIGHT["RED"]
        theme.apply("dark")
        assert theme.BG == "#1a1b26" and theme.MODE == "dark"
        assert theme.PRIORITY_COLORS["H"] == "#f7768e"
    finally:
        theme.apply("dark")


def test_theme_mode_resolution(monkeypatch):
    monkeypatch.setattr(theme, "system_prefers_light", lambda: True)
    assert theme.resolve_mode({}) == "light"
    assert theme.resolve_mode({"theme": "system"}) == "light"
    assert theme.resolve_mode({"theme": "dark"}) == "dark"
    monkeypatch.setattr(theme, "system_prefers_light", lambda: False)
    assert theme.resolve_mode({"theme": "bogus"}) == "dark"


def test_quick_date_uses_local_day():
    bogota = timezone(timedelta(hours=-5))
    late = datetime(2026, 11, 25, 3, 0, tzinfo=timezone.utc)  # still the 24th in Bogota
    assert cal.quick_date("hoy", late, bogota) == date(2026, 11, 24)


@pytest.mark.parametrize("palette", [theme.DARK, theme.LIGHT], ids=["dark", "light"])
def test_text_colors_meet_wcag_aa(palette):
    for key in ("FG", "MUTED", "ACCENT", "RED", "ORANGE", "YELLOW", "GREEN", "CYAN", "PURPLE"):
        for back in ("BG", "SURFACE"):
            ratio = theme.contrast(palette[key], palette[back])
            assert ratio >= 4.5, f"{key} on {back}: {ratio:.2f}"


def test_date_picker_keyboard_and_chips():
    tk = pytest.importorskip("tkinter")
    try:
        root = tk.Tk()
    except tk.TclError:
        pytest.skip("no display")
    root.withdraw()
    from taskline.datepicker import DatePicker

    anchor = tk.Entry(root)
    anchor.pack()
    picks = []
    picker = DatePicker(root, today=lambda: date(2026, 11, 24), texts=i18n.calendar_texts("es"))
    try:
        picker.open(anchor, None, {date(2026, 11, 25): 2}, picks.append)
        root.update()
        assert picker.visible and picker.cursor == date(2026, 11, 24)
        assert picker.title.cget("text") == "noviembre 2026"
        assert picker.grid_days()[0] == date(2026, 10, 26)
        assert picker.handle_key("Right") and picker.cursor == date(2026, 11, 25)
        assert picker.handle_key("Down") and picker.cursor == date(2026, 12, 2)
        assert picker.title.cget("text") == "diciembre 2026"
        assert picker.handle_key("Prior") and picker.cursor == date(2026, 11, 2)
        assert picker.handle_key("Up") and picker.cursor == date(2026, 10, 26)
        assert not picker.handle_key("a")
        # Enter with unreadable text lets the entry submit it as typed.
        assert picker.handle_key("Return", "friday", parses=False) is False and not picker.visible
        picker.open(anchor, date(2026, 12, 3), {}, picks.append)
        assert picker.cursor == date(2026, 12, 3)
        assert picker.handle_key("Return", "2026-12-03", parses=True)
        assert picks[-1] == date(2026, 12, 3) and not picker.visible
        picker.open(anchor, None, {}, picks.append)
        picker.choose_chip(7)
        assert picks[-1] == date(2026, 12, 1)
        picker.open(anchor, date(2026, 12, 3), {}, picks.append)
        picker.choose_chip(None)
        assert picks[-1] is None
        picker.open(anchor, None, {}, picks.append)
        assert picker.handle_key("Escape") and not picker.visible and picks[-1] is None
    finally:
        picker.destroy()
        root.destroy()


def test_priority_and_due_words_in_edit_text():
    assert cal.set_priority_token("Plan trip due:2026-11-27 priority:L", "H") == "Plan trip due:2026-11-27 priority:H"
    assert cal.set_priority_token("Plan trip pri:M", None) == "Plan trip"
    assert cal.priority_in_text("x priority:h") == "H"
    assert cal.priority_in_text("x priority:Z") is None
    assert cal.due_in_text("x due:2026-12-01 priority:H", NOW, timezone.utc) == date(2026, 12, 1)
    assert cal.due_in_text("x due:friday", NOW, timezone.utc) is None
    assert cal.due_in_text("x", NOW, timezone.utc) is None
