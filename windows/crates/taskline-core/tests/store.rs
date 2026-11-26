//! The task store, ported from the v1.x Python tests (tests/test_model.py).

use std::cell::Cell;
use std::fs;
use std::path::Path;
use std::rc::Rc;

use chrono::{DateTime, Duration, Utc};
use serde_json::{Value, json};
use taskline_core::dates::{Zone, parse_tw};
use taskline_core::store::{DueState, Sort, StoreError, TaskStore};

#[derive(Clone)]
struct Clock(Rc<Cell<DateTime<Utc>>>);

impl Clock {
    fn new() -> Self {
        Clock(Rc::new(Cell::new(parse_tw("20261124T150000Z").unwrap())))
    }
    fn advance(&self, duration: Duration) {
        self.0.set(self.0.get() + duration);
    }
    fn boxed(&self) -> Box<dyn Fn() -> DateTime<Utc>> {
        let cell = self.0.clone();
        Box::new(move || cell.get())
    }
}

fn open(path: &Path, clock: &Clock) -> TaskStore {
    TaskStore::open(path, clock.boxed(), Zone::utc(), "2.0.0")
}

fn store() -> (tempfile::TempDir, Clock, TaskStore) {
    let dir = tempfile::tempdir().unwrap();
    let clock = Clock::new();
    let store = open(&dir.path().join("tasks.json"), &clock);
    (dir, clock, store)
}

fn descriptions(store: &TaskStore, sort: Sort) -> Vec<String> {
    store
        .today(sort)
        .iter()
        .map(|t| t.description().to_owned())
        .collect()
}

#[test]
fn empty_store_has_no_progress() {
    let (_dir, _clock, store) = store();
    assert_eq!(store.progress(), (0, 0));
    assert!(store.today(Sort::Due).is_empty());
}

#[test]
fn add_creates_a_pending_task_and_persists_it() {
    let (dir, clock, mut store) = store();
    let task = store.add("Write report project:Work", None, None).unwrap();
    assert_eq!(task.description(), "Write report");
    assert_eq!(task.project(), Some("Work"));
    assert_eq!(task.status(), "pending");
    assert_eq!(task.data["entry"], "20261124T150000Z");
    let saved: Value =
        serde_json::from_str(&fs::read_to_string(dir.path().join("tasks.json")).unwrap()).unwrap();
    assert_eq!(saved.as_array().unwrap().len(), 1);
    assert_eq!(saved[0]["uuid"], task.uuid());
    let meta: Value =
        serde_json::from_str(&fs::read_to_string(dir.path().join("tasks.meta.json")).unwrap()).unwrap();
    assert_eq!(meta, json!({"schemaVersion": 2, "appVersion": "2.0.0"}));
    let reopened = open(&dir.path().join("tasks.json"), &clock);
    assert_eq!(reopened.all().len(), 1);
}

#[test]
fn empty_descriptions_are_rejected() {
    let (_dir, _clock, mut store) = store();
    assert!(matches!(store.add("   ", None, None), Err(StoreError::Input(_))));
    assert!(matches!(
        store.add("project:Work", None, None),
        Err(StoreError::Input(_))
    ));
    assert!(store.all().is_empty());
}

#[test]
fn completing_counts_towards_today_and_yesterday_does_not() {
    let (_dir, clock, mut store) = store();
    let a = store.add("a", None, None).unwrap();
    store.add("b", None, None).unwrap();
    store.complete(a.uuid()).unwrap();
    assert_eq!(store.progress(), (1, 2));
    assert_eq!(store.get(a.uuid()).unwrap().data["end"], "20261124T150000Z");
    clock.advance(Duration::days(1));
    assert_eq!(store.progress(), (0, 1));
}

#[test]
fn completed_today_uses_the_local_zone() {
    let dir = tempfile::tempdir().unwrap();
    let clock = Clock::new();
    clock.0.set(parse_tw("20261125T030000Z").unwrap()); // 22:00 on the 24th in Bogotá
    let mut store = TaskStore::open(dir.path().join("tasks.json"), clock.boxed(), Zone::hours(-5), "");
    let task = store.add("late", None, None).unwrap();
    store.complete(task.uuid()).unwrap();
    clock.advance(Duration::hours(4)); // 02:00 on the 25th
    assert_eq!(store.progress(), (0, 0));
}

