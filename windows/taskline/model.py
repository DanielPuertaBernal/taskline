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
import tempfile
import uuid as uuidlib
from collections.abc import Callable, Iterable
from dataclasses import dataclass
from datetime import datetime, timezone, tzinfo
from pathlib import Path

PENDING = "pending"
COMPLETED = "completed"
DELETED = "deleted"
STATUSES = {PENDING, COMPLETED, DELETED, "waiting", "recurring"}

TW_DATE = "%Y%m%dT%H%M%SZ"

FILLED = "▰"  # ▰
EMPTY = "▱"  # ▱


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


def progress_bar(done: int, total: int, segments: int = 10) -> str:
    """Render the bar text exactly like the Omarchy widget: ▰▰▱▱ 1/4."""
    ratio = done / total if total > 0 else 0
    # Same rounding as JavaScript's Math.round in the QML widget (.5 rounds up).
    filled = min(segments, math.floor(ratio * segments + 0.5))
    return FILLED * filled + EMPTY * (segments - filled) + f"  {done}/{total}"


def parse_input(text: str) -> tuple[str, dict[str, str]]:
    """Split "Buy milk project:Home" into a description and attributes.

    Only `project:` is understood; any other word is kept in the description.
    """
    words = text.split()
    attrs: dict[str, str] = {}
    rest = []
    for word in words:
        key, sep, value = word.partition(":")
        if sep and value and key in ("project", "pro"):
            attrs["project"] = value
        else:
            rest.append(word)
    return " ".join(rest), attrs


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


def _now() -> datetime:
    return datetime.now(timezone.utc)


class TaskStore:
    def __init__(
        self,
        path: Path | str,
        clock: Callable[[], datetime] = _now,
        tz: tzinfo | None = None,
    ) -> None:
        self.path = Path(path)
        self.clock = clock
        self.tz = tz
        self._tasks: list[dict] = []
        self._mtime: float | None = None
        self.load()

    # ------------------------------------------------------------ storage --
    def load(self) -> None:
        if not self.path.exists():
            self._tasks = []
            self._mtime = None
            return
        raw = json.loads(self.path.read_text(encoding="utf-8") or "[]")
        if not isinstance(raw, list):
            raise ValueError(f"{self.path} is not a Taskwarrior export (expected a JSON array)")
        self._tasks = [t for t in raw if isinstance(t, dict) and t.get("uuid")]
        self._mtime = self.path.stat().st_mtime

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

    def save(self) -> None:
        self.path.parent.mkdir(parents=True, exist_ok=True)
        fd, tmp = tempfile.mkstemp(dir=self.path.parent, prefix=".tasks.", suffix=".json")
        try:
            with os.fdopen(fd, "w", encoding="utf-8") as handle:
                json.dump(self._tasks, handle, ensure_ascii=False, indent=1)
            os.replace(tmp, self.path)
        except BaseException:
            if os.path.exists(tmp):
                os.unlink(tmp)
            raise
        self._mtime = self.path.stat().st_mtime

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

    def today(self) -> list[Task]:
        """Pending tasks (oldest first), then tasks completed today (latest first)."""
        tasks = self.all()
        pending = [t for t in tasks if t.status == PENDING]
        pending.sort(key=lambda t: (-(t.data.get("urgency") or 0), t.data.get("entry", "")))
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

    def add(self, text: str) -> Task:
        description, attrs = parse_input(text)
        if not description:
            raise ValueError("description must not be empty")
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
        data = self.get(task_uuid).data
        data["status"] = COMPLETED
        data["end"] = format_date(self.clock())
        self._touch(data)
        self.save()
        return Task(data)

    def reopen(self, task_uuid: str) -> Task:
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
        description, attrs = parse_input(text)
        if not description:
            raise ValueError("description must not be empty")
        data = self.get(task_uuid).data
        data["description"] = description
        data.update(attrs)
        self._touch(data)
        self.save()
        return Task(data)

    def delete(self, task_uuid: str) -> Task:
        """Mark deleted like `task delete`; the record stays for export."""
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
