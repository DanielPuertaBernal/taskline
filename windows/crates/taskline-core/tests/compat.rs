//! Data compatibility with the files Taskline v1.0, v1.1 and v1.2 (Python) wrote in
//! `%APPDATA%\taskline`. The fixtures were written with Python's `json.dumps` exactly as
//! those versions did (`indent=1`, `ensure_ascii=False` for tasks).

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, NaiveDate, Utc};
use serde_json::{Value, json};
use taskline_core::dates::{Zone, parse_tw};
use taskline_core::settings::Settings;
use taskline_core::store::{Sort, TaskStore};
use taskline_core::theme::Choice;

fn fixture(version: &str) -> (tempfile::TempDir, PathBuf) {
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(version);
    let dir = tempfile::tempdir().unwrap();
    for entry in fs::read_dir(source).unwrap().flatten() {
        fs::copy(entry.path(), dir.path().join(entry.file_name())).unwrap();
    }
    let tasks = dir.path().join("tasks.json");
    (dir, tasks)
}

fn at(stamp: &'static str) -> Box<dyn Fn() -> DateTime<Utc>> {
    Box::new(move || parse_tw(stamp).unwrap())
}

fn open(path: &Path) -> TaskStore {
    TaskStore::open(path, at("20261126T150000Z"), Zone::utc(), "2.0.0")
}

/// The fixture as v1.x wrote it on this platform (CRLF on Windows).
fn as_v1_wrote(text: &str) -> String {
    taskline_core::store::native_newlines(&text.replace("\r\n", "\n"))
}

fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn v1_0_file_keeps_every_task_and_is_backed_up_once() {
    let (dir, path) = fixture("v1.0");
    let before = fs::read(&path).unwrap();
    let original: Value = serde_json::from_slice(&before).unwrap();
    let store = open(&path);
    assert!(!store.read_only);
    let exported = Value::Array(
        store
            .export_taskwarrior()
            .into_iter()
            .map(Value::Object)
            .collect(),
    );
    assert_eq!(exported, original);
    assert_eq!(fs::read(&path).unwrap(), before, "nothing needed rewriting");
    let backups = store.backups();
    assert_eq!(backups.len(), 1);
    assert_eq!(
        backups[0].file_name().unwrap(),
        "tasks.json.bak-1.0.0-20261126-150000"
    );
    assert_eq!(fs::read(&backups[0]).unwrap(), before);
    let meta: Value =
        serde_json::from_str(&fs::read_to_string(dir.path().join("tasks.meta.json")).unwrap()).unwrap();
    assert_eq!(meta, json!({"schemaVersion": 2, "appVersion": "2.0.0"}));
    // Second start: already migrated, no new backup.
    let again = open(&path);
    assert_eq!(again.backups().len(), 1);
    assert_eq!(again.progress(), (0, 1));
}

#[test]
fn v1_1_and_v1_2_files_open_without_migrating() {
    for version in ["v1.1", "v1.2"] {
        let (dir, path) = fixture(version);
        let before = names(dir.path());
        let bytes = fs::read(&path).unwrap();
        let store = open(&path);
        assert!(!store.read_only, "{version}");
        assert!(
            store.backups().is_empty(),
            "{version}: no backup when the schema is current"
        );
        assert_eq!(names(dir.path()), before);
        assert_eq!(fs::read(&path).unwrap(), bytes);
        let rent = store.get("0b6e1e8e-1d1f-4c8a-9a51-1f0c6d1e0a04").unwrap();
        assert_eq!(rent.priority(), Some("H"));
        assert_eq!(
            store.local_date(rent.due().unwrap()),
            NaiveDate::from_ymd_opt(2026, 12, 1).unwrap()
        );
    }
}

#[test]
fn saving_writes_the_same_bytes_as_v1() {
    // Python wrote `json.dumps(tasks, ensure_ascii=False, indent=1)`; unknown fields
    // (annotations, tags, custom numbers) and key order survive a load + save.
    for version in ["v1.1", "v1.2"] {
        let (_dir, path) = fixture(version);
        let bytes = as_v1_wrote(&fs::read_to_string(&path).unwrap());
        let mut store = open(&path);
        store.save().unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), bytes, "{version}");
    }
}

#[test]
fn changes_keep_other_records_and_fields_intact() {
    let (_dir, path) = fixture("v1.2");
    let mut store = open(&path);
    let report = "0b6e1e8e-1d1f-4c8a-9a51-1f0c6d1e0a06";
    store.complete(report).unwrap();
    store.add("New in v2", Some("tomorrow"), Some("L")).unwrap();
    let saved: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    let saved = saved.as_array().unwrap();
    assert_eq!(saved.len(), 7);
    let imported = saved
        .iter()
        .find(|t| t["description"] == "Imported from Taskwarrior")
        .unwrap();
    assert_eq!(imported["annotations"][0]["description"], "note");
    assert_eq!(imported["urgency_custom"], 1.5);
    let done = saved.iter().find(|t| t["uuid"] == report).unwrap();
    let keys: Vec<&str> = done.as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(
        keys,
        [
            "uuid",
            "description",
            "status",
            "entry",
            "modified",
            "due",
            "priority",
            "project",
            "end"
        ]
    );
    assert_eq!(done["status"], "completed");
    assert_eq!(store.progress(), (1, 5));
    assert_eq!(store.today(Sort::Due)[0].description(), "New in v2");
}

