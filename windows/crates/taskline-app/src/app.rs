//! The application state and its updates (iced's Elm architecture, as in kazari): `App`
//! is the state, `Message` lists every user action, `update` changes the state and the
//! views in `bar.rs` and `panel.rs` draw it. The task logic lives in `taskline-core`.

use std::path::PathBuf;
use std::time::Duration;

use chrono::{Datelike, Duration as Days, NaiveDate};
use iced::keyboard::{self, Key, key::Named};
use iced::widget::{Id, operation};
use iced::window::{self, Level, Position};
use iced::{Event, Point, Size, Subscription, Task, event, mouse};
use taskline_core::calendar::{self, due_in_text, move_month, priority_in_text, quick_date};
use taskline_core::i18n::Texts;
use taskline_core::settings::Settings;
use taskline_core::store::{Sort, StoreError, TaskStore};
use taskline_core::theme::{Choice, Mode, Palette};
use taskline_core::updater::{self, Release};

use crate::platform::{self, MenuAction, MenuState, tray::Tray};
use crate::shapes::Segments;

pub const INPUT_ID: &str = "new-task";
pub const DUE_ID: &str = "new-due";
pub const EDIT_ID: &str = "edit-task";

pub const PANEL_SIZE: Size = Size::new(875.0, 600.0);
pub const BAR_HEIGHT: f32 = 30.0;
const NOTICE: Duration = Duration::from_secs(6);
const UPDATE_DELAY: Duration = Duration::from_secs(4);
const DRAG_THRESHOLD: f32 = 4.0;

/// Where a popover writes: the add row or the task being edited.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    New,
    Edit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Popover {
    None,
    Calendar {
        target: Target,
        year: i32,
        month: u32,
        cursor: NaiveDate,
    },
    Priority {
        target: Target,
    },
}

#[derive(Debug, Clone)]
pub enum Message {
    // Bar
    BarPressed,
    BarMoved(Point),
    BarReleased,
    BarMenu,
    BarHandle(isize),
    // Panel
    TogglePanel,
    ClosePanel,
    DragPanel,
    Input(String),
    Due(String),
    Submit,
    DueSubmit,
    OpenPriority(Target),
    PickPriority(Option<&'static str>),
    OpenCalendar(Target),
    CalendarMonth(i32),
    PickDay(Option<NaiveDate>),
    ChipDays(Option<i64>),
    ClosePopover,
    ToggleDone(String),
    StartEdit(String),
    EditText(String),
    SaveEdit,
    Delete(String),
    Hover(Option<String>),
    Scrolled(f32),
    ToggleSort,
    FlipTheme,
    SetTheme(Choice),
    SystemTheme(iced::theme::Mode),
    DueFocused(bool),
    Key(Key, keyboard::Modifiers),
    PanelPressed,
    // Runtime
    Window(window::Id, window::Event),
    Tick,
    Poll,
    Menu(MenuAction),
    CheckUpdates {
        manual: bool,
    },
    UpdateResult {
        manual: bool,
        result: Result<Option<(Release, PathBuf)>, String>,
    },
    ClearNotice,
    Selftest(crate::selftest::Step),
    Opened,
}

pub struct App {
    pub store: TaskStore,
    pub settings: Settings,
    pub texts: &'static Texts,
    pub choice: Choice,
    pub system_light: bool,
    pub sort: Sort,
    pub bar: Option<window::Id>,
    pub panel: Option<window::Id>,
    pub bar_visible: bool,
    pub input: String,
    pub due: String,
    pub new_priority: Option<&'static str>,
    pub editing: Option<String>,
    pub edit_text: String,
    pub hovered: Option<String>,
    pub popover: Popover,
    pub scroll_y: f32,
    pub error: Option<String>,
    pub notice: Option<String>,
    pub autostart: bool,
    pub panel_size: Size,
    press: Option<Point>,
    dragging: bool,
    position_dirty: Option<(i32, i32)>,
    tray: Option<Tray>,
    tray_tip: String,
    pub selftest: Option<crate::selftest::Selftest>,
    update_running: bool,
}

/// What `main` hands to the app.
pub struct Boot {
    pub store: TaskStore,
    pub settings: Settings,
    pub texts: &'static Texts,
    pub with_tray: bool,
    pub selftest: Option<crate::selftest::Selftest>,
}

impl App {
    pub fn new(boot: Boot) -> (App, Task<Message>) {
        let Boot {
            store,
            mut settings,
            texts,
            with_tray,
            selftest,
        } = boot;
        let notice = settings.take_update_notice();
        if notice.is_some() {
            let _ = settings.save();
        }
        let mut app = App {
            sort: settings.sort(),
            choice: settings.theme(),
            store,
            settings,
            texts,
            system_light: false,
            bar: None,
            panel: None,
            bar_visible: true,
            input: String::new(),
            due: String::new(),
            new_priority: None,
            editing: None,
            edit_text: String::new(),
            hovered: None,
            popover: Popover::None,
            scroll_y: 0.0,
            error: None,
            notice: None,
            autostart: platform::autostart_enabled(),
            panel_size: PANEL_SIZE,
            press: None,
            dragging: false,
            position_dirty: None,
            tray: None,
            tray_tip: String::new(),
            selftest,
            update_running: false,
        };
        if with_tray {
            let (rgba, size) = crate::logo_rgba();
            app.tray = Some(Tray::new(texts.ui, rgba, size, app.menu_state()));
            app.refresh_tray();
        }
        let mut tasks = vec![app.open_bar(), iced::system::theme().map(Message::SystemTheme)];
        // A v1.2 (or older v2) build that just updated to this one left the version it
        // installed; it matches us unless the update was rolled back.
        if notice.as_deref() == Some(taskline_core::VERSION) {
            app.notice = Some(format!("✓ {} v{}", texts.ui.updated, taskline_core::VERSION));
            tasks.push(Task::perform(sleep(NOTICE), |()| Message::ClearNotice));
        }
        if app.selftest.is_none() {
            if app.store.read_only {
                let reason = app.store.load_error.clone().unwrap_or_default();
                platform::warn(&format!(
                    "{reason}\n\nTaskline opened read-only so nothing is overwritten. Restore the file \
                     (backups are next to it as tasks.json.bak-*) and restart Taskline."
                ));
            }
            tasks.push(Task::perform(sleep(UPDATE_DELAY), |()| Message::CheckUpdates {
                manual: false,
            }));
        } else {
            tasks.push(Task::done(Message::Selftest(crate::selftest::Step::Start)));
        }
        (app, Task::batch(tasks))
    }

