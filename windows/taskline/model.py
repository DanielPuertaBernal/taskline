"""Task model with Taskwarrior semantics, stored as a Taskwarrior export.

The store file is a JSON array in the exact format `task export` produces,
so it can be imported into Taskwarrior and vice versa. Progress follows the
Omarchy widget: tasks completed today against today's completed plus every
task still pending.
"""

from __future__ import annotations

import json
import math
import os
import re
import tempfile
import uuid as uuidlib
from collections.abc import Callable, Iterable
from dataclasses import dataclass
from datetime import date, datetime, time, timedelta, timezone, tzinfo
from pathlib import Path

PENDING = "pending"
COMPLETED = "completed"
DELETED = "deleted"
STATUSES = {PENDING, COMPLETED, DELETED, "waiting", "recurring"}

TW_DATE = "%Y%m%dT%H%M%SZ"

FILLED = "▰"  # ▰
EMPTY = "▱"  # ▱

PRIORITIES = ("H", "M", "L")
SORT_DUE = "due"
SORT_PRIORITY = "priority"
SORTS = (SORT_DUE, SORT_PRIORITY)

OVERDUE = "overdue"
DUE_TODAY = "today"
UPCOMING = "upcoming"

_TODAY_WORDS = {"today", "hoy"}
_TOMORROW_WORDS = {"tomorrow", "mañana", "manana"}
_OFFSET = re.compile(r"^\+?(\d{1,4})([dw])$")


def format_date(moment: datetime) -> str:
    return moment.astimezone(timezone.utc).strftime(TW_DATE)


def parse_date(value: str | None) -> datetime | None:
    if not value:
        return None
    try:
        return datetime.strptime(value, TW_DATE).replace(tzinfo=timezone.utc)
    except ValueError:
        try:
            parsed = datetime.fromisoformat(value)
        except ValueError:
            return None
        return parsed if parsed.tzinfo else parsed.replace(tzinfo=timezone.utc)


def _local_midnight(day: date, tz: tzinfo | None) -> datetime:
    """Start of `day` in the local timezone, like Taskwarrior's `due:2026-12-01`."""
    if tz is None:
        return datetime.combine(day, time()).astimezone()
    return datetime.combine(day, time(), tzinfo=tz)


def _local_today(now: datetime, tz: tzinfo | None) -> date:
    return now.astimezone(tz).date() if tz else now.astimezone().date()


def parse_due(text: str, now: datetime, tz: tzinfo | None = None) -> datetime:
    """Turn quick input into a due moment at local midnight.

    Accepts today/hoy, tomorrow/mañana, YYYY-MM-DD and offsets such as +3d,
    3d or +2w. Raises ValueError for anything else.
    """
    value = text.strip().lower()
    today = _local_today(now, tz)
    if value in _TODAY_WORDS:
        day = today
    elif value in _TOMORROW_WORDS:
        day = today + timedelta(days=1)
    elif match := _OFFSET.match(value):
        amount = int(match.group(1)) * (7 if match.group(2) == "w" else 1)
        day = today + timedelta(days=amount)
    else:
        try:
            day = date.fromisoformat(value)
        except ValueError:
            raise ValueError(f"could not understand due date '{text}'") from None
    return _local_midnight(day, tz)


def parse_priority(text: str) -> str | None:
    """H, M or L (any case); an empty string clears it."""
    value = text.strip().upper()
    if not value:
        return None
    if value not in PRIORITIES:
        raise ValueError(f"priority must be H, M or L, not '{text}'")
    return value


def progress_bar(done: int, total: int, segments: int = 10) -> str:
    """Render the bar text exactly like the Omarchy widget: ▰▰▱▱ 1/4."""
    ratio = done / total if total > 0 else 0
    # Same rounding as JavaScript's Math.round in the QML widget (.5 rounds up).
    filled = min(segments, math.floor(ratio * segments + 0.5))
    return FILLED * filled + EMPTY * (segments - filled) + f"  {done}/{total}"


