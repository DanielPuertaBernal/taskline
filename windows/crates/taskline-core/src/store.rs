//! Tasks in a `task export` JSON array, plus a sidecar with the schema version.
//!
//! This is the exact file layout of Taskline v1.x for Windows, so an upgrade keeps every
//! task: `tasks.json` stays a plain Taskwarrior export (older builds and Taskwarrior can
//! still read it), `tasks.meta.json` holds `{"schemaVersion": 2, "appVersion": "..."}`.
//! Before a schema upgrade the original is copied to `tasks.json.bak-<old>-<stamp>` (last
//! five kept). A file that cannot be read is never written: the store opens read-only.
//! Unknown fields and their order are kept, so nothing written by Taskwarrior is lost.

use std::cmp::Ordering;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use chrono::{DateTime, NaiveDate, Utc};
use serde_json::{Map, Value, json};

use crate::dates::{Zone, format_date, parse_date, parse_tw};
use crate::input::{InputError, PRIORITIES, parse_due, parse_input, parse_priority};

pub const PENDING: &str = "pending";
pub const COMPLETED: &str = "completed";
pub const DELETED: &str = "deleted";
const STATUSES: [&str; 5] = [PENDING, COMPLETED, DELETED, "waiting", "recurring"];

/// 1 = v1.0.x (no due/priority handling), 2 = v1.1 and later.
pub const SCHEMA_VERSION: i64 = 2;
pub const KEEP_BACKUPS: usize = 5;