    pub fn mode(&self) -> Mode {
        self.choice.resolve(self.system_light)
    }

    pub fn palette(&self) -> &'static Palette {
        self.mode().palette()
    }

    pub fn segments(&self) -> usize {
        self.settings.segments()
    }

    fn menu_state(&self) -> MenuState {
        MenuState {
            autostart: self.autostart,
            auto_update: self.settings.auto_update(),
            theme: Some(self.choice),
        }
    }

    fn refresh_tray(&mut self) {
        let (done, total) = self.store.progress();
        let tip = format!("Taskline — {done}/{total} {}", self.texts.ui.tray_tip);
        if let Some(tray) = &self.tray {
            if tip != self.tray_tip {
                tray.set_tooltip(&tip);
            }
            tray.set_state(self.menu_state());
        }
        self.tray_tip = tip;
    }

    fn save_settings(&mut self) {
        if let Err(error) = self.settings.save() {
            self.error = Some(format!("{}: {error}", self.settings.path().display()));
        }
    }

    // ------------------------------------------------------------------- windows --

    pub fn bar_size(&self) -> Size {
        let text = self.bar_text();
        let chars = text.chars().count() as f32;
        let segments = if self.notice.is_some() {
            0.0
        } else {
            Segments::width(self.segments()) + 10.0
        };
        Size::new((24.0 + segments + chars * 8.5).ceil(), BAR_HEIGHT)
    }

    pub fn bar_text(&self) -> String {
        if let Some(notice) = &self.notice {
            return notice.clone();
        }
        let (done, total) = self.store.progress();
        format!("{done}/{total}")
    }

    fn open_bar(&mut self) -> Task<Message> {
        let saved = self.settings.position();
        *SAVED_POSITION.lock().unwrap_or_else(|e| e.into_inner()) = saved;
        let position = match saved {
            Some(_) => Position::SpecificWith(place_saved),
            None => Position::SpecificWith(place_default),
        };
        let (id, open) = window::open(window::Settings {
            size: self.bar_size(),
            position,
            decorations: false,
            resizable: false,
            level: Level::AlwaysOnTop,
            exit_on_close_request: false,
            icon: crate::window_icon(),
            platform_specific: crate::platform_settings(true),
            ..Default::default()
        });
        self.bar = Some(id);
        open.map(|_| Message::Opened)
    }