def parse_input(
    text: str, now: datetime | None = None, tz: tzinfo | None = None
) -> tuple[str, dict[str, str | None]]:
    """Split "Pay rent due:+3d priority:H project:Home" into a description and attributes.

    `project:`/`pro:`, `due:` and `priority:`/`pri:` are understood; `due:` or
    `priority:` with no value maps to None (clear it). Any other word is kept
    in the description. Raises ValueError for an unreadable due or priority.
    """
    words = text.split()
    attrs: dict[str, str | None] = {}
    rest = []
    for word in words:
        key, sep, value = word.partition(":")
        if sep and value and key in ("project", "pro"):
            attrs["project"] = value
        elif sep and key == "due":
            attrs["due"] = format_date(parse_due(value, now or _now(), tz)) if value else None
        elif sep and key in ("priority", "pri"):
            attrs["priority"] = parse_priority(value)
        else:
            rest.append(word)
    return " ".join(rest), attrs


def _migrate_record(record: dict) -> bool:
    """Normalize fields written by older builds or by hand. Returns True if changed.

    `due` in ISO form becomes the Taskwarrior format and lower-case priorities
    are upper-cased. Values that cannot be read are left untouched so no data
    is ever lost; the app simply ignores them.
    """
    changed = False
    due = record.get("due")
    if isinstance(due, str) and due:
        try:
            datetime.strptime(due, TW_DATE)
        except ValueError:
            parsed = parse_date(due)
            if parsed is not None:
                record["due"] = format_date(parsed)
                changed = True
    priority = record.get("priority")
    if isinstance(priority, str) and priority != priority.upper() and priority.upper() in PRIORITIES:
        record["priority"] = priority.upper()
        changed = True
    return changed


@dataclass
class Task:
    data: dict

    @property
    def uuid(self) -> str:
        return self.data["uuid"]

    @property
    def description(self) -> str:
        return self.data.get("description", "")

    @property
    def status(self) -> str:
        return self.data.get("status", PENDING)

    @property
    def project(self) -> str | None:
        return self.data.get("project")

    @property
    def due(self) -> datetime | None:
        value = self.data.get("due")
        try:
            return datetime.strptime(value, TW_DATE).replace(tzinfo=timezone.utc) if value else None
        except (TypeError, ValueError):
            return None

    @property
    def priority(self) -> str | None:
        value = self.data.get("priority")
        return value if value in PRIORITIES else None

    @property
    def end(self) -> datetime | None:
        return parse_date(self.data.get("end"))

    @property
    def entry(self) -> datetime | None:
        return parse_date(self.data.get("entry"))

    @property
    def is_done(self) -> bool:
        return self.status == COMPLETED


class TaskNotFound(KeyError):
    pass


class ReadOnlyError(RuntimeError):
    """The task file could not be read safely, so the store refuses to write."""


SCHEMA_VERSION = 2  # 1 = v1.0.x (no due/priority handling), 2 = v1.1+
KEEP_BACKUPS = 5


def _now() -> datetime:
    return datetime.now(timezone.utc)


def atomic_write(path: Path, text: str) -> None:
    """Write via a temp file in the same folder, fsync, then rename over `path`.

    An interrupted write (crash, power loss, full disk) leaves the previous
    file intact: the rename is the only step that touches it.
    """
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, tmp = tempfile.mkstemp(dir=path.parent, prefix="." + path.stem + ".", suffix=".tmp")
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as handle:
            handle.write(text)
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(tmp, path)
    except BaseException:
        if os.path.exists(tmp):
            os.unlink(tmp)
        raise


