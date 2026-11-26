//! `%APPDATA%\taskline\settings.json`, shared with Taskline v1.x.
//!
//! Known keys: `theme` (light/dark/system), `sort` (due/priority), `segments`, `x`/`y`
//! (bar position), `auto_update`, `last_update_check` (YYYY-MM-DD) and `update_notice`
//! (the version an update installed, shown once by the new build). Unknown keys are
//! kept as they are.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::NaiveDate;
use serde_json::{Map, Value, json};

use crate::store::{Sort, native_newlines, to_json};
use crate::theme::Choice;

/// `TASKLINE_DATA_DIR`, else `%APPDATA%\taskline`, else `~/.local/share/taskline`.
pub fn data_dir_from(
    override_dir: Option<String>,
    appdata: Option<String>,
    home: Option<PathBuf>,
) -> PathBuf {
    if let Some(dir) = override_dir.filter(|d| !d.is_empty()) {
        return PathBuf::from(dir);
    }
    if let Some(appdata) = appdata.filter(|d| !d.is_empty()) {
        return PathBuf::from(appdata).join("taskline");
    }
    home.unwrap_or_default()
        .join(".local")
        .join("share")
        .join("taskline")
}

pub fn data_dir() -> PathBuf {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from);
    data_dir_from(
        std::env::var("TASKLINE_DATA_DIR").ok(),
        std::env::var("APPDATA").ok(),
        home,
    )
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Settings {
    pub path: PathBuf,
    pub values: Map<String, Value>,
}

impl Settings {
    pub fn load(path: impl Into<PathBuf>) -> Settings {
        let path = path.into();
        let values = fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str::<Value>(text.trim_start_matches('\u{feff}')).ok())
            .and_then(|value| match value {
                Value::Object(map) => Some(map),
                _ => None,
            })
            .unwrap_or_default();
        Settings { path, values }
    }

    /// Writes `settings.tmp` and renames it over the file, as v1.x did.
    pub fn save(&self) -> std::io::Result<()> {
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }
        let tmp = self.path.with_extension("tmp");
        fs::write(
            &tmp,
            native_newlines(&to_json(&Value::Object(self.values.clone()))),
        )?;
        fs::rename(&tmp, &self.path)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn theme(&self) -> Choice {
        Choice::from_key(self.values.get("theme").and_then(Value::as_str))
    }

    pub fn set_theme(&mut self, choice: Choice) {
        self.values.insert("theme".into(), json!(choice.key()));
    }

    pub fn sort(&self) -> Sort {
        Sort::from_key(self.values.get("sort").and_then(Value::as_str))
    }

    pub fn set_sort(&mut self, sort: Sort) {
        self.values.insert("sort".into(), json!(sort.key()));
    }

    pub fn segments(&self) -> usize {
        self.values
            .get("segments")
            .and_then(Value::as_u64)
            .map(|s| s.clamp(1, 40) as usize)
            .unwrap_or(10)
    }

    /// The saved bar position (integers only, like v1.x).
    pub fn position(&self) -> Option<(i32, i32)> {
        let x = self.values.get("x").and_then(Value::as_i64)?;
        let y = self.values.get("y").and_then(Value::as_i64)?;
        Some((x as i32, y as i32))
    }

    pub fn set_position(&mut self, position: Option<(i32, i32)>) {
        match position {
            Some((x, y)) => {
                self.values.insert("x".into(), json!(x));
                self.values.insert("y".into(), json!(y));
            }
            None => {
                self.values.shift_remove("x");
                self.values.shift_remove("y");
            }
        }
    }

    /// On unless the value is exactly `false`.
    pub fn auto_update(&self) -> bool {
        self.values.get("auto_update") != Some(&Value::Bool(false))
    }

    pub fn set_auto_update(&mut self, enabled: bool) {
        self.values.insert("auto_update".into(), json!(enabled));
    }

    /// Automatic updates are on and nothing was checked today.
    pub fn update_check_due(&self, today: NaiveDate) -> bool {
        let last = self.values.get("last_update_check").and_then(Value::as_str);
        self.auto_update() && last != Some(today.format("%Y-%m-%d").to_string().as_str())
    }

    pub fn mark_update_checked(&mut self, today: NaiveDate) {
        self.values.insert(
            "last_update_check".into(),
            json!(today.format("%Y-%m-%d").to_string()),
        );
    }

    pub fn set_update_notice(&mut self, version: &str) {
        self.values.insert("update_notice".into(), json!(version));
    }

    /// Removes and returns `update_notice`.
    pub fn take_update_notice(&mut self) -> Option<String> {
        match self.values.shift_remove("update_notice") {
            Some(Value::String(version)) => Some(version),
            Some(_) => Some(String::new()),
            None => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_dir_order() {
        let home = Some(PathBuf::from("/home/me"));
        assert_eq!(
            data_dir_from(Some("/tmp/x".into()), Some("C:/A".into()), home.clone()),
            PathBuf::from("/tmp/x")
        );
        assert_eq!(
            data_dir_from(None, Some("C:/A".into()), home.clone()),
            PathBuf::from("C:/A").join("taskline")
        );
        assert_eq!(
            data_dir_from(Some(String::new()), None, home),
            PathBuf::from("/home/me/.local/share/taskline")
        );
    }

    #[test]
    fn auto_update_defaults_on_and_checks_once_a_day() {
        let today = NaiveDate::from_ymd_opt(2026, 11, 26).unwrap();
        let mut s = Settings::default();
        assert!(s.auto_update() && s.update_check_due(today));
        s.mark_update_checked(today);
        assert!(!s.update_check_due(today));
        assert!(s.update_check_due(today.succ_opt().unwrap()));
        s.set_auto_update(false);
        assert!(!s.update_check_due(today.succ_opt().unwrap()));
        s.values.insert("auto_update".into(), json!(0));
        assert!(s.auto_update(), "only false turns it off, like v1.x");
    }
}