    fn open_panel(&mut self) -> Task<Message> {
        if let Some(id) = self.panel {
            return window::gain_focus(id);
        }
        let (id, open) = window::open(window::Settings {
            size: PANEL_SIZE,
            min_size: Some(Size::new(760.0, 520.0)),
            position: Position::Centered,
            decorations: false,
            level: Level::Normal,
            exit_on_close_request: false,
            icon: crate::window_icon(),
            platform_specific: crate::platform_settings(false),
            ..Default::default()
        });
        self.panel = Some(id);
        self.panel_size = PANEL_SIZE;
        self.scroll_y = 0.0;
        open.map(|_| Message::Opened)
            .chain(operation::focus(Id::new(INPUT_ID)))
    }

    fn close_panel(&mut self) -> Task<Message> {
        self.popover = Popover::None;
        self.editing = None;
        self.hovered = None;
        match self.panel.take() {
            Some(id) => window::close(id),
            None => Task::none(),
        }
    }

    fn resize_bar(&self) -> Task<Message> {
        match self.bar {
            Some(id) => window::resize(id, self.bar_size()),
            None => Task::none(),
        }
    }

    // -------------------------------------------------------------------- update --

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::BarPressed => {
                self.press = Some(Point::ORIGIN);
                self.dragging = false;
                Task::none()
            }
            Message::BarMoved(point) => {
                match self.press {
                    Some(origin) if origin == Point::ORIGIN => self.press = Some(point),
                    Some(origin) if !self.dragging && origin.distance(point) > DRAG_THRESHOLD => {
                        self.dragging = true;
                        self.press = None;
                        if let Some(id) = self.bar {
                            return window::drag(id);
                        }
                    }
                    _ => {}
                }
                Task::none()
            }
            Message::BarReleased => {
                let clicked = self.press.is_some() && !self.dragging;
                self.press = None;
                self.dragging = false;
                if clicked {
                    self.update(Message::TogglePanel)
                } else {
                    Task::none()
                }
            }
            Message::BarMenu => {
                self.autostart = platform::autostart_enabled();
                self.refresh_tray();
                match self.bar {
                    Some(id) => window::run(id, |w| crate::raw_hwnd(w)).map(Message::BarHandle),
                    None => Task::none(),
                }
            }
            Message::BarHandle(hwnd) => {
                if let Some(tray) = &self.tray {
                    tray.show_menu_for(hwnd);
                }
                Task::done(Message::Poll)
            }
            Message::TogglePanel => {
                if self.panel.is_some() {
                    self.close_panel()
                } else {
                    self.open_panel()
                }
            }
            Message::ClosePanel => self.close_panel(),
            Message::DragPanel => match self.panel {
                Some(id) if self.popover == Popover::None => window::drag(id),
                _ => Task::none(),
            },
            Message::Input(text) => {
                self.input = text;
                self.error = None;
                Task::none()
            }
            Message::Due(text) => {
                self.due = text;
                self.error = None;
                if let Popover::Calendar {
                    target: Target::New, ..
                } = self.popover
                {
                    self.follow_text(quick_date(&self.due, self.store.now(), self.store.zone));
                }
                Task::none()
            }
            Message::Submit => self.submit(),
            Message::DueSubmit => {
                if let Popover::Calendar {
                    target: Target::New,
                    cursor,
                    ..
                } = self.popover
                {
                    return self.update(Message::PickDay(Some(cursor)));
                }
                self.submit()
            }
            Message::OpenPriority(target) => {
                self.popover = match self.popover {
                    Popover::Priority { target: open } if open == target => Popover::None,
                    _ => Popover::Priority { target },
                };
                Task::none()
            }
            Message::PickPriority(priority) => {
                let target = match self.popover {
                    Popover::Priority { target } => target,
                    _ => Target::New,
                };
                self.popover = Popover::None;
                match target {
                    Target::New => {
                        self.new_priority = priority;
                        operation::focus(Id::new(INPUT_ID))
                    }
                    Target::Edit => {
                        self.edit_text = calendar::set_priority_token(&self.edit_text, priority);
                        Task::batch([
                            operation::focus(Id::new(EDIT_ID)),
                            operation::move_cursor_to_end(Id::new(EDIT_ID)),
                        ])
                    }
                }
            }
            Message::OpenCalendar(target) => {
                if let Popover::Calendar { target: open, .. } = self.popover
                    && open == target
                {
                    self.popover = Popover::None;
                    return Task::none();
                }
                self.open_calendar(target);
                Task::none()
            }
            Message::CalendarMonth(delta) => {
                if let Popover::Calendar {
                    year, month, cursor, ..
                } = &mut self.popover
                {
                    (*year, *month) = calendar::shift_month(*year, *month, delta);
                    *cursor = move_month(*cursor, delta);
                }
                Task::none()
            }
            Message::ChipDays(days) => {
                let day = days.map(|d| self.store.today_date() + Days::days(d));
                self.update(Message::PickDay(day))
            }
            Message::PickDay(day) => {
                let Popover::Calendar { target, .. } = self.popover else {
                    return Task::none();
                };
                self.popover = Popover::None;
                match target {
                    Target::New => {
                        self.due = day.map(|d| d.format("%Y-%m-%d").to_string()).unwrap_or_default();
                        Task::batch([
                            operation::focus(Id::new(DUE_ID)),
                            operation::move_cursor_to_end(Id::new(DUE_ID)),
                        ])
                    }
                    Target::Edit => {
                        self.edit_text = calendar::set_due_token(&self.edit_text, day);
                        Task::batch([
                            operation::focus(Id::new(EDIT_ID)),
                            operation::move_cursor_to_end(Id::new(EDIT_ID)),
                        ])
                    }
                }
            }
            Message::ClosePopover => {
                self.popover = Popover::None;
                Task::none()
            }
            Message::ToggleDone(uuid) => {
                let result = self.store.toggle(&uuid).map(|_| ());
                self.after_change(result)
            }
            Message::Delete(uuid) => {
                if self.editing.as_deref() == Some(uuid.as_str()) {
                    self.editing = None;
                }
                let result = self.store.delete(&uuid).map(|_| ());
                self.after_change(result)
            }
            Message::StartEdit(uuid) => {
                self.popover = Popover::None;
                if self.editing.as_deref() == Some(uuid.as_str()) {
                    self.editing = None;
                    return Task::none();
                }
                match self.store.get(&uuid) {
                    Ok(task) => {
                        self.edit_text = self.store.edit_text(&task);
                        self.editing = Some(uuid);
                        Task::batch([
                            operation::focus(Id::new(EDIT_ID)),
                            operation::select_all(Id::new(EDIT_ID)),
                        ])
                    }
                    Err(_) => Task::none(),
                }
            }
            Message::EditText(text) => {
                self.edit_text = text;
                self.error = None;
                if let Popover::Calendar {
                    target: Target::Edit, ..
                } = self.popover
                {
                    self.follow_text(due_in_text(&self.edit_text, self.store.now(), self.store.zone));
                }
                Task::none()
            }
            Message::SaveEdit => {
                if let Popover::Calendar {
                    target: Target::Edit,
                    cursor,
                    ..
                } = self.popover
                {
                    return self.update(Message::PickDay(Some(cursor)));
                }
                let Some(uuid) = self.editing.clone() else {
                    return Task::none();
                };
                if self.edit_text.trim().is_empty() {
                    self.editing = None;
                    return Task::none();
                }
                match self.store.edit(&uuid, &self.edit_text.clone()) {
                    Ok(_) => {
                        self.editing = None;
                        self.after_change(Ok(()))
                    }
                    // Keep the field open so the typo can be fixed.
                    Err(error) => self.after_change(Err(error)),
                }
            }
            Message::Hover(uuid) => {
                self.hovered = uuid;
                Task::none()
            }
            Message::Scrolled(y) => {
                self.scroll_y = y;
                if matches!(
                    self.popover,
                    Popover::Calendar {
                        target: Target::Edit,
                        ..
                    } | Popover::Priority { target: Target::Edit }
                ) {
                    self.popover = Popover::None;
                }
                Task::none()
            }
            Message::ToggleSort => {
                self.sort = self.sort.toggled();
                self.settings.set_sort(self.sort);
                self.save_settings();
                Task::none()
            }
            Message::FlipTheme => {
                let next = if self.mode() == Mode::Light {
                    Choice::Dark
                } else {
                    Choice::Light
                };
                self.update(Message::SetTheme(next))
            }
            Message::SetTheme(choice) => {
                self.choice = choice;
                self.settings.set_theme(choice);
                self.save_settings();
                self.refresh_tray();
                Task::none()
            }
            Message::SystemTheme(mode) => {
                self.system_light = mode == iced::theme::Mode::Light;
                Task::none()
            }
            Message::PanelPressed => operation::is_focused(Id::new(DUE_ID)).map(Message::DueFocused),
            Message::DueFocused(focused) => {
                if focused && self.popover == Popover::None {
                    self.open_calendar(Target::New);
                }
                Task::none()
            }
            Message::Key(key, modifiers) => self.key(key, modifiers),
            Message::Window(id, event) => self.window_event(id, event),
            Message::Tick => {
                self.store.reload_if_changed();
                self.refresh_tray();
                Task::none()
            }
            Message::Poll => {
                if let Some((x, y)) = self.position_dirty.take() {
                    self.settings.set_position(Some((x, y)));
                    self.save_settings();
                }
                let actions = self.tray.as_ref().map(Tray::poll).unwrap_or_default();
                Task::batch(actions.into_iter().map(|a| self.update(Message::Menu(a))))
            }
            Message::Menu(action) => self.menu(action),
            Message::CheckUpdates { manual } => self.check_updates(manual),
            Message::UpdateResult { manual, result } => self.update_result(manual, result),
            Message::ClearNotice => {
                self.notice = None;
                self.resize_bar()
            }
            Message::Selftest(step) => crate::selftest::step(self, step),
            Message::Opened => Task::none(),
        }
    }

    fn after_change(&mut self, result: Result<(), StoreError>) -> Task<Message> {
        match result {
            Ok(()) => self.error = None,
            Err(error) => self.error = Some(error.to_string()),
        }
        self.refresh_tray();
        self.resize_bar()
    }

    fn submit(&mut self) -> Task<Message> {
        if self.input.trim().is_empty() {
            return operation::focus(Id::new(INPUT_ID));
        }
        let due = self.due.trim().to_owned();
        let result = self.store.add(
            &self.input,
            (!due.is_empty()).then_some(due.as_str()),
            self.new_priority,
        );
        match result {
            Ok(_) => {
                self.input.clear();
                self.due.clear();
                self.new_priority = None;
                self.popover = Popover::None;
                let resize = self.after_change(Ok(()));
                Task::batch([resize, operation::focus(Id::new(INPUT_ID))])
            }
            Err(error) => self.after_change(Err(error)),
        }
    }

    fn open_calendar(&mut self, target: Target) {
        let now = self.store.now();
        let typed = match target {
            Target::New => quick_date(&self.due, now, self.store.zone),
            Target::Edit => due_in_text(&self.edit_text, now, self.store.zone),
        };
        let cursor = typed.unwrap_or_else(|| self.store.today_date());
        self.popover = Popover::Calendar {
            target,
            year: cursor.year(),
            month: cursor.month(),
            cursor,
        };
    }

    /// Typed shortcuts move the calendar.
    fn follow_text(&mut self, day: Option<NaiveDate>) {
        if let (
            Some(day),
            Popover::Calendar {
                year, month, cursor, ..
            },
        ) = (day, &mut self.popover)
        {
            *cursor = day;
            *year = day.year();
            *month = day.month();
        }
    }

    pub fn edit_priority(&self) -> Option<&'static str> {
        priority_in_text(&self.edit_text)
    }

    fn key(&mut self, key: Key, _modifiers: keyboard::Modifiers) -> Task<Message> {
        if self.panel.is_none() {
            return Task::none();
        }
        if let Key::Named(Named::Escape) = key {
            return if self.popover != Popover::None {
                self.popover = Popover::None;
                Task::none()
            } else if self.editing.is_some() {
                self.editing = None;
                Task::none()
            } else {
                self.close_panel()
            };
        }
        if let Key::Named(Named::Tab) = key {
            return operation::is_focused(Id::new(DUE_ID)).map(Message::DueFocused);
        }
        if let Popover::Calendar {
            year, month, cursor, ..
        } = &mut self.popover
        {
            let moved = match key {
                Key::Named(Named::ArrowLeft) => Some(*cursor - Days::days(1)),
                Key::Named(Named::ArrowRight) => Some(*cursor + Days::days(1)),
                Key::Named(Named::ArrowUp) => Some(*cursor - Days::days(7)),
                Key::Named(Named::ArrowDown) => Some(*cursor + Days::days(7)),
                Key::Named(Named::PageUp) => Some(move_month(*cursor, -1)),
                Key::Named(Named::PageDown) => Some(move_month(*cursor, 1)),
                _ => None,
            };
            if let Some(day) = moved {
                *cursor = day;
                *year = day.year();
                *month = day.month();
            }
        }
        Task::none()
    }

    fn window_event(&mut self, id: window::Id, event: window::Event) -> Task<Message> {
        match event {
            window::Event::Moved(point) if Some(id) == self.bar => {
                self.position_dirty = Some((point.x.round() as i32, point.y.round() as i32));
                Task::none()
            }
            window::Event::Resized(size) if Some(id) == self.panel => {
                self.panel_size = size;
                Task::none()
            }
            window::Event::CloseRequested if Some(id) == self.panel => self.close_panel(),
            window::Event::Closed if Some(id) == self.panel => {
                self.panel = None;
                Task::none()
            }
            _ => Task::none(),
        }
    }

    fn menu(&mut self, action: MenuAction) -> Task<Message> {
        match action {
            MenuAction::OpenPanel => self.open_panel(),
            MenuAction::ToggleBar => {
                self.bar_visible = !self.bar_visible;
                match self.bar {
                    Some(id) => window::set_mode(
                        id,
                        if self.bar_visible {
                            window::Mode::Windowed
                        } else {
                            window::Mode::Hidden
                        },
                    ),
                    None => Task::none(),
                }
            }
            MenuAction::Autostart => {
                if let Err(error) = platform::set_autostart(!platform::autostart_enabled()) {
                    platform::error(&format!("Could not change the startup setting:\n{error}"));
                }
                self.autostart = platform::autostart_enabled();
                self.refresh_tray();
                Task::none()
            }
            MenuAction::Import => {
                if let Some(path) = platform::pick_import() {
                    match self.store.import_file(&path) {
                        Ok(count) => platform::info(&format!("{} {count}", self.texts.ui.imported)),
                        Err(error) => platform::error(&format!("{}:\n{error}", path.display())),
                    }
                }
                self.after_change(Ok(()))
            }
            MenuAction::Export => {
                if let Some(path) = platform::pick_export()
                    && let Err(error) = self.store.export_file(&path)
                {
                    platform::error(&format!("{}:\n{error}", path.display()));
                }
                Task::none()
            }
            MenuAction::ResetPosition => {
                self.settings.set_position(None);
                self.save_settings();
                match self.bar {
                    Some(id) => window::monitor_size(id).then(move |monitor| {
                        let width = monitor.map(|m| m.width).unwrap_or(1280.0);
                        window::size(id).then(move |size| {
                            window::move_to(id, Point::new(((width - size.width) / 2.0).max(0.0), 6.0))
                        })
                    }),
                    None => Task::none(),
                }
            }
            MenuAction::ThemeLight => self.update(Message::SetTheme(Choice::Light)),
            MenuAction::ThemeDark => self.update(Message::SetTheme(Choice::Dark)),
            MenuAction::ThemeSystem => self.update(Message::SetTheme(Choice::System)),
            MenuAction::AutoUpdate => {
                let enabled = !self.settings.auto_update();
                self.settings.set_auto_update(enabled);
                self.save_settings();
                self.refresh_tray();
                Task::none()
            }
            MenuAction::CheckUpdates => self.update(Message::CheckUpdates { manual: true }),
            MenuAction::About => {
                platform::info(&format!(
                    "Taskline {}\n\nYour day, as one line.\nhttps://github.com/{}\n\nTasks: {}\n\n\
                     Interface based on kazari by Santiago Otalvaro (MIT).",
                    taskline_core::VERSION,
                    updater::REPO,
                    self.store.path.display()
                ));
                Task::none()
            }
            MenuAction::Quit => iced::exit(),
        }
    }

    // ------------------------------------------------------------------- updates --

    fn check_updates(&mut self, manual: bool) -> Task<Message> {
        let today = chrono::Local::now().date_naive();
        if self.update_running || (!manual && !self.settings.update_check_due(today)) {
            return Task::none();
        }
        let api = std::env::var("TASKLINE_UPDATE_API")
            .ok()
            .filter(|u| !u.is_empty());
        if !cfg!(windows) && api.is_none() {
            if manual {
                platform::info("Updates are installed by taskline.exe on Windows only.");
            }
            return Task::none();
        }
        self.settings.mark_update_checked(today);
        self.save_settings();
        self.update_running = true;
        let api = api.unwrap_or_else(|| updater::API_URL.to_owned());
        let Ok(exe) = std::env::current_exe() else {
            return Task::none();
        };
        Task::perform(
            blocking(move || {
                let Some(release) = updater::find_update(taskline_core::VERSION, &api, &platform::http_get)
                else {
                    return Ok(None);
                };
                let directory = exe.parent().map(PathBuf::from).unwrap_or_default();
                updater::download_verified(&release, &directory, &platform::http_get)
                    .map(|new| Some((release, new)))
                    .map_err(|e| e.0)
            }),
            move |result| Message::UpdateResult { manual, result },
        )
    }

    fn update_result(
        &mut self,
        manual: bool,
        result: Result<Option<(Release, PathBuf)>, String>,
    ) -> Task<Message> {
        self.update_running = false;
        match result {
            Ok(None) => {
                if manual {
                    platform::info(&format!(
                        "Taskline {} {}",
                        taskline_core::VERSION,
                        self.texts.ui.up_to_date
                    ));
                }
                Task::none()
            }
            Err(error) => {
                if manual {
                    platform::error(&format!("{}:\n{error}", self.texts.ui.update_failed));
                }
                Task::none()
            }
            Ok(Some((release, new))) => {
                let Ok(target) = std::env::current_exe() else {
                    return Task::none();
                };
                let script = updater::swap_script(&target, &new, &[std::process::id()]);
                let launched = platform::write_script(&script, &std::env::temp_dir())
                    .and_then(|path| platform::launch_script(&path));
                match launched {
                    Ok(()) => {
                        self.settings.set_update_notice(&release.version);
                        self.save_settings();
                        iced::exit()
                    }
                    Err(error) => {
                        let _ = std::fs::remove_file(&new);
                        if manual {
                            platform::error(&format!("{}:\n{error}", self.texts.ui.update_failed));
                        }
                        Task::none()
                    }
                }
            }
        }
    }

    // -------------------------------------------------------------- subscription --

    pub fn subscription(&self) -> Subscription<Message> {
        let mut subscriptions = vec![
            window::events().map(|(id, event)| Message::Window(id, event)),
            iced::time::every(Duration::from_secs(60)).map(|_| Message::Tick),
            iced::system::theme_changes().map(Message::SystemTheme),
            event::listen_with(panel_events),
        ];
        if self.tray.is_some() || self.position_dirty.is_some() {
            subscriptions.push(iced::time::every(Duration::from_millis(150)).map(|_| Message::Poll));
        }
        Subscription::batch(subscriptions)
    }
}