#[test]
fn toggle_reopens_and_delete_keeps_the_record() {
    let (_dir, _clock, mut store) = store();
    let task = store.add("a", None, None).unwrap();
    store.toggle(task.uuid()).unwrap();
    let reopened = store.toggle(task.uuid()).unwrap();
    assert_eq!(reopened.status(), "pending");
    assert!(!reopened.data.contains_key("end"));
    store.delete(task.uuid()).unwrap();
    assert!(store.today(Sort::Due).is_empty());
    assert_eq!(store.export_taskwarrior()[0]["status"], "deleted");
    assert!(matches!(store.toggle("nope"), Err(StoreError::NotFound(_))));
}

#[test]
fn edit_sets_and_clears_due_and_priority_and_keeps_the_project() {
    let (_dir, _clock, mut store) = store();
    let task = store.add("a project:Home", Some("+3d"), Some("h")).unwrap();
    assert_eq!(store.edit_text(&task), "a project:Home due:2026-11-27 priority:H");
    let edited = store.edit(task.uuid(), "b due:2026-12-01").unwrap();
    assert_eq!(edited.description(), "b");
    assert_eq!(edited.project(), Some("Home"));
    assert_eq!(edited.data["due"], "20261201T000000Z");
    assert!(!edited.data.contains_key("priority"));
    let edited = store.edit(task.uuid(), "c priority:L").unwrap();
    assert!(!edited.data.contains_key("due"));
    assert_eq!(edited.priority(), Some("L"));
    assert!(store.edit(task.uuid(), "c due:someday").is_err());
    assert_eq!(
        store.get(task.uuid()).unwrap().description(),
        "c",
        "a failed edit changes nothing"
    );
}

#[test]
fn add_with_explicit_due_and_priority_overrides_the_text() {
    let (_dir, _clock, mut store) = store();
    let task = store
        .add("x due:today priority:L", Some("tomorrow"), Some("M"))
        .unwrap();
    assert_eq!(task.data["due"], "20261125T000000Z");
    assert_eq!(task.priority(), Some("M"));
    assert!(store.add("y", Some("someday"), None).is_err());
    assert!(store.add("y", None, Some("Z")).is_err());
    assert_eq!(store.all().len(), 1);
}

#[test]
fn due_states() {
    let (_dir, _clock, mut store) = store();
    let overdue = store.add("o due:2026-11-23", None, None).unwrap();
    let today = store.add("t due:today", None, None).unwrap();
    let later = store.add("u due:+2d", None, None).unwrap();
    let none = store.add("n", None, None).unwrap();
    assert_eq!(store.due_state(&overdue), Some(DueState::Overdue));
    assert_eq!(store.due_state(&today), Some(DueState::Today));
    assert_eq!(store.due_state(&later), Some(DueState::Upcoming));
    assert_eq!(store.due_state(&none), None);
    let done = store.complete(overdue.uuid()).unwrap();
    assert_eq!(store.due_state(&done), None);
}

#[test]
fn sorting_by_due_and_by_priority() {
    let (_dir, clock, mut store) = store();
    for (text, due, priority) in [
        ("no date low", None, Some("L")),
        ("later high", Some("+5d"), Some("H")),
        ("soon none", Some("+1d"), None),
        ("today medium", Some("today"), Some("M")),
        ("no date high", None, Some("H")),
    ] {
        store.add(text, due, priority).unwrap();
        clock.advance(Duration::seconds(1));
    }
    assert_eq!(
        descriptions(&store, Sort::Due),
        [
            "today medium",
            "soon none",
            "later high",
            "no date low",
            "no date high"
        ]
    );
    assert_eq!(
        descriptions(&store, Sort::Priority),
        [
            "later high",
            "no date high",
            "today medium",
            "no date low",
            "soon none"
        ]
    );
}

#[test]
fn completed_tasks_follow_pending_latest_first() {
    let (_dir, clock, mut store) = store();
    let a = store.add("a", None, None).unwrap();
    let b = store.add("b", None, None).unwrap();
    store.add("c", None, None).unwrap();
    store.complete(a.uuid()).unwrap();
    clock.advance(Duration::minutes(1));
    store.complete(b.uuid()).unwrap();
    assert_eq!(descriptions(&store, Sort::Due), ["c", "b", "a"]);
}

