//! `taskline.exe --selftest REPORT [--screenshots DIR]`: drives the real app with demo
//! tasks in a temporary data folder, renders the bar and the panel in real windows,
//! captures both themes with the calendar open, and exercises the updater against a
//! local fake release. CI runs it on the built exe; nothing here touches real tasks.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::Duration as Days;
use iced::Task;
use iced::keyboard::{Key, key::Named};
use iced::window::{self, Screenshot};
use serde_json::Value;
use taskline_core::store::{Sort, TaskStore};
use taskline_core::theme::{Choice, Mode};
use taskline_core::updater;

use crate::app::{App, Message, Popover, Target, sleep};
use crate::platform;

pub static EXIT_CODE: AtomicI32 = AtomicI32::new(0);

pub struct Selftest {
    pub report: PathBuf,
    pub screenshots: Option<PathBuf>,
    pub data: PathBuf,
    lines: Vec<String>,
    failed: bool,
}

impl Selftest {
    #[cfg(test)]
    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    #[cfg(test)]
    pub fn failures(&self) -> Vec<&str> {
        self.lines
            .iter()
            .filter(|l| l.starts_with("FAILED"))
            .map(String::as_str)
            .collect()
    }

    pub fn new(report: PathBuf, screenshots: Option<PathBuf>, data: PathBuf) -> Selftest {
        Selftest {
            report,
            screenshots,
            data,
            lines: vec![format!("taskline {}", taskline_core::VERSION)],
            failed: false,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Step {
    Start,
    Calendar,
    Shot(&'static str),
    Saved(&'static str, Screenshot),
    Locate(Mode),
    LocateClicks,
    Click(isize, usize),
    Press(isize, usize),
    ClickCheck(isize, usize),
    Hover(Mode, isize, u8),
    Grabbed(Mode, isize),
    Finish,
}

/// Demo tasks only, never anybody's real list.
pub fn demo_store(directory: &Path) -> TaskStore {
    let mut store = TaskStore::open_system(directory.join("tasks.json"), taskline_core::VERSION);
    let review = store
        .add("Review pull requests project:Work", None, Some("H"))
        .expect("demo");
    store
        .add("Send the invoice project:Work", Some("2020-01-01"), Some("H"))
        .expect("demo");
    store
        .add("Write the weekly report project:Work", Some("today"), Some("M"))
        .expect("demo");
    store
        .add("Buy groceries project:Home", Some("tomorrow"), Some("L"))
        .expect("demo");
    store.add("Book the dentist", Some("+3d"), None).expect("demo");
    store
        .add("30 minutes of reading", Some("+5d"), Some("L"))
        .expect("demo");
    store.complete(review.uuid()).expect("demo");
    store
}

fn check(app: &mut App, ok: bool, what: impl Into<String>) {
    if !ok && let Some(test) = app.selftest.as_mut() {
        test.failed = true;
        test.lines.push(format!("FAILED: {}", what.into()));
    }
}

fn line(app: &mut App, text: impl Into<String>) {
    if let Some(test) = app.selftest.as_mut() {
        test.lines.push(text.into());
    }
}

fn pending_order(app: &App) -> Vec<String> {
    app.store
        .today(app.sort)
        .iter()
        .filter(|t| !t.is_done())
        .map(|t| t.description().to_owned())
        .collect()
}

pub fn step(app: &mut App, step: Step) -> Task<Message> {
    match step {
        Step::Start => {
            let open = app.update(Message::TogglePanel);
            logic(app);
            Task::batch([open, after(Duration::from_millis(1500), Step::LocateClicks)])
        }
        Step::Calendar => {
            app.hovered = None;
            let _ = app.update(Message::OpenCalendar(Target::New));
            after(Duration::from_millis(1200), Step::Shot("windows-panel-dark.png"))
        }
        Step::Shot(name) => {
            let id = if name == "windows-bar.png" {
                app.bar
            } else {
                app.panel
            };
            match id {
                Some(id) => {
                    window::screenshot(id).map(move |shot| Message::Selftest(Step::Saved(name, shot)))
                }
                None => {
                    check(app, false, format!("{name}: window missing"));
                    Task::done(Message::Selftest(Step::Finish))
                }
            }
        }
        Step::Saved(name, shot) => {
            let colors = distinct_colors(&shot);
            check(
                app,
                colors > 12,
                format!("{name}: the window rendered ({colors} colors)"),
            );
            let saved = app
                .selftest
                .as_ref()
                .and_then(|t| t.screenshots.clone())
                .map(|dir| save_png(&dir, name, &shot));
            match saved {
                Some(Err(error)) => check(app, false, format!("{name}: {error}")),
                Some(Ok(())) => line(
                    app,
                    format!("screenshot: {name} {}x{}", shot.size.width, shot.size.height),
                ),
                None => line(
                    app,
                    format!("rendered: {name} {}x{}", shot.size.width, shot.size.height),
                ),
            }
            match name {
                "windows-panel-dark.png" => Task::done(Message::Selftest(Step::Locate(Mode::Dark))),
                "windows-panel-light.png" => Task::done(Message::Selftest(Step::Locate(Mode::Light))),
                _ => Task::done(Message::Selftest(Step::Finish)),
            }
        }
        Step::LocateClicks => match app.panel {
            Some(id) => {
                window::run(id, |w| crate::raw_hwnd(w)).map(|hwnd| Message::Selftest(Step::Click(hwnd, 0)))
            }
            None => Task::done(Message::Selftest(Step::Calendar)),
        },
        Step::Click(hwnd, i) => {
            let Some((left, top, scale)) = platform::window_origin(hwnd).filter(|_| i < CLICKS.len()) else {
                if i == 0 {
                    line(app, "clicks: real mouse input unavailable on this system");
                } else {
                    line(
                        app,
                        format!(
                            "clicks: {} real clicks checked against the popover rules",
                            CLICKS.len()
                        ),
                    );
                }
                return Task::done(Message::Selftest(Step::Calendar));
            };
            if i == 0 {
                app.selftest_progress = app.store.progress();
            }
            let (target, _, _) = CLICKS[i];
            let point = click_point(app, target);
            platform::move_cursor(left + (point.x * scale) as i32, top + (point.y * scale) as i32);
            after(Duration::from_millis(250), Step::Press(hwnd, i))
        }
        Step::Press(hwnd, i) => {
            platform::click();
            after(Duration::from_millis(400), Step::ClickCheck(hwnd, i))
        }
        Step::ClickCheck(hwnd, i) => {
            let (target, expected, why) = CLICKS[i];
            let got = match app.popover {
                Popover::None => "none",
                Popover::Calendar { .. } => "calendar",
                Popover::Priority { .. } => "priority",
            };
            check(
                app,
                got == expected,
                format!("click {i} on {target}: expected {expected}, got {got} ({why})"),
            );
            let progress = app.store.progress();
            let before = app.selftest_progress;
            check(
                app,
                progress == before,
                format!("click {i} on {target} changed a task ({progress:?})"),
            );
            Task::done(Message::Selftest(Step::Click(hwnd, i + 1)))
        }
        Step::Locate(mode) => match app.panel {
            Some(id) => window::run(id, |w| crate::raw_hwnd(w))
                .map(move |hwnd| Message::Selftest(Step::Hover(mode, hwnd, 0))),
            None => Task::done(Message::Selftest(Step::Grabbed(mode, 0))),
        },
        Step::Hover(mode, hwnd, i) => {
            // Move the real pointer over the calendar's days: every move repaints only
            // the damaged cells, which is where v2.0.0 stacked its shadow into black.
            let Some((left, top, scale)) = platform::window_origin(hwnd) else {
                return Task::done(Message::Selftest(Step::Grabbed(mode, hwnd)));
            };
            let (x, y) = crate::panel::new_calendar_origin(app.panel_size.width);
            let (column, row) = (f32::from(i % 7), f32::from(i / 7 % 4));
            let px = x + 12.0 + 19.0 + column * 39.0;
            let py = y + 70.0 + 17.0 + row * 36.0;
            platform::move_cursor(left + (px * scale) as i32, top + (py * scale) as i32);
            if i < 20 {
                after(Duration::from_millis(120), Step::Hover(mode, hwnd, i + 1))
            } else {
                after(Duration::from_millis(600), Step::Grabbed(mode, hwnd))
            }
        }
        Step::Grabbed(mode, hwnd) => {
            screen_check(app, mode, hwnd);
            match mode {
                Mode::Dark => {
                    // The calendar stays open; only the palette changes.
                    let _ = app.update(Message::SetTheme(Choice::Light));
                    app.hovered = None;
                    after(Duration::from_millis(1200), Step::Shot("windows-panel-light.png"))
                }
                Mode::Light => {
                    let _ = app.update(Message::SetTheme(Choice::Dark));
                    let _ = app.update(Message::ClosePopover);
                    after(Duration::from_millis(800), Step::Shot("windows-bar.png"))
                }
            }
        }
        Step::Finish => {
            updater_check(app);
            registry_check(app);
            finish(app)
        }
    }
}

/// What the screen shows inside the panel after hovering the calendar: its padding,
/// header and weekday row must still be the theme's background, not shadow or black.
fn screen_check(app: &mut App, mode: Mode, hwnd: isize) {
    let name = match mode {
        Mode::Dark => "windows-calendar-dark-screen.png",
        Mode::Light => "windows-calendar-light-screen.png",
    };
    let (Some((width, height, rgba)), Some((_, _, scale))) =
        (platform::capture_window(hwnd), platform::window_origin(hwnd))
    else {
        return line(app, format!("{name}: screen capture unavailable on this system"));
    };
    let shot = Screenshot::new(rgba, iced::Size::new(width, height), scale);
    if let Some(dir) = app.selftest.as_ref().and_then(|t| t.screenshots.clone())
        && let Err(error) = save_png(&dir, name, &shot)
    {
        check(app, false, format!("{name}: {error}"));
    }
    let bg = mode.palette().bg;
    let (x, y) = crate::panel::new_calendar_origin(app.panel_size.width);
    let w = crate::panel::CAL_W;
    // Padding left of the grid, right of the header, between header and weekdays.
    let samples = [
        (x + 5.0, y + 60.0),
        (x + w - 5.0, y + 30.0),
        (x + w / 2.0 - 60.0, y + 6.0),
        (x + 5.0, y + 200.0),
    ];
    let mut bad = Vec::new();
    for (sx, sy) in samples {
        let (px, py) = ((sx * scale) as u32, (sy * scale) as u32);
        if px >= width || py >= height {
            continue;
        }
        let at = ((py * width + px) * 4) as usize;
        let pixel = &shot.rgba[at..at + 3];
        let off = [bg.0, bg.1, bg.2]
            .iter()
            .zip(pixel)
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap_or(0);
        if off > 12 {
            bad.push(format!(
                "({sx:.0},{sy:.0}) = #{:02x}{:02x}{:02x}",
                pixel[0], pixel[1], pixel[2]
            ));
        }
    }
    check(
        app,
        bad.is_empty(),
        format!(
            "{name}: calendar background after hovering is not {} at {}",
            bg.to_hex(),
            bad.join(", ")
        ),
    );
    line(
        app,
        format!(
            "screen: {name} {width}x{height}, calendar background intact after hover: {}",
            bad.is_empty()
        ),
    );
}

/// Real clicks on the panel: (where, popover expected afterwards, rule).
const CLICKS: [(&str, &str, &str); 11] = [
    ("date", "calendar", "the date field opens the calendar"),
    ("list", "none", "a click elsewhere closes it"),
    ("date", "calendar", "reopens"),
    ("date", "none", "the date field toggles it closed"),
    ("priority", "priority", "the priority button opens its menu"),
    ("date", "calendar", "another trigger opens its own popover"),
    (
        "checkbox",
        "none",
        "closes without completing the task underneath",
    ),
    ("date", "calendar", "reopens"),
    ("input", "none", "the new-task field only closes it"),
    ("input", "none", "and never opens the calendar"),
    ("list", "none", "nor does an empty area"),
];

fn click_point(app: &App, target: &str) -> iced::Point {
    use crate::app::Target;
    use crate::panel::Trigger;
    let center = |trigger: Trigger| {
        crate::panel::triggers(app)
            .into_iter()
            .find(|(t, _)| *t == trigger)
            .map(|(_, r)| r.center())
            .unwrap_or_default()
    };
    match target {
        "date" => center(Trigger::Calendar(Target::New)),
        "priority" => center(Trigger::Priority(Target::New)),
        "checkbox" => iced::Point::new(307.0, 91.0),
        "input" => iced::Point::new(400.0, 42.0),
        _ => iced::Point::new(520.0, app.panel_size.height - 60.0),
    }
}

fn after(delay: Duration, step: Step) -> Task<Message> {
    Task::perform(sleep(delay), move |()| Message::Selftest(step.clone()))
}

/// The same checks v1.2's selftest made, through the app's own messages.
pub(crate) fn logic(app: &mut App) {
    let progress = app.store.progress();
    check(app, progress == (1, 6), format!("demo progress {progress:?}"));
    line(app, format!("bar: {}", app.bar_text()));

    for message in [
        Message::Input("Added by selftest".into()),
        Message::Due("+2d".into()),
        Message::OpenPriority(Target::New),
        Message::PickPriority(Some("H")),
        Message::Submit,
    ] {
        let _ = app.update(message);
    }
    let progress = app.store.progress();
    check(app, progress == (1, 7), format!("after add {progress:?}"));
    let saved: Value = std::fs::read_to_string(&app.store.path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or(Value::Null);
    let added = saved
        .as_array()
        .and_then(|a| a.iter().find(|t| t["description"] == "Added by selftest"))
        .cloned();
    check(
        app,
        added
            .as_ref()
            .is_some_and(|t| t["priority"] == "H" && t["due"].is_string()),
        "the added task has priority H and a due date",
    );
    check(
        app,
        app.input.is_empty() && app.due.is_empty() && app.new_priority.is_none(),
        "fields cleared",
    );
    let order = pending_order(app);
    check(
        app,
        order.first().map(String::as_str) == Some("Send the invoice"),
        format!("due order {order:?}"),
    );
    let _ = app.update(Message::ToggleSort);
    let order = pending_order(app);
    check(
        app,
        app.sort == Sort::Priority && order[..2] == ["Send the invoice", "Added by selftest"],
        format!("priority order {order:?}"),
    );
    let saved = taskline_core::settings::Settings::load(app.settings.path()).sort();
    check(app, saved == Sort::Priority, "sort saved in settings.json");
    let _ = app.update(Message::ToggleSort);
    line(app, "due/priority/sort: OK");

    // Calendar: open from the due field, move with the keyboard, Enter picks.
    let today = app.store.today_date();
    let _ = app.update(Message::Input("Picked in the calendar".into()));
    let _ = app.update(Message::OpenCalendar(Target::New));
    check(
        app,
        matches!(app.popover, Popover::Calendar { cursor, .. } if cursor == today),
        "calendar opens on today",
    );
    let _ = app.update(Message::Key(Key::Named(Named::ArrowRight), Default::default()));
    let _ = app.update(Message::Key(Key::Named(Named::ArrowDown), Default::default()));
    check(
        app,
        matches!(app.popover, Popover::Calendar { cursor, .. } if cursor == today + Days::days(8)),
        "arrows move the cursor",
    );
    let _ = app.update(Message::DueSubmit);
    let picked = today + Days::days(8);
    let shown = app.texts.short_date(picked, today);
    check(
        app,
        app.popover == Popover::None && app.due_day == Some(picked) && app.due == shown,
        format!(
            "Enter picks the day and the field shows it as `{shown}` ({})",
            app.due
        ),
    );
    let _ = app.update(Message::OpenPriority(Target::New));
    let _ = app.update(Message::PickPriority(Some("M")));
    let _ = app.update(Message::Submit);
    let added = app
        .store
        .all()
        .into_iter()
        .find(|t| t.description() == "Picked in the calendar");
    check(
        app,
        added.as_ref().is_some_and(|t| {
            t.due().map(|d| app.store.local_date(d)) == Some(today + Days::days(8))
                && t.priority() == Some("M")
        }),
        "calendar task saved with its day and priority",
    );
    let _ = app.update(Message::OpenCalendar(Target::New));
    let _ = app.update(Message::Due("+3d".into()));
    check(
        app,
        matches!(app.popover, Popover::Calendar { cursor, .. } if cursor == today + Days::days(3)),
        "typed shortcuts move the calendar",
    );
    let _ = app.update(Message::ClosePopover);
    let _ = app.update(Message::Due(String::new()));

    // Editing: the calendar and the priority menu rewrite the words of the edit field.
    if let Some(task) = added {
        let uuid = task.uuid().to_owned();
        let _ = app.update(Message::StartEdit(uuid.clone()));
        let _ = app.update(Message::OpenCalendar(Target::Edit));
        let _ = app.update(Message::ChipDays(Some(1)));
        let _ = app.update(Message::OpenPriority(Target::Edit));
        let _ = app.update(Message::PickPriority(Some("L")));
        let expected = format!(
            "Picked in the calendar due:{} priority:L",
            (today + Days::days(1)).format("%Y-%m-%d")
        );
        check(
            app,
            app.edit_text == expected,
            format!("edit text {}", app.edit_text),
        );
        let _ = app.update(Message::SaveEdit);
        let edited = app.store.get(&uuid).ok();
        check(
            app,
            edited.is_some_and(|t| t.priority() == Some("L")),
            "edit saved",
        );
        let _ = app.update(Message::Delete(uuid));
    }
    for task in app.store.all() {
        if task.description() == "Added by selftest" {
            let _ = app.store.delete(task.uuid());
        }
    }
    line(
        app,
        "calendar: keyboard pick, chips, typed shortcut, edit and priority icons OK",
    );

    // Theme: the switch flips light/dark and is saved; Auto follows the system.
    let _ = app.update(Message::SetTheme(Choice::Light));
    let light = app.mode() == Mode::Light && app.settings.theme() == Choice::Light;
    let _ = app.update(Message::FlipTheme);
    let dark = app.mode() == Mode::Dark && app.settings.theme() == Choice::Dark;
    check(app, light && dark, "theme switch");
    line(app, "theme: light/dark switch saved in settings.json");
    let progress = app.store.progress();
    check(app, progress == (1, 6), format!("demo restored {progress:?}"));
}

/// Serves a fake v99.0.0 release on 127.0.0.1 and runs the real HTTP updater against it.
fn updater_check(app: &mut App) {
    let payload = b"MZ selftest build".to_vec();
    let exe = Arc::new(Mutex::new(payload.clone()));
    let Ok(listener) = TcpListener::bind("127.0.0.1:0") else {
        return check(app, false, "updater: could not listen on 127.0.0.1");
    };
    let base = format!("http://{}", listener.local_addr().expect("bound address"));
    let sums = format!("{}  taskline.exe\n", updater::sha256_hex(&payload));
    let api = format!(
        r#"{{"tag_name": "v99.0.0", "assets": [
            {{"name": "taskline.exe", "browser_download_url": "{base}/taskline.exe"}},
            {{"name": "SHA256SUMS.txt", "browser_download_url": "{base}/SHA256SUMS.txt"}}]}}"#
    );
    let served = exe.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut stream = stream;
            let mut buffer = [0u8; 2048];
            let n = stream.read(&mut buffer).unwrap_or(0);
            let request = String::from_utf8_lossy(&buffer[..n]);
            let path = request.split_whitespace().nth(1).unwrap_or("/").to_owned();
            let body = match path.as_str() {
                "/api" => Some(api.clone().into_bytes()),
                "/taskline.exe" => Some(served.lock().expect("exe").clone()),
                "/SHA256SUMS.txt" => Some(sums.clone().into_bytes()),
                _ => None,
            };
            let status = if body.is_some() { "200 OK" } else { "404 Not Found" };
            let body = body.unwrap_or_default();
            let head = format!(
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(&body);
        }
    });
    let directory = app
        .selftest
        .as_ref()
        .map(|t| t.data.join("update"))
        .unwrap_or_default();
    let _ = std::fs::create_dir_all(&directory);
    let release = updater::find_update(
        taskline_core::VERSION,
        &format!("{base}/api"),
        &platform::http_get,
    );
    let Some(release) = release else {
        return check(app, false, "updater: the fake v99.0.0 was not found");
    };
    match updater::download_verified(&release, &directory, &platform::http_get) {
        Ok(new) => check(
            app,
            std::fs::read(&new).ok() == Some(payload.clone()),
            "updater: verified download",
        ),
        Err(error) => check(app, false, format!("updater: {error}")),
    }
    *exe.lock().expect("exe") = b"tampered".to_vec();
    let tampered = updater::download_verified(&release, &directory, &platform::http_get);
    check(app, tampered.is_err(), "updater: a tampered download is rejected");
    let target = directory.join("taskline.exe");
    let script = updater::swap_script(
        &target,
        &directory.join("taskline.exe.new"),
        &[std::process::id()],
    );
    let written = platform::write_script(&script, &directory);
    check(
        app,
        written
            .as_ref()
            .is_ok_and(|p| std::fs::read(p).is_ok_and(|b| b.windows(7).any(|w| w == b"move /Y"))),
        "updater: swap script written",
    );
    line(
        app,
        "updater: fake v99.0.0 found over HTTP, verified, tampered copy rejected",
    );
}

/// With TASKLINE_SELFTEST_REGISTRY=1 (CI only): Start with Windows writes the same Run
/// value v1.x used, then the previous state is restored.
fn registry_check(app: &mut App) {
    if std::env::var("TASKLINE_SELFTEST_REGISTRY").as_deref() != Ok("1") {
        return;
    }
    let before = platform::autostart_enabled();
    let set = platform::set_autostart(true);
    let command = platform::autostart_command();
    check(
        app,
        set.is_ok() && command.as_deref() == Some(platform::launch_command().as_str()),
        format!(
            "autostart: HKCU\\{}\\{} = {command:?}",
            platform::RUN_KEY,
            platform::RUN_VALUE
        ),
    );
    let _ = platform::set_autostart(before);
    check(app, platform::autostart_enabled() == before, "autostart restored");
    line(
        app,
        format!(
            "autostart: Run value '{}' = {}",
            platform::RUN_VALUE,
            command.unwrap_or_default()
        ),
    );
}

fn finish(app: &mut App) -> Task<Message> {
    if let Some(test) = app.selftest.as_mut() {
        test.lines
            .push(if test.failed { "FAILED".into() } else { "OK".into() });
        EXIT_CODE.store(if test.failed { 1 } else { 0 }, Ordering::SeqCst);
        let _ = std::fs::write(&test.report, test.lines.join("\n") + "\n");
    }
    iced::exit()
}

fn distinct_colors(shot: &Screenshot) -> usize {
    let mut colors = std::collections::HashSet::new();
    for pixel in shot.rgba.as_chunks::<4>().0.iter().step_by(7) {
        colors.insert([pixel[0], pixel[1], pixel[2]]);
        if colors.len() > 64 {
            break;
        }
    }
    colors.len()
}

pub fn save_png(directory: &Path, name: &str, shot: &Screenshot) -> Result<(), String> {
    std::fs::create_dir_all(directory).map_err(|e| e.to_string())?;
    let file = std::fs::File::create(directory.join(name)).map_err(|e| e.to_string())?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), shot.size.width, shot.size.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    // Opaque PNGs: the windows have no transparency.
    let mut rgba = shot.rgba.to_vec();
    for pixel in rgba.as_chunks_mut::<4>().0 {
        pixel[3] = 255;
    }
    writer.write_image_data(&rgba).map_err(|e| e.to_string())
}
