import json
import os
from datetime import datetime, timedelta, timezone

import pytest

from taskline.model import (
    COMPLETED,
    DELETED,
    PENDING,
    TaskNotFound,
    TaskStore,
    parse_input,
    progress_bar,
)


class Clock:
    def __init__(self, moment):
        self.moment = moment

    def __call__(self):
        return self.moment

    def advance(self, **kwargs):
        self.moment += timedelta(**kwargs)


@pytest.fixture
def clock():
    return Clock(datetime(2026, 11, 24, 15, 0, tzinfo=timezone.utc))


@pytest.fixture
def store(tmp_path, clock):
    return TaskStore(tmp_path / "taskline" / "tasks.json", clock=clock, tz=timezone.utc)


def test_empty_store_has_no_progress(store):
    assert store.progress() == (0, 0)
    assert store.today() == []


def test_add_creates_pending_task_and_persists(store, tmp_path, clock):
    task = store.add("Write report")
    assert task.status == PENDING
    assert task.description == "Write report"
    assert store.progress() == (0, 1)
    saved = json.loads((tmp_path / "taskline" / "tasks.json").read_text())
    assert saved[0]["uuid"] == task.uuid
    assert saved[0]["entry"] == "20261124T150000Z"
    reloaded = TaskStore(store.path, clock=clock, tz=timezone.utc)
    assert [t.description for t in reloaded.today()] == ["Write report"]


def test_add_parses_project_and_rejects_empty(store):
    task = store.add("Buy milk project:Home")
    assert task.description == "Buy milk"
    assert task.project == "Home"
    with pytest.raises(ValueError):
        store.add("   ")
    with pytest.raises(ValueError):
        store.add("project:Only")


def test_complete_counts_towards_today(store):
    a = store.add("one")
    store.add("two")
    store.add("three")
    store.add("four")
    store.complete(a.uuid)
    assert store.get(a.uuid).status == COMPLETED
    assert store.get(a.uuid).data["end"] == "20261124T150000Z"
    assert store.progress() == (1, 4)
    # Pending first, then today's completed.
    assert [t.description for t in store.today()] == ["two", "three", "four", "one"]


def test_completed_yesterday_is_not_counted(store, clock):
    a = store.add("old")
    store.complete(a.uuid)
    store.add("new")
    clock.advance(days=1)
    assert store.progress() == (0, 1)
    assert [t.description for t in store.today()] == ["new"]


def test_completed_today_respects_local_timezone(tmp_path, clock):
    bogota = timezone(timedelta(hours=-5))
    store = TaskStore(tmp_path / "t.json", clock=clock, tz=bogota)
    # 03:00 UTC on the 25th is still the 24th in Bogota.
    clock.moment = datetime(2026, 11, 25, 3, 0, tzinfo=timezone.utc)
    store.complete(store.add("late night").uuid)
    clock.moment = datetime(2026, 11, 24, 20, 0, tzinfo=timezone.utc)
    assert store.progress() == (1, 1)


def test_toggle_reopens_completed_task(store):
    a = store.add("toggle me")
    store.toggle(a.uuid)
    assert store.get(a.uuid).is_done
    store.toggle(a.uuid)
    task = store.get(a.uuid)
    assert task.status == PENDING
    assert "end" not in task.data
    assert store.progress() == (0, 1)


def test_delete_hides_task_but_keeps_record(store):
    a = store.add("drop me")
    store.add("keep me")
    store.delete(a.uuid)
    assert store.get(a.uuid).status == DELETED
    assert store.progress() == (0, 1)
    assert [t.description for t in store.today()] == ["keep me"]
    assert any(t["uuid"] == a.uuid for t in store.export_taskwarrior())


def test_edit_changes_description(store):
    a = store.add("tpyo")
    store.edit(a.uuid, "typo project:Fix")
    task = store.get(a.uuid)
    assert task.description == "typo"
    assert task.project == "Fix"


def test_unknown_uuid_raises(store):
    with pytest.raises(TaskNotFound):
        store.complete("nope")


def test_import_taskwarrior_export_merges_by_uuid(store, tmp_path):
    a = store.add("local")
    export = [
        {"id": 1, "uuid": "11111111-1111-1111-1111-111111111111", "description": "from tw",
         "status": "pending", "entry": "20261124T100000Z", "modified": "20261124T100000Z",
         "urgency": 1.8, "tags": ["work"]},
        {"id": 0, "uuid": "22222222-2222-2222-2222-222222222222", "description": "done in tw",
         "status": "completed", "entry": "20261124T090000Z", "end": "20261124T110000Z",
         "modified": "20261124T110000Z"},
        {"uuid": a.uuid, "description": "local (newer)", "status": "pending",
         "entry": a.data["entry"], "modified": "20991231T000000Z"},
        {"uuid": "bad", "status": "pending"},  # no description: skipped
    ]
    path = tmp_path / "export.json"
    path.write_text(json.dumps(export))
    assert store.import_file(path) == 3
    assert store.progress() == (1, 3)
    imported = store.get("11111111-1111-1111-1111-111111111111").data
    assert "id" not in imported and "urgency" not in imported
    assert imported["tags"] == ["work"]
    assert store.get(a.uuid).description == "local (newer)"
    # Importing the same data again changes nothing new for older records.
    older = [{"uuid": a.uuid, "description": "stale", "modified": "20000101T000000Z"}]
    assert store.import_taskwarrior(older) == 0
    assert store.get(a.uuid).description == "local (newer)"


def test_export_round_trip(store, tmp_path, clock):
    store.complete(store.add("x").uuid)
    store.add("y")
    out = tmp_path / "out.json"
    store.export_file(out)
    other = TaskStore(tmp_path / "other.json", clock=clock, tz=timezone.utc)
    assert other.import_file(out) == 2
    assert other.progress() == (1, 2)


def test_store_rejects_non_array_file(tmp_path):
    path = tmp_path / "tasks.json"
    path.write_text('{"not": "a list"}')
    with pytest.raises(ValueError):
        TaskStore(path)


def test_reload_if_changed_picks_up_external_edits(store, clock):
    store.add("mine")
    other = TaskStore(store.path, clock=clock, tz=timezone.utc)
    other.add("theirs")
    os.utime(store.path, (1, 1))  # force a different mtime than the cached one
    assert store.reload_if_changed() is True
    assert store.progress() == (0, 2)


@pytest.mark.parametrize(
    ("done", "total", "expected"),
    [
        (0, 0, "▱" * 10 + "  0/0"),
        (1, 4, "\u25b0" * 3 + "\u25b1" * 7 + "  1/4"),  # 2.5 rounds up like Math.round
        (4, 4, "▰" * 10 + "  4/4"),
    ],
)
def test_progress_bar(done, total, expected):
    assert progress_bar(done, total) == expected


def test_parse_input_keeps_other_modifiers_in_text():
    assert parse_input("call mom due:friday pro:Family") == ("call mom due:friday", {"project": "Family"})
