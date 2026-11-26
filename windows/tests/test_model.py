import json
import os
from datetime import datetime, timedelta, timezone

import pytest

from taskline.model import (
    COMPLETED,
    DELETED,
    PENDING,
    ReadOnlyError,
    TaskNotFound,
    TaskStore,
    format_date,
    parse_due,
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


def test_store_opens_non_array_file_read_only(tmp_path):
    path = tmp_path / "tasks.json"
    path.write_text('{"not": "a list"}')
    store = TaskStore(path)
    assert store.read_only
    assert "not a Taskwarrior export" in store.load_error
    with pytest.raises(ReadOnlyError):
        store.add("anything")
    assert path.read_text() == '{"not": "a list"}'


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


def test_parse_input_keeps_unknown_modifiers_in_text():
    assert parse_input("call mom tags:x pro:Family") == ("call mom tags:x", {"project": "Family"})


# ---------------------------------------------------------------- due dates --
BOGOTA = timezone(timedelta(hours=-5))
NOW = datetime(2026, 11, 24, 15, 0, tzinfo=timezone.utc)  # 10:00 in Bogota


@pytest.mark.parametrize(
    ("text", "expected"),
    [
        ("today", "20261124T050000Z"),
        ("hoy", "20261124T050000Z"),
        ("tomorrow", "20261125T050000Z"),
        ("mañana", "20261125T050000Z"),
        ("2026-12-01", "20261201T050000Z"),
        ("+3d", "20261127T050000Z"),
        ("3d", "20261127T050000Z"),
        ("+2w", "20261208T050000Z"),
    ],
)
def test_parse_due_quick_inputs_are_local_midnight(text, expected):
    assert format_date(parse_due(text, NOW, BOGOTA)) == expected


@pytest.mark.parametrize("text", ["friday", "2026-13-01", "+d", "soon"])
def test_parse_due_rejects_unknown_input(text):
    with pytest.raises(ValueError):
        parse_due(text, NOW, BOGOTA)


def test_parse_input_reads_due_and_priority():
    description, attrs = parse_input("Pay rent due:2026-12-01 priority:h", NOW, BOGOTA)
    assert description == "Pay rent"
    assert attrs == {"due": "20261201T050000Z", "priority": "H"}
    assert parse_input("x pri:L", NOW, BOGOTA)[1] == {"priority": "L"}
    assert parse_input("x due: priority:", NOW, BOGOTA)[1] == {"due": None, "priority": None}
    with pytest.raises(ValueError):
        parse_input("x priority:Z", NOW, BOGOTA)
    with pytest.raises(ValueError):
        parse_input("x due:someday", NOW, BOGOTA)


def test_add_with_due_and_priority_persists_taskwarrior_fields(store, tmp_path):
    task = store.add("Pay rent due:2026-12-01 priority:H project:Home")
    assert task.description == "Pay rent"
    assert task.priority == "H"
    assert task.due == datetime(2026, 12, 1, tzinfo=timezone.utc)
    saved = json.loads((tmp_path / "taskline" / "tasks.json").read_text())[0]
    assert saved["due"] == "20261201T000000Z"
    assert saved["priority"] == "H"
    assert saved["project"] == "Home"


def test_add_with_explicit_due_and_priority_arguments(store):
    task = store.add("Call bank", due="tomorrow", priority="m")
    assert task.data["due"] == "20261125T000000Z"
    assert task.priority == "M"
    with pytest.raises(ValueError):
        store.add("bad", due="whenever")
    assert [t.description for t in store.today()] == ["Call bank"]


def test_edit_sets_and_clears_due_and_priority(store):
    task = store.add("Plan trip due:today priority:L project:Fun")
    store.edit(task.uuid, "Plan the trip due:+3d priority:H")
    edited = store.get(task.uuid)
    assert edited.description == "Plan the trip"
    assert edited.data["due"] == "20261127T000000Z"
    assert edited.priority == "H"
    assert edited.project == "Fun"  # project is kept when not mentioned, as before
    store.edit(task.uuid, "Plan the trip")
    cleared = store.get(task.uuid)
    assert "due" not in cleared.data and "priority" not in cleared.data
    full = store.add("A due:2026-12-01 priority:M project:X")
    assert store.edit_text(full) == "A project:X due:2026-12-01 priority:M"


def test_due_state(store, clock):
    overdue = store.add("late due:2026-11-23")
    today = store.add("now due:today")
    upcoming = store.add("soon due:tomorrow")
    none = store.add("whenever")
    assert store.due_state(overdue) == "overdue"
    assert store.due_state(today) == "today"
    assert store.due_state(upcoming) == "upcoming"
    assert store.due_state(none) is None
    store.complete(overdue.uuid)
    assert store.due_state(store.get(overdue.uuid)) is None  # finished tasks are never overdue


def test_sort_by_due_puts_soonest_first_and_no_date_last(store):
    store.add("no date")
    store.add("next week due:+7d")
    store.add("overdue due:2026-11-20")
    store.add("tomorrow due:tomorrow priority:H")
    done = store.add("done today due:2026-11-01")
    store.complete(done.uuid)
    assert [t.description for t in store.today(sort="due")] == [
        "overdue", "tomorrow", "next week", "no date", "done today",
    ]


def test_sort_by_priority_breaks_ties_by_due(store):
    store.add("none early due:2026-11-20")
    store.add("low")
    store.add("high late due:+9d priority:H")
    store.add("high soon due:tomorrow priority:H")
    store.add("high no date priority:H")
    store.add("medium priority:M")
    assert [t.description for t in store.today(sort="priority")] == [
        "high soon", "high late", "high no date", "medium", "none early", "low",
    ]


def test_unknown_sort_falls_back_to_due(store):
    store.add("b due:+2d")
    store.add("a due:+1d")
    assert [t.description for t in store.today(sort="bogus")] == ["a", "b"]


# ---------------------------------------------------------------- migration --
V1_RECORDS = [
    {"uuid": "u1", "description": "old", "status": "pending", "entry": "20261120T100000Z",
     "modified": "20261120T100000Z", "project": "Home", "tags": ["a"], "annotations": [{"x": 1}]},
    {"uuid": "u2", "description": "done", "status": "completed", "entry": "20261120T100000Z",
     "end": "20261124T120000Z", "modified": "20261124T120000Z"},
    {"uuid": "u3", "description": "gone", "status": "deleted", "entry": "20261120T100000Z",
     "end": "20261121T120000Z", "modified": "20261121T120000Z"},
]


def test_v1_0_file_keeps_every_task_and_is_backed_up(tmp_path, clock):
    path = tmp_path / "tasks.json"
    path.write_text(json.dumps(V1_RECORDS, indent=1), encoding="utf-8")
    before = path.read_bytes()
    store = TaskStore(path, clock=clock, tz=timezone.utc, app_version="1.1.0")
    assert not store.read_only
    assert store.export_taskwarrior() == V1_RECORDS
    assert path.read_bytes() == before  # nothing needed rewriting
    backups = store.backups()
    assert [b.name for b in backups] == ["tasks.json.bak-1.0.0-20261124-150000"]
    assert backups[0].read_bytes() == before
    meta = json.loads((tmp_path / "tasks.meta.json").read_text())
    assert meta == {"schemaVersion": 2, "appVersion": "1.1.0"}
    # Second start: already migrated, no new backup.
    TaskStore(path, clock=clock, tz=timezone.utc, app_version="1.1.0")
    assert len(store.backups()) == 1


def test_legacy_due_and_priority_are_migrated(tmp_path, clock):
    records = [
        {"uuid": "u1", "description": "iso", "status": "pending", "due": "2026-12-01", "priority": "h",
         "custom": "kept"},
        {"uuid": "u2", "description": "iso time", "status": "pending", "due": "2026-12-02T08:30:00+00:00"},
        {"uuid": "u3", "description": "junk", "status": "pending", "due": "whenever", "priority": "urgent"},
    ]
    path = tmp_path / "tasks.json"
    path.write_text(json.dumps(records), encoding="utf-8")
    original = path.read_text(encoding="utf-8")
    store = TaskStore(path, clock=clock, tz=timezone.utc)
    assert store.get("u1").data["due"] == "20261201T000000Z"
    assert store.get("u1").priority == "H"
    assert store.get("u1").data["custom"] == "kept"
    assert store.get("u2").data["due"] == "20261202T083000Z"
    # Unreadable values are kept verbatim rather than dropped, and ignored for sorting.
    assert store.get("u3").data["due"] == "whenever"
    assert store.get("u3").priority is None
    assert store.get("u3").data["priority"] == "urgent"
    assert store.backups()[0].read_text(encoding="utf-8") == original
    assert json.loads(path.read_text(encoding="utf-8"))[0]["due"] == "20261201T000000Z"
    assert [t.description for t in store.today(sort="due")] == ["iso", "iso time", "junk"]


def test_only_the_last_five_backups_are_kept(tmp_path, clock):
    path = tmp_path / "tasks.json"
    for i in range(7):
        path.write_text(json.dumps(V1_RECORDS), encoding="utf-8")
        (tmp_path / "tasks.meta.json").unlink(missing_ok=True)
        TaskStore(path, clock=clock, tz=timezone.utc)
        newest = sorted(tmp_path.glob("tasks.json.bak-*"), key=lambda p: p.stat().st_mtime)[-1]
        os.utime(newest, (1000 + i, 1000 + i))
        clock.advance(seconds=1)
    names = [p.name for p in TaskStore(path, clock=clock, tz=timezone.utc).backups()]
    assert len(names) == 5
    assert names[0].endswith("150002") and names[-1].endswith("150006")


@pytest.mark.parametrize("content", ["{broken json", "", "\x00\x01garbage"])
def test_corrupt_file_is_never_overwritten(tmp_path, clock, content):
    path = tmp_path / "tasks.json"
    path.write_text(content + "x", encoding="utf-8")
    before = path.read_bytes()
    store = TaskStore(path, clock=clock, tz=timezone.utc)
    assert store.read_only
    assert "left untouched" in store.load_error
    assert store.today() == []
    for action in (lambda: store.add("new"), store.save, lambda: store.import_taskwarrior([])):
        with pytest.raises(ReadOnlyError):
            action()
    assert path.read_bytes() == before
    assert sorted(p.name for p in tmp_path.iterdir()) == ["tasks.json"]


def test_failed_migration_leaves_file_untouched_and_read_only(tmp_path, clock, monkeypatch):
    path = tmp_path / "tasks.json"
    path.write_text(json.dumps([{"uuid": "u1", "description": "x", "due": "2026-12-01"}]), encoding="utf-8")
    before = path.read_bytes()
    monkeypatch.setattr(TaskStore, "_backup", lambda self, version: (_ for _ in ()).throw(OSError("disk full")))
    store = TaskStore(path, clock=clock, tz=timezone.utc)
    assert store.read_only and "disk full" in store.load_error
    assert [t.description for t in store.all()] == ["x"]  # still shown, from the original data
    assert path.read_bytes() == before


def test_interrupted_write_keeps_previous_file(store, tmp_path, monkeypatch):
    store.add("safe")
    before = store.path.read_bytes()

    def crash(*_args, **_kwargs):
        raise OSError("power loss")

    monkeypatch.setattr(os, "fsync", crash)
    with pytest.raises(OSError, match="power loss"):
        store.add("lost")
    assert store.path.read_bytes() == before
    assert not [p for p in store.path.parent.iterdir() if p.suffix == ".tmp"]


def test_newer_schema_is_loaded_without_migrating(tmp_path, clock):
    path = tmp_path / "tasks.json"
    path.write_text(json.dumps(V1_RECORDS), encoding="utf-8")
    (tmp_path / "tasks.meta.json").write_text('{"schemaVersion": 3, "appVersion": "9.0.0"}')
    store = TaskStore(path, clock=clock, tz=timezone.utc)
    assert not store.read_only and store.backups() == []
    assert len(store.all()) == 3


def test_import_and_export_keep_due_and_priority(store, tmp_path, clock):
    export = [{"uuid": "11111111-1111-1111-1111-111111111111", "description": "from tw", "status": "pending",
               "entry": "20261124T100000Z", "modified": "20261124T100000Z", "due": "20261126T050000Z",
               "priority": "M"}]
    path = tmp_path / "export.json"
    path.write_text(json.dumps(export))
    assert store.import_file(path) == 1
    task = store.get("11111111-1111-1111-1111-111111111111")
    assert task.data["due"] == "20261126T050000Z" and task.priority == "M"
    out = tmp_path / "out.json"
    store.export_file(out)
    assert json.loads(out.read_text())[0]["due"] == "20261126T050000Z"
    assert json.loads(out.read_text())[0]["priority"] == "M"


def test_due_label(store):
    assert store.due_label(store.add("a due:2026-11-23")) == "overdue · Nov 23"
    assert store.due_label(store.add("old due:2025-12-30")) == "overdue · Dec 30, 2025"
    assert store.due_label(store.add("b due:today")) == "today"
    assert store.due_label(store.add("c due:tomorrow")) == "tomorrow"
    assert store.due_label(store.add("d due:2026-11-27")) == "Fri Nov 27"
    assert store.due_label(store.add("e due:2027-01-05")) == "Jan 5, 2027"
    assert store.due_label(store.add("f")) == ""