fn panel_events(event: Event, _status: event::Status, _window: window::Id) -> Option<Message> {
    match event {
        Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) => {
            Some(Message::Key(key, modifiers))
        }
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => Some(Message::PanelPressed),
        _ => None,
    }
}

/// Read by `place_saved`, which iced calls with no state of ours.
static SAVED_POSITION: std::sync::Mutex<Option<(i32, i32)>> = std::sync::Mutex::new(None);

/// Top center, 6 px from the top, like v1.x.
fn place_default(window: Size, monitor: Size) -> Point {
    Point::new(((monitor.width - window.width) / 2.0).max(0.0).round(), 6.0)
}

/// The saved position, kept on screen (v1.x saved physical pixels; at a display scale
/// above 100% they can point past the edge).
fn place_saved(window: Size, monitor: Size) -> Point {
    let Some((x, y)) = *SAVED_POSITION.lock().unwrap_or_else(|e| e.into_inner()) else {
        return place_default(window, monitor);
    };
    let max_x = (monitor.width - window.width).max(0.0);
    let max_y = (monitor.height - window.height).max(0.0);
    Point::new((x as f32).clamp(0.0, max_x), (y as f32).clamp(0.0, max_y))
}

pub async fn sleep(duration: Duration) {
    blocking(move || std::thread::sleep(duration)).await
}

/// Runs `work` on its own thread and resolves with its result.
pub async fn blocking<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> T {
    let (sender, receiver) = iced::futures::channel::oneshot::channel();
    std::thread::spawn(move || {
        let _ = sender.send(work());
    });
    receiver.await.expect("worker thread finished")
}