pub type Record = Map<String, Value>;
pub type Clock = Box<dyn Fn() -> DateTime<Utc>>;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("{0}")]
    ReadOnly(String),
    #[error("no task with uuid {0}")]
    NotFound(String),
    #[error(transparent)]
    Input(#[from] InputError),
    #[error("{0}")]
    Invalid(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Sort {
    #[default]
    Due,
    Priority,
}

impl Sort {
    pub fn key(self) -> &'static str {
        match self {
            Sort::Due => "due",
            Sort::Priority => "priority",
        }
    }

    /// The saved `sort` setting; anything unknown falls back to due.
    pub fn from_key(key: Option<&str>) -> Sort {
        if key == Some("priority") {
            Sort::Priority
        } else {
            Sort::Due
        }
    }

    pub fn toggled(self) -> Sort {
        match self {
            Sort::Due => Sort::Priority,
            Sort::Priority => Sort::Due,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DueState {
    Overdue,
    Today,
    Upcoming,
}

/// One task record (a copy; change tasks through the store).
#[derive(Debug, Clone, PartialEq)]
pub struct Task {
    pub data: Record,
}

impl Task {
    fn str_field(&self, key: &str) -> Option<&str> {
        self.data.get(key).and_then(Value::as_str)
    }

    pub fn uuid(&self) -> &str {
        self.str_field("uuid").unwrap_or("")
    }

    pub fn description(&self) -> &str {
        self.str_field("description").unwrap_or("")
    }

    pub fn status(&self) -> &str {
        self.str_field("status").unwrap_or(PENDING)
    }

    pub fn project(&self) -> Option<&str> {
        self.str_field("project")
    }

    /// Only the Taskwarrior form counts; anything else is kept but ignored.
    pub fn due(&self) -> Option<DateTime<Utc>> {
        self.str_field("due").and_then(parse_tw)
    }

    pub fn priority(&self) -> Option<&str> {
        self.str_field("priority").filter(|p| PRIORITIES.contains(p))
    }

    pub fn end(&self) -> Option<DateTime<Utc>> {
        self.str_field("end").and_then(parse_date)
    }

    pub fn is_done(&self) -> bool {
        self.status() == COMPLETED
    }

    pub fn is_pending(&self) -> bool {
        self.status() == PENDING
    }
}

/// Writes through a temp file in the same folder, flushes it to disk, then renames it
/// over `path`: an interrupted write leaves the previous file intact.
pub fn atomic_write(path: &Path, text: &str) -> std::io::Result<()> {
    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(dir)?;
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("file");
    let mut tmp = tempfile_in(dir, &format!(".{stem}."), ".tmp")?;
    let result = (|| {
        tmp.1.write_all(text.as_bytes())?;
        tmp.1.flush()?;
        tmp.1.sync_all()?;
        Ok(())
    })();
    drop(tmp.1);
    match result.and_then(|()| fs::rename(&tmp.0, path)) {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = fs::remove_file(&tmp.0);
            tmp.0.clear();
            Err(error)
        }
    }
}

/// A new, exclusively created file named `<prefix><random><suffix>` in `dir`.
pub fn tempfile_in(dir: &Path, prefix: &str, suffix: &str) -> std::io::Result<(PathBuf, fs::File)> {
    for _ in 0..64 {
        let name = format!(
            "{prefix}{}{suffix}",
            &uuid::Uuid::new_v4().simple().to_string()[..10]
        );
        let path = dir.join(name);
        match fs::OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(std::io::Error::other("could not create a temporary file"))
}

/// Text as v1.x wrote it: Python's text-mode files use CRLF on Windows.
pub fn native_newlines(text: &str) -> String {
    if cfg!(windows) {
        text.replace('\n', "\r\n")
    } else {
        text.to_owned()
    }
}

/// JSON with one-space indentation, like Python's `json.dumps(..., indent=1)` in v1.x.
pub fn to_json(value: &Value) -> String {
    let mut out = Vec::new();
    let formatter = serde_json::ser::PrettyFormatter::with_indent(b" ");
    let mut serializer = serde_json::Serializer::with_formatter(&mut out, formatter);
    serde::Serialize::serialize(value, &mut serializer).expect("serializing JSON values cannot fail");
    String::from_utf8(out).expect("serde_json writes UTF-8")
}

/// Normalizes fields written by older builds or by hand; returns whether it changed.
/// ISO `due` becomes the Taskwarrior form and lower-case priorities are upper-cased.
/// Values that cannot be read are left untouched so no data is ever lost.
pub fn migrate_record(record: &mut Record) -> bool {
    let mut changed = false;
    if let Some(Value::String(due)) = record.get("due")
        && !due.is_empty()
        && parse_tw(due).is_none()
        && let Some(parsed) = parse_date(due)
    {
        record.insert("due".into(), Value::String(format_date(parsed)));
        changed = true;
    }
    if let Some(Value::String(priority)) = record.get("priority") {
        let upper = priority.to_uppercase();
        if *priority != upper && PRIORITIES.contains(&upper.as_str()) {
            record.insert("priority".into(), Value::String(upper));
            changed = true;
        }
    }
    changed
}

pub struct TaskStore {
    pub path: PathBuf,
    pub meta_path: PathBuf,
    clock: Clock,
    pub zone: Zone,
    pub app_version: String,
    tasks: Vec<Record>,
    mtime: Option<SystemTime>,
    pub read_only: bool,
    pub load_error: Option<String>,
}

impl TaskStore {
    pub fn open(path: impl Into<PathBuf>, clock: Clock, zone: Zone, app_version: &str) -> Self {
        let path = path.into();
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("tasks")
            .to_owned();
        let mut store = TaskStore {
            meta_path: path.with_file_name(format!("{stem}.meta.json")),
            path,
            clock,
            zone,
            app_version: app_version.to_owned(),
            tasks: Vec::new(),
            mtime: None,
            read_only: false,
            load_error: None,
        };
        store.load();
        store
    }

    /// A store on the system clock and time zone.
    pub fn open_system(path: impl Into<PathBuf>, app_version: &str) -> Self {
        Self::open(path, Box::new(Utc::now), Zone::Local, app_version)
    }

    pub fn now(&self) -> DateTime<Utc> {
        (self.clock)()
    }

    pub fn today_date(&self) -> NaiveDate {
        self.zone.date_of(self.now())
    }

    pub fn local_date(&self, moment: DateTime<Utc>) -> NaiveDate {
        self.zone.date_of(moment)
    }

    // ------------------------------------------------------------------ storage --

    fn read_meta(&self) -> Map<String, Value> {
        fs::read_to_string(&self.meta_path)
            .ok()
            .and_then(|text| serde_json::from_str::<Value>(&text).ok())
            .and_then(|value| match value {
                Value::Object(map) => Some(map),
                _ => None,
            })
            .unwrap_or_default()
    }

    fn write_meta(&self) -> std::io::Result<()> {
        let version = if self.app_version.is_empty() {
            Value::Null
        } else {
            json!(self.app_version)
        };
        let meta = json!({"schemaVersion": SCHEMA_VERSION, "appVersion": version});
        atomic_write(&self.meta_path, &native_newlines(&(to_json(&meta) + "\n")))
    }

    /// Migration backups, oldest first.
    pub fn backups(&self) -> Vec<PathBuf> {
        let Some(dir) = self.path.parent() else {
            return Vec::new();
        };
        let prefix = format!("{}.bak-", self.file_name());
        let mut found: Vec<(SystemTime, String, PathBuf)> = fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|entry| {
                let name = entry.file_name().to_string_lossy().into_owned();
                if !name.starts_with(&prefix) {
                    return None;
                }
                let mtime = entry
                    .metadata()
                    .and_then(|m| m.modified())
                    .unwrap_or(SystemTime::UNIX_EPOCH);
                Some((mtime, name, entry.path()))
            })
            .collect();
        found.sort();
        found.into_iter().map(|(_, _, path)| path).collect()
    }

    fn file_name(&self) -> String {
        self.path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    fn backup(&self, old_version: &str) -> std::io::Result<PathBuf> {
        let stamp = self.zone.naive(self.now()).format("%Y%m%d-%H%M%S").to_string();
        let base = format!("{}.bak-{old_version}-{stamp}", self.file_name());
        let mut backup = self.path.with_file_name(&base);
        let mut n = 1;
        while backup.exists() {
            n += 1;
            backup = self.path.with_file_name(format!("{base}-{n}"));
        }
        fs::copy(&self.path, &backup)?;
        let backups = self.backups();
        if backups.len() > KEEP_BACKUPS {
            for old in &backups[..backups.len() - KEEP_BACKUPS] {
                let _ = fs::remove_file(old);
            }
        }
        Ok(backup)
    }

    fn fail_read_only(&mut self, reason: String) {
        self.tasks.clear();
        self.read_only = true;
        self.load_error = Some(reason);
    }

    pub fn load(&mut self) {
        self.read_only = false;
        self.load_error = None;
        if !self.path.exists() {
            self.tasks.clear();
            self.mtime = None;
            return;
        }
        let shown = self.path.display().to_string();
        let raw = match fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(error) => {
                return self.fail_read_only(format!(
                    "{shown} could not be read ({error}). It was left untouched."
                ));
            }
        };
        let parsed = match std::str::from_utf8(&raw) {
            Ok(text) if text.trim().is_empty() => Ok(Value::Array(Vec::new())),
            Ok(text) => {
                serde_json::from_str::<Value>(text.trim_start_matches('\u{feff}')).map_err(|e| e.to_string())
            }
            Err(error) => Err(error.to_string()),
        };
        let value = match parsed {
            Ok(value) => value,
            Err(error) => {
                return self.fail_read_only(format!(
                    "{shown} could not be read ({error}). It was left untouched."
                ));
            }
        };
        let Value::Array(items) = value else {
            return self.fail_read_only(format!(
                "{shown} is not a Taskwarrior export (expected a JSON array). It was left untouched."
            ));
        };
        let tasks: Vec<Record> = items
            .into_iter()
            .filter_map(|item| match item {
                Value::Object(map) if map.get("uuid").is_some_and(truthy) => Some(map),
                _ => None,
            })
            .collect();
        self.mtime = fs::metadata(&self.path).and_then(|m| m.modified()).ok();
        let meta = self.read_meta();
        let stored = meta.get("schemaVersion").and_then(Value::as_i64).unwrap_or(1);
        if stored >= SCHEMA_VERSION {
            self.tasks = tasks;
            return;
        }
        // Upgrade: back the file up first, migrate a copy, and only then save.
        let old_version = match meta.get("appVersion") {
            Some(Value::String(v)) if !v.is_empty() => v.clone(),
            _ if stored == 1 => "1.0.0".to_owned(),
            _ => stored.to_string(),
        };
        let original = tasks.clone();
        let result = (|| -> Result<(), StoreError> {
            self.backup(&old_version)?;
            let mut migrated = tasks;
            let changed = migrated.iter_mut().map(migrate_record).fold(false, |a, b| a | b);
            self.tasks = migrated;
            if changed {
                self.save()?;
            }
            self.write_meta()?;
            Ok(())
        })();
        if let Err(error) = result {
            self.fail_read_only(format!(
                "Could not upgrade {shown} ({error}). It was left untouched."
            ));
            self.tasks = original;
        }
    }

    /// Reloads when another process changed the file. Returns whether it reloaded.
    pub fn reload_if_changed(&mut self) -> bool {
        let Ok(mtime) = fs::metadata(&self.path).and_then(|m| m.modified()) else {
            return false;
        };
        if Some(mtime) != self.mtime {
            self.load();
            return true;
        }
        false
    }

    fn check_writable(&self) -> Result<(), StoreError> {
        if self.read_only {
            let reason = self
                .load_error
                .clone()
                .unwrap_or_else(|| "the task file is read-only".into());
            return Err(StoreError::ReadOnly(reason));
        }
        Ok(())
    }

    pub fn save(&mut self) -> Result<(), StoreError> {
        self.check_writable()?;
        let first = !self.meta_path.exists();
        let array = Value::Array(self.tasks.iter().cloned().map(Value::Object).collect());
        atomic_write(&self.path, &native_newlines(&to_json(&array)))?;
        self.mtime = fs::metadata(&self.path).and_then(|m| m.modified()).ok();
        if first {
            self.write_meta()?;
        }
        Ok(())
    }

    // ------------------------------------------------------------------ queries --

    pub fn all(&self) -> Vec<Task> {
        self.tasks
            .iter()
            .map(|data| Task { data: data.clone() })
            .collect()
    }

    pub fn get(&self, uuid: &str) -> Result<Task, StoreError> {
        self.position(uuid).map(|i| Task {
            data: self.tasks[i].clone(),
        })
    }

    fn position(&self, uuid: &str) -> Result<usize, StoreError> {
        self.tasks
            .iter()
            .position(|t| t.get("uuid").and_then(Value::as_str) == Some(uuid))
            .ok_or_else(|| StoreError::NotFound(uuid.to_owned()))
    }

    pub fn completed_today(&self, task: &Task) -> bool {
        task.is_done()
            && task
                .end()
                .is_some_and(|end| self.local_date(end) == self.today_date())
    }

    /// Overdue, today or upcoming, for pending tasks with a due date.
    pub fn due_state(&self, task: &Task) -> Option<DueState> {
        let due = task.due()?;
        if !task.is_pending() {
            return None;
        }
        let (day, today) = (self.local_date(due), self.today_date());
        Some(match day.cmp(&today) {
            Ordering::Less => DueState::Overdue,
            Ordering::Equal => DueState::Today,
            Ordering::Greater => DueState::Upcoming,
        })
    }

    /// Pending tasks in `sort` order, then the tasks completed today (latest first).
    /// Due: soonest first, no date last. Priority: H, M, L, none; ties by due date.
    pub fn today(&self, sort: Sort) -> Vec<Task> {
        let tasks = self.all();
        let entry = |t: &Task| t.str_field("entry").unwrap_or("").to_owned();
        let due_key = |t: &Task| (t.due().is_none(), t.due());
        let rank = |t: &Task| {
            t.priority()
                .and_then(|p| PRIORITIES.iter().position(|q| *q == p))
                .unwrap_or(3)
        };
        let mut pending: Vec<Task> = tasks.iter().filter(|t| t.is_pending()).cloned().collect();
        pending.sort_by(|a, b| {
            let first = if sort == Sort::Priority {
                rank(a).cmp(&rank(b))
            } else {
                Ordering::Equal
            };
            first
                .then_with(|| due_key(a).cmp(&due_key(b)))
                .then_with(|| entry(a).cmp(&entry(b)))
        });
        let mut done: Vec<Task> = tasks.into_iter().filter(|t| self.completed_today(t)).collect();
        done.sort_by(|a, b| {
            b.str_field("end")
                .unwrap_or("")
                .cmp(a.str_field("end").unwrap_or(""))
        });
        pending.extend(done);
        pending
    }

    pub fn progress(&self) -> (usize, usize) {
        let tasks = self.today(Sort::Due);
        (tasks.iter().filter(|t| t.is_done()).count(), tasks.len())
    }

    /// Pending tasks per local due day, for the dots in the calendar.
    pub fn due_days(&self) -> std::collections::HashMap<NaiveDate, usize> {
        let mut days = std::collections::HashMap::new();
        for task in self.all() {
            if let (true, Some(due)) = (task.is_pending(), task.due()) {
                *days.entry(self.local_date(due)).or_insert(0) += 1;
            }
        }
        days
    }

    // ----------------------------------------------------------------- commands --

    fn parse(&self, text: &str) -> Result<(String, crate::input::Attrs), StoreError> {
        let (description, attrs) = parse_input(text, self.now(), self.zone)?;
        if description.is_empty() {
            return Err(InputError::EmptyDescription.into());
        }
        Ok((description, attrs))
    }

    /// Adds a task. `due` / `priority` (quick input) override modifiers in `text`.
    pub fn add(&mut self, text: &str, due: Option<&str>, priority: Option<&str>) -> Result<Task, StoreError> {
        self.check_writable()?;
        let (description, mut attrs) = self.parse(text)?;
        if let Some(due) = due.filter(|d| !d.trim().is_empty()) {
            attrs.due = Some(Some(format_date(parse_due(due, self.now(), self.zone)?)));
        }
        if let Some(priority) = priority.filter(|p| !p.trim().is_empty()) {
            attrs.priority = Some(parse_priority(priority)?);
        }
        let now = format_date(self.now());
        let mut record = Record::new();
        record.insert("uuid".into(), json!(uuid::Uuid::new_v4().to_string()));
        record.insert("description".into(), json!(description));
        record.insert("status".into(), json!(PENDING));
        record.insert("entry".into(), json!(now));
        record.insert("modified".into(), json!(now));
        if let Some(project) = attrs.project {
            record.insert("project".into(), json!(project));
        }
        if let Some(Some(due)) = attrs.due {
            record.insert("due".into(), json!(due));
        }
        if let Some(Some(priority)) = attrs.priority {
            record.insert("priority".into(), json!(priority));
        }
        self.tasks.push(record.clone());
        if let Err(error) = self.save() {
            self.tasks.pop();
            return Err(error);
        }
        Ok(Task { data: record })
    }

    fn change(&mut self, uuid: &str, apply: impl FnOnce(&mut Record, &str)) -> Result<Task, StoreError> {
        self.check_writable()?;
        let index = self.position(uuid)?;
        let before = self.tasks[index].clone();
        let now = format_date(self.now());
        apply(&mut self.tasks[index], &now);
        self.tasks[index].insert("modified".into(), json!(now));
        if let Err(error) = self.save() {
            self.tasks[index] = before;
            return Err(error);
        }
        Ok(Task {
            data: self.tasks[index].clone(),
        })
    }

    pub fn complete(&mut self, uuid: &str) -> Result<Task, StoreError> {
        self.change(uuid, |r, now| {
            r.insert("status".into(), json!(COMPLETED));
            r.insert("end".into(), json!(now));
        })
    }

    pub fn reopen(&mut self, uuid: &str) -> Result<Task, StoreError> {
        self.change(uuid, |r, _| {
            r.insert("status".into(), json!(PENDING));
            r.shift_remove("end");
        })
    }

    pub fn toggle(&mut self, uuid: &str) -> Result<Task, StoreError> {
        if self.get(uuid)?.is_done() {
            self.reopen(uuid)
        } else {
            self.complete(uuid)
        }
    }

    /// Marks it deleted like `task delete`; the record stays for export.
    pub fn delete(&mut self, uuid: &str) -> Result<Task, StoreError> {
        self.change(uuid, |r, now| {
            r.insert("status".into(), json!(DELETED));
            r.insert("end".into(), json!(now));
        })
    }

    /// Applies the edit field (see `edit_text`). The project is kept when not mentioned;
    /// due and priority are part of the edit text, so leaving them out clears them.
    pub fn edit(&mut self, uuid: &str, text: &str) -> Result<Task, StoreError> {
        self.check_writable()?;
        let (description, attrs) = self.parse(text)?;
        self.position(uuid)?;
        self.change(uuid, |r, _| {
            r.insert("description".into(), json!(description));
            if let Some(project) = attrs.project {
                r.insert("project".into(), json!(project));
            }
            for (key, value) in [
                ("due", attrs.due.flatten()),
                ("priority", attrs.priority.flatten()),
            ] {
                match value {
                    Some(value) => {
                        r.insert(key.into(), json!(value));
                    }
                    None => {
                        r.shift_remove(key);
                    }
                }
            }
        })
    }

    /// The editable text of a task, which `edit` reads back.
    pub fn edit_text(&self, task: &Task) -> String {
        let mut parts = vec![task.description().to_owned()];
        if let Some(project) = task.project() {
            parts.push(format!("project:{project}"));
        }
        if let Some(due) = task.due() {
            parts.push(format!("due:{}", self.local_date(due).format("%Y-%m-%d")));
        }
        if let Some(priority) = task.priority() {
            parts.push(format!("priority:{priority}"));
        }
        parts.join(" ")
    }

    // ------------------------------------------------------------ import/export --

    pub fn export_taskwarrior(&self) -> Vec<Record> {
        self.tasks.clone()
    }

    /// Merges `task export` records by uuid; the newer `modified` wins.
    pub fn import_taskwarrior(&mut self, records: Vec<Value>) -> Result<usize, StoreError> {
        self.check_writable()?;
        let before = self.tasks.clone();
        let mut count = 0;
        for record in records {
            let Value::Object(record) = record else { continue };
            if !record.get("uuid").is_some_and(truthy) || !record.get("description").is_some_and(truthy) {
                continue;
            }
            let status = record.get("status").and_then(Value::as_str).unwrap_or(PENDING);
            if !STATUSES.contains(&status) {
                continue;
            }
            let mut clean: Record = record
                .into_iter()
                .filter(|(k, _)| k != "id" && k != "urgency")
                .collect();
            let uuid = clean["uuid"].clone();
            let modified = |r: &Record| r.get("modified").and_then(Value::as_str).unwrap_or("").to_owned();
            match self.tasks.iter().position(|t| t.get("uuid") == Some(&uuid)) {
                None => {
                    clean.entry("status").or_insert_with(|| json!(PENDING));
                    let now = format_date(self.now());
                    clean.entry("entry").or_insert_with(|| json!(now));
                    self.tasks.push(clean);
                    count += 1;
                }
                Some(index) if modified(&clean) >= modified(&self.tasks[index]) => {
                    self.tasks[index] = clean;
                    count += 1;
                }
                Some(_) => {}
            }
        }
        if count > 0
            && let Err(error) = self.save()
        {
            self.tasks = before;
            return Err(error);
        }
        Ok(count)
    }

    pub fn import_file(&mut self, path: &Path) -> Result<usize, StoreError> {
        let text = fs::read_to_string(path)?;
        let value: Value = serde_json::from_str(text.trim_start_matches('\u{feff}'))
            .map_err(|e| StoreError::Invalid(format!("not valid JSON: {e}")))?;
        let records = match value {
            Value::Array(items) => items,
            object @ Value::Object(_) => vec![object],
            _ => {
                return Err(StoreError::Invalid(
                    "expected a Taskwarrior export (JSON array)".into(),
                ));
            }
        };
        self.import_taskwarrior(records)
    }

    pub fn export_file(&self, path: &Path) -> Result<(), StoreError> {
        let array = Value::Array(self.tasks.iter().cloned().map(Value::Object).collect());
        fs::write(path, native_newlines(&to_json(&array)))?;
        Ok(())
    }
}

/// Python truthiness for the fields v1.x checked with `t.get("uuid")`.
fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
        Value::Number(n) => n.as_f64() != Some(0.0),
    }
}