#[test]
fn legacy_due_and_priority_values_are_migrated() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tasks.json");
    let records = json!([
        {"uuid": "u1", "description": "iso", "status": "pending", "due": "2026-12-01", "priority": "h", "custom": "kept"},
        {"uuid": "u2", "description": "iso time", "status": "pending", "due": "2026-12-02T08:30:00+00:00"},
        {"uuid": "u3", "description": "junk", "status": "pending", "due": "whenever", "priority": "urgent"}
    ]);
    fs::write(&path, records.to_string()).unwrap();
    let original = fs::read_to_string(&path).unwrap();
    let store = TaskStore::open(&path, at("20261124T150000Z"), Zone::utc(), "");
    assert_eq!(store.get("u1").unwrap().data["due"], "20261201T000000Z");
    assert_eq!(store.get("u1").unwrap().priority(), Some("H"));
    assert_eq!(store.get("u1").unwrap().data["custom"], "kept");
    assert_eq!(store.get("u2").unwrap().data["due"], "20261202T083000Z");
    // Unreadable values are kept verbatim and ignored for sorting.
    assert_eq!(store.get("u3").unwrap().data["due"], "whenever");
    assert_eq!(store.get("u3").unwrap().priority(), None);
    assert_eq!(store.get("u3").unwrap().data["priority"], "urgent");
    assert_eq!(fs::read_to_string(&store.backups()[0]).unwrap(), original);
    let saved: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(saved[0]["due"], "20261201T000000Z");
    let order: Vec<_> = store
        .today(Sort::Due)
        .iter()
        .map(|t| t.description().to_owned())
        .collect();
    assert_eq!(order, ["iso", "iso time", "junk"]);
    let meta: Value =
        serde_json::from_str(&fs::read_to_string(dir.path().join("tasks.meta.json")).unwrap()).unwrap();
    assert_eq!(meta, json!({"schemaVersion": 2, "appVersion": null}));
}

#[test]
fn only_the_last_five_backups_are_kept() {
    let (dir, path) = fixture("v1.0");
    let original = fs::read(&path).unwrap();
    for i in 0..7u64 {
        fs::write(&path, &original).unwrap();
        let _ = fs::remove_file(dir.path().join("tasks.meta.json"));
        let stamp: &'static str = Box::leak(format!("20261124T15000{i}Z").into_boxed_str());
        let store = TaskStore::open(&path, at(stamp), Zone::utc(), "");
        let newest = store
            .backups()
            .into_iter()
            .max_by_key(|p| p.file_name().unwrap().to_owned())
            .unwrap();
        let mtime = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1000 + i);
        fs::File::options()
            .write(true)
            .open(&newest)
            .unwrap()
            .set_modified(mtime)
            .unwrap();
    }
    let backups: Vec<String> = open(&path)
        .backups()
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(backups.len(), 5);
    assert!(
        backups[0].ends_with("150002") && backups[4].ends_with("150006"),
        "{backups:?}"
    );
}

#[test]
fn a_newer_schema_is_loaded_without_migrating() {
    let (dir, path) = fixture("v1.0");
    fs::write(
        dir.path().join("tasks.meta.json"),
        r#"{"schemaVersion": 3, "appVersion": "9.0.0"}"#,
    )
    .unwrap();
    let store = open(&path);
    assert!(!store.read_only && store.backups().is_empty());
    assert_eq!(store.all().len(), 3);
}

#[test]
fn a_failed_migration_leaves_the_file_untouched_and_read_only() {
    let (dir, path) = fixture("v1.0");
    let before = fs::read(&path).unwrap();
    // The meta file cannot be written: a directory sits where it goes.
    fs::create_dir(dir.path().join("tasks.meta.json")).unwrap();
    fs::write(dir.path().join("tasks.meta.json").join("x"), "").unwrap();
    let store = open(&path);
    assert!(store.read_only);
    assert!(store.load_error.as_deref().unwrap().contains("left untouched"));
    assert_eq!(store.all().len(), 3, "still shown, from the original data");
    assert_eq!(fs::read(&path).unwrap(), before);
}

#[test]
fn settings_from_every_version_are_read_and_kept() {
    let (dir, _) = fixture("v1.0");
    let s = Settings::load(dir.path().join("settings.json"));
    assert_eq!(s.position(), Some((812, 6)));
    assert_eq!(
        (s.theme(), s.sort(), s.auto_update(), s.segments()),
        (Choice::System, Sort::Due, true, 10)
    );

    let (dir, _) = fixture("v1.1");
    let s = Settings::load(dir.path().join("settings.json"));
    assert_eq!(s.sort(), Sort::Priority);
    assert!(!s.update_check_due(NaiveDate::from_ymd_opt(2026, 11, 25).unwrap()));

    let (dir, _) = fixture("v1.2");
    let path = dir.path().join("settings.json");
    let bytes = as_v1_wrote(&fs::read_to_string(&path).unwrap());
    let mut s = Settings::load(&path);
    assert_eq!(s.theme(), Choice::Light);
    assert_eq!(s.segments(), 8);
    assert!(!s.auto_update());
    s.save().unwrap();
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        bytes,
        "same bytes as v1.2 wrote"
    );
    assert_eq!(s.take_update_notice().as_deref(), Some("2.0.0"));
    s.set_theme(Choice::Dark);
    s.save().unwrap();
    let saved: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(saved["theme"], "dark");
    assert_eq!(saved["x"], 812);
    assert!(saved.get("update_notice").is_none());
    assert!(!dir.path().join("settings.tmp").exists());
}