class TaskStore:
    """Tasks in a `task export` JSON array, plus a small sidecar with the schema.

    The task file stays a plain Taskwarrior export so older Taskline builds
    and Taskwarrior itself can still read it; the schema version lives in
    `tasks.meta.json` next to it. Before a schema upgrade the original file is
    copied to `tasks.json.bak-<old version>-<timestamp>` (the last five are
    kept). A file that cannot be read is never written: the store opens
    read-only with `load_error` explaining why.
    """

    def __init__(
        self,
        path: Path | str,
        clock: Callable[[], datetime] = _now,
        tz: tzinfo | None = None,
        app_version: str = "",
    ) -> None:
        self.path = Path(path)
        self.meta_path = self.path.with_name(self.path.stem + ".meta.json")
        self.clock = clock
        self.tz = tz
        self.app_version = app_version
        self._tasks: list[dict] = []
        self._mtime: float | None = None
        self.read_only = False
        self.load_error: str | None = None
        self.load()

    # ------------------------------------------------------------ storage --
    def _read_meta(self) -> dict:
        try:
            meta = json.loads(self.meta_path.read_text(encoding="utf-8"))
            return meta if isinstance(meta, dict) else {}
        except (OSError, ValueError):
            return {}

    def _write_meta(self) -> None:
        meta = {"schemaVersion": SCHEMA_VERSION, "appVersion": self.app_version or None}
        atomic_write(self.meta_path, json.dumps(meta, indent=1) + "\n")

    def backups(self) -> list[Path]:
        """Migration backups, oldest first."""
        found = self.path.parent.glob(self.path.name + ".bak-*")
        return sorted(found, key=lambda p: (p.stat().st_mtime, p.name))

    def _backup(self, old_version: str) -> Path:
        stamp = self.clock().astimezone(self.tz).strftime("%Y%m%d-%H%M%S")
        backup = self.path.with_name(f"{self.path.name}.bak-{old_version}-{stamp}")
        n = 1
        while backup.exists():
            n += 1
            backup = self.path.with_name(f"{self.path.name}.bak-{old_version}-{stamp}-{n}")
        backup.write_bytes(self.path.read_bytes())
        for old in self.backups()[:-KEEP_BACKUPS]:
            old.unlink(missing_ok=True)
        return backup

    def _fail_read_only(self, reason: str) -> None:
        self._tasks = []
        self.read_only = True
        self.load_error = reason

    def load(self) -> None:
        self.read_only = False
        self.load_error = None
        if not self.path.exists():
            self._tasks = []
            self._mtime = None
            return
        try:
            raw = json.loads(self.path.read_text(encoding="utf-8") or "[]")
        except (OSError, ValueError) as exc:
            self._fail_read_only(f"{self.path} could not be read ({exc}). It was left untouched.")
            return
        if not isinstance(raw, list):
            self._fail_read_only(f"{self.path} is not a Taskwarrior export (expected a JSON array). "
                                 "It was left untouched.")
            return
        tasks = [t for t in raw if isinstance(t, dict) and t.get("uuid")]
        self._mtime = self.path.stat().st_mtime
        meta = self._read_meta()
        stored = meta.get("schemaVersion")
        stored = stored if isinstance(stored, int) else 1
        if stored >= SCHEMA_VERSION:
            self._tasks = tasks
            return
        # Upgrade: back the file up first, migrate a copy, and only then save.
        try:
            self._backup(str(meta.get("appVersion") or ("1.0.0" if stored == 1 else stored)))
            migrated = [dict(t) for t in tasks]
            changed = [_migrate_record(t) for t in migrated]
            self._tasks = migrated
            if any(changed):
                self.save()
            self._write_meta()
        except Exception as exc:  # never lose data: fall back to the untouched original
            self._fail_read_only(f"Could not upgrade {self.path} ({exc}). It was left untouched.")
            self._tasks = tasks

    def reload_if_changed(self) -> bool:
        """Reload when another process edited the file. Returns True if reloaded."""
        try:
            mtime = self.path.stat().st_mtime
        except FileNotFoundError:
            return False
        if mtime != self._mtime:
            self.load()
            return True
        return False

    def _check_writable(self) -> None:
        if self.read_only:
            raise ReadOnlyError(self.load_error or "the task file is read-only")

    def save(self) -> None:
        self._check_writable()
        first = not self.meta_path.exists()
        atomic_write(self.path, json.dumps(self._tasks, ensure_ascii=False, indent=1))
        self._mtime = self.path.stat().st_mtime
        if first:
            self._write_meta()

    # ------------------------------------------------------------- queries --
    def all(self) -> list[Task]:
        return [Task(t) for t in self._tasks]

    def get(self, task_uuid: str) -> Task:
        for data in self._tasks:
            if data["uuid"] == task_uuid:
                return Task(data)
        raise TaskNotFound(task_uuid)

    def _local_date(self, moment: datetime):
        return moment.astimezone(self.tz).date() if self.tz else moment.astimezone().date()

    def completed_today(self, task: Task) -> bool:
        end = task.end
        return task.is_done and end is not None and self._local_date(end) == self._local_date(self.clock())

    def due_state(self, task: Task) -> str | None:
        """overdue, today or upcoming for pending tasks with a due date."""
        due = task.due
        if due is None or task.status != PENDING:
            return None
        day, today = self._local_date(due), self._local_date(self.clock())
        if day < today:
            return OVERDUE
        return DUE_TODAY if day == today else UPCOMING

    @staticmethod
    def _due_key(task: Task) -> tuple:
        due = task.due
        return (due is None, due or datetime.min.replace(tzinfo=timezone.utc))

    def sort_key(self, sort: str) -> Callable[[Task], tuple]:
        if sort == SORT_PRIORITY:
            rank = {p: i for i, p in enumerate(PRIORITIES)}
            return lambda t: (rank.get(t.priority, len(PRIORITIES)), *self._due_key(t), t.data.get("entry", ""))
        return lambda t: (*self._due_key(t), t.data.get("entry", ""))

    def today(self, sort: str = SORT_DUE) -> list[Task]:
        """Pending tasks in `sort` order, then tasks completed today (latest first).

        due: soonest due first, tasks without a date last. priority: H, M, L,
        then none, ties broken by due date.
        """
        tasks = self.all()
        pending = [t for t in tasks if t.status == PENDING]
        pending.sort(key=self.sort_key(sort))
        done = [t for t in tasks if self.completed_today(t)]
        done.sort(key=lambda t: t.data.get("end", ""), reverse=True)
        return pending + done

    def progress(self) -> tuple[int, int]:
        tasks = self.today()
        done = sum(1 for t in tasks if t.is_done)
        return done, len(tasks)

    # ------------------------------------------------------------ commands --
    def _touch(self, data: dict) -> None:
        data["modified"] = format_date(self.clock())

    def _parse(self, text: str) -> tuple[str, dict[str, str | None]]:
        description, attrs = parse_input(text, self.clock(), self.tz)
        if not description:
            raise ValueError("description must not be empty")
        return description, attrs

    def add(self, text: str, due: str | None = None, priority: str | None = None) -> Task:
        """Add a task. `due`/`priority` (quick input) override modifiers in `text`."""
        self._check_writable()
        description, attrs = self._parse(text)
        if due:
            attrs["due"] = format_date(parse_due(due, self.clock(), self.tz))
        if priority:
            attrs["priority"] = parse_priority(priority)
        attrs = {k: v for k, v in attrs.items() if v is not None}
        now = format_date(self.clock())
        data = {
            "uuid": str(uuidlib.uuid4()),
            "description": description,
            "status": PENDING,
            "entry": now,
            "modified": now,
            **attrs,
        }
        self._tasks.append(data)
        self.save()
        return Task(data)

    def complete(self, task_uuid: str) -> Task:
        self._check_writable()
        data = self.get(task_uuid).data
        data["status"] = COMPLETED
        data["end"] = format_date(self.clock())
        self._touch(data)
        self.save()
        return Task(data)

    def reopen(self, task_uuid: str) -> Task:
        self._check_writable()
        data = self.get(task_uuid).data
        data["status"] = PENDING
        data.pop("end", None)
        self._touch(data)
        self.save()
        return Task(data)

    def toggle(self, task_uuid: str) -> Task:
        task = self.get(task_uuid)
        return self.reopen(task_uuid) if task.is_done else self.complete(task_uuid)

    def edit(self, task_uuid: str, text: str) -> Task:
        """Apply the text of the edit field (see edit_text).

        The project is kept when not mentioned; due and priority are part of
        the edit text, so leaving them out clears them.
        """
        self._check_writable()
        description, attrs = self._parse(text)
        data = self.get(task_uuid).data
        data["description"] = description
        for key in ("due", "priority"):
            attrs.setdefault(key, None)
        for key, value in attrs.items():
            if value is None:
                data.pop(key, None)
            else:
                data[key] = value
        self._touch(data)
        self.save()
        return Task(data)

    def due_label(self, task: Task) -> str:
        """Short text for the due badge: "overdue · Nov 23", "today", "tomorrow", "Fri Nov 27"."""
        due = task.due
        if due is None:
            return ""
        day, today = self._local_date(due), self._local_date(self.clock())
        short = f"{day:%b} {day.day}" + (f", {day.year}" if day.year != today.year else "")
        if day < today:
            return f"overdue · {short}"
        if day == today:
            return "today"
        if day == today + timedelta(days=1):
            return "tomorrow"
        return short if day.year != today.year else f"{day:%a} {short}"

    def edit_text(self, task: Task) -> str:
        """The editable form of a task, which edit() reads back."""
        parts = [task.description]
        if task.project:
            parts.append(f"project:{task.project}")
        if task.due:
            parts.append(f"due:{self._local_date(task.due).isoformat()}")
        if task.priority:
            parts.append(f"priority:{task.priority}")
        return " ".join(parts)

    def delete(self, task_uuid: str) -> Task:
        """Mark deleted like `task delete`; the record stays for export."""
        self._check_writable()
        data = self.get(task_uuid).data
        data["status"] = DELETED
        data["end"] = format_date(self.clock())
        self._touch(data)
        self.save()
        return Task(data)

    # ------------------------------------------------------ import/export --
    def export_taskwarrior(self) -> list[dict]:
        return [dict(t) for t in self._tasks]

    def import_taskwarrior(self, records: Iterable[dict]) -> int:
        """Merge `task export` records by uuid; the newer `modified` wins."""
        self._check_writable()
        by_uuid = {t["uuid"]: t for t in self._tasks}
        count = 0
        for record in records:
            if not isinstance(record, dict) or not record.get("uuid") or not record.get("description"):
                continue
            if record.get("status", PENDING) not in STATUSES:
                continue
            clean = {k: v for k, v in record.items() if k not in ("id", "urgency")}
            current = by_uuid.get(clean["uuid"])
            if current is None:
                clean.setdefault("status", PENDING)
                clean.setdefault("entry", format_date(self.clock()))
                self._tasks.append(clean)
                by_uuid[clean["uuid"]] = clean
                count += 1
            elif clean.get("modified", "") >= current.get("modified", ""):
                current.clear()
                current.update(clean)
                count += 1
        if count:
            self.save()
        return count

    def import_file(self, path: Path | str) -> int:
        records = json.loads(Path(path).read_text(encoding="utf-8"))
        if isinstance(records, dict):
            records = [records]
        if not isinstance(records, list):
            raise ValueError("expected a Taskwarrior export (JSON array)")
        return self.import_taskwarrior(records)

    def export_file(self, path: Path | str) -> None:
        Path(path).write_text(
            json.dumps(self.export_taskwarrior(), ensure_ascii=False, indent=1), encoding="utf-8"
        )