#[test]
fn import_merges_by_uuid_and_newer_modified_wins() {
    let (dir, _clock, mut store) = store();
    let existing = store.add("mine", None, None).unwrap();
    let export = json!([
        {"id": 3, "uuid": existing.uuid(), "description": "older", "status": "pending", "modified": "20200101T000000Z"},
        {"id": 4, "uuid": "11111111-1111-1111-1111-111111111111", "description": "from tw", "status": "pending",
         "entry": "20261124T100000Z", "modified": "20261124T100000Z", "due": "20261126T050000Z",
         "priority": "M", "urgency": 4.2},
        {"uuid": "22222222-2222-2222-2222-222222222222", "description": "bad", "status": "weird"},
        {"uuid": "", "description": "no uuid"},
        "not an object"
    ]);
    let path = dir.path().join("export.json");
    fs::write(&path, export.to_string()).unwrap();
    assert_eq!(store.import_file(&path).unwrap(), 1);
    assert_eq!(store.get(existing.uuid()).unwrap().description(), "mine");
    let imported = store.get("11111111-1111-1111-1111-111111111111").unwrap();
    assert_eq!(imported.priority(), Some("M"));
    assert!(!imported.data.contains_key("id") && !imported.data.contains_key("urgency"));
    let newer = json!([{"uuid": existing.uuid(), "description": "theirs", "status": "pending",
                        "modified": "20991231T000000Z"}]);
    assert_eq!(
        store
            .import_taskwarrior(newer.as_array().unwrap().clone())
            .unwrap(),
        1
    );
    assert_eq!(store.get(existing.uuid()).unwrap().description(), "theirs");

    let out = dir.path().join("out.json");
    store.export_file(&out).unwrap();
    let exported: Value = serde_json::from_str(&fs::read_to_string(&out).unwrap()).unwrap();
    assert_eq!(exported.as_array().unwrap().len(), 2);
    assert_eq!(exported[1]["due"], "20261126T050000Z");
    fs::write(
        &path,
        "{\"uuid\": \"33333333-3333-3333-3333-333333333333\", \"description\": \"one\"}",
    )
    .unwrap();
    assert_eq!(
        store.import_file(&path).unwrap(),
        1,
        "a single object is accepted"
    );
    fs::write(&path, "42").unwrap();
    assert!(matches!(store.import_file(&path), Err(StoreError::Invalid(_))));
}

#[test]
fn reload_picks_up_external_edits() {
    let (dir, clock, mut store) = store();
    store.add("a", None, None).unwrap();
    assert!(!store.reload_if_changed());
    std::thread::sleep(std::time::Duration::from_millis(20));
    let mut other = open(&dir.path().join("tasks.json"), &clock);
    other.add("b", None, None).unwrap();
    // Some file systems keep coarse mtimes; force a different one.
    let later = std::time::SystemTime::now() + std::time::Duration::from_secs(5);
    fs::File::options()
        .write(true)
        .open(dir.path().join("tasks.json"))
        .unwrap()
        .set_modified(later)
        .unwrap();
    assert!(store.reload_if_changed());
    assert_eq!(store.all().len(), 2);
}

#[test]
fn corrupt_files_are_never_overwritten() {
    for content in ["{broken json", "\u{0}\u{1}garbage", "{\"not\": \"an array\"}"] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tasks.json");
        fs::write(&path, format!("{content}x")).unwrap();
        let before = fs::read(&path).unwrap();
        let mut store = open(&path, &Clock::new());
        assert!(store.read_only, "{content}");
        assert!(store.load_error.as_deref().unwrap().contains("left untouched"));
        assert!(store.today(Sort::Due).is_empty());
        assert!(matches!(
            store.add("new", None, None),
            Err(StoreError::ReadOnly(_))
        ));
        assert!(matches!(store.save(), Err(StoreError::ReadOnly(_))));
        assert!(matches!(
            store.import_taskwarrior(vec![]),
            Err(StoreError::ReadOnly(_))
        ));
        assert_eq!(fs::read(&path).unwrap(), before);
        assert_eq!(
            fs::read_dir(dir.path()).unwrap().count(),
            1,
            "no backup, meta or temp file"
        );
    }
}

#[test]
fn empty_file_is_an_empty_list() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("tasks.json"), "").unwrap();
    let store = open(&dir.path().join("tasks.json"), &Clock::new());
    assert!(!store.read_only);
    assert!(store.all().is_empty());
}

#[test]
fn failed_save_keeps_the_previous_file_and_state() {
    let (dir, _clock, mut store) = store();
    store.add("safe", None, None).unwrap();
    let before = fs::read(dir.path().join("tasks.json")).unwrap();
    // Make the rename fail: the target becomes a non-empty directory.
    let path = dir.path().join("tasks.json");
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    fs::write(path.join("keep"), "x").unwrap();
    assert!(store.add("lost", None, None).is_err());
    assert_eq!(store.all().len(), 1, "the failed add is rolled back in memory");
    fs::remove_dir_all(&path).unwrap();
    fs::write(&path, &before).unwrap();
    let leftovers: Vec<_> = fs::read_dir(dir.path())
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
        .collect();
    assert!(leftovers.is_empty(), "temp files are cleaned up");
}
