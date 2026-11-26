"""Pure helpers behind the date picker: the month grid, quick input and the
days that already have tasks. Kept free of Tk so they are unit tested."""

from __future__ import annotations

import calendar
import re
from datetime import date, datetime, timedelta, tzinfo

from .model import PENDING, TaskStore, parse_due


def month_grid(year: int, month: int) -> list[date]:
    """42 days (6 weeks), Monday first, covering `month`."""
    first = date(year, month, 1)
    start = first - timedelta(days=first.weekday())
    return [start + timedelta(days=i) for i in range(42)]


def shift_month(year: int, month: int, delta: int) -> tuple[int, int]:
    index = year * 12 + (month - 1) + delta
    return index // 12, index % 12 + 1


def move_month(day: date, delta: int) -> date:
    """Same day of the month `delta` months away, clamped to the month's length."""
    year, month = shift_month(day.year, day.month, delta)
    return date(year, month, min(day.day, calendar.monthrange(year, month)[1]))


def quick_date(text: str, now: datetime, tz: tzinfo | None = None) -> date | None:
    """The local day typed quick input points at, or None when it is not a date."""
    if not text.strip():
        return None
    try:
        due = parse_due(text, now, tz)
    except ValueError:
        return None
    return due.astimezone(tz).date() if tz else due.date()


def due_days(store: TaskStore) -> dict[date, int]:
    """Pending tasks per local due day, for the dots in the calendar."""
    days: dict[date, int] = {}
    for task in store.all():
        if task.status == PENDING and task.due is not None:
            day = store._local_date(task.due)
            days[day] = days.get(day, 0) + 1
    return days


_DUE_WORD = re.compile(r"^due:")
_PRIORITY_WORD = re.compile(r"^(priority|pri):")


def set_due_token(text: str, day: date | None) -> str:
    """Write `due:<day>` into edit text, replacing any due: word; None removes it."""
    words = [w for w in text.split() if not _DUE_WORD.match(w)]
    if day is not None:
        at = next((i for i, w in enumerate(words) if _PRIORITY_WORD.match(w)), len(words))
        words.insert(at, f"due:{day.isoformat()}")
    return " ".join(words)


def set_priority_token(text: str, priority: str | None) -> str:
    """Write `priority:<H|M|L>` into edit text, replacing priority:/pri:; None removes it."""
    words = [w for w in text.split() if not _PRIORITY_WORD.match(w)]
    if priority:
        words.append(f"priority:{priority}")
    return " ".join(words)


def due_in_text(text: str, now: datetime, tz: tzinfo | None = None) -> date | None:
    """The date of the `due:` word in edit text, if it is one the calendar can show."""
    match = re.search(r"(?:^|\s)due:(\S+)", text)
    return quick_date(match.group(1), now, tz) if match else None


def priority_in_text(text: str) -> str | None:
    match = re.search(r"(?:^|\s)(?:priority|pri):(\S+)", text)
    value = match.group(1).upper() if match else ""
    return value if value in ("H", "M", "L") else None
