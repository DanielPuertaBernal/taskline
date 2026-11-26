//! The task panel, laid out like the Omarchy v1.2 panel (`TasksPanel.qml`): title, donut,
//! Done/Pending legend, sort button, theme switch and help on the left; the add row and
//! the task list on the right; calendar and priority popovers; themed tooltips.
//!
//! The overall structure (a fixed-width control column beside a filling content area,
//! upper-case section labels, rounded containers styled with closures) follows kazari.

use std::time::Duration;

use chrono::{Datelike, NaiveDate};
use iced::widget::text::{Span, Wrapping};
use iced::widget::tooltip::Position as TipPosition;
use iced::widget::{
    Space, button, canvas, column, container, mouse_area, opaque, pin, rich_text, row, scrollable, span,
    stack, text, text_input, tooltip,
};
use iced::{Alignment, Background, Border, Color, Element, Fill, Length, Padding, Theme};
use taskline_core::calendar::month_grid;
use taskline_core::store::Task;
use taskline_core::theme::{Mode, Palette};

use crate::app::{App, DUE_ID, EDIT_ID, INPUT_ID, Message, Popover, Target};
use crate::icons::{self, icon};
use crate::shapes::Donut;
use crate::style::{
    self, BODY, CAPTION, DISPLAY, DISPLAY_LARGE, HEADING, MONO, MONO_BOLD, TITLE, alpha, mix, to_iced,
};

const PAD: f32 = 24.0;
const RIGHT_PAD: f32 = 48.0;
const LEFT_W: f32 = 240.0;
const FIELD_H: f32 = 36.0;
const ROW_H: f32 = 40.0;
const ROW_GAP: f32 = 4.0;
const LIST_TOP: f32 = PAD + FIELD_H + 12.0;
const DUE_W: f32 = 150.0;
const PRIORITY_W: f32 = 44.0;
const ICON: f32 = 18.0;
const ACTION_GAP: f32 = 16.0;
pub const CAL_W: f32 = 300.0;
pub const CAL_H: f32 = 340.0;
const MENU_W: f32 = 184.0;
const MENU_H: f32 = 4.0 * 34.0 + 12.0;
const TIP_DELAY: Duration = Duration::from_millis(450);

pub fn view(app: &App, width: f32, height: f32) -> Element<'_, Message> {
    let p = app.palette();
    let ui = app.texts.ui;
    let tasks = app.store.today(app.sort);
    let (done, total) = app.store.progress();

    let content = row![summary(app, p, done, total), list(app, p, &tasks)]
        .spacing(PAD)
        .padding(Padding {
            top: PAD,
            right: RIGHT_PAD,
            bottom: PAD,
            left: PAD,
        });

    let close = button(
        icon(&icons::CLOSE, ICON, to_iced(p.muted)).style(move |_, status| iced::widget::svg::Style {
            color: Some(if status == iced::widget::svg::Status::Hovered {
                to_iced(p.red)
            } else {
                to_iced(p.muted)
            }),
        }),
    )
    .padding(6)
    .style(style::bare(p))
    .on_press(Message::ClosePanel);

    let mut layers: Vec<Element<'_, Message>> = vec![
        container(content)
            .width(Fill)
            .height(Fill)
            .style(style::background(p))
            .into(),
        pin(tip(close, ui.close, p))
            .x(width - 16.0 - ICON - 12.0)
            .y(10.0)
            .into(),
    ];
    if let Some((popover, x, y)) = popover(app, p, &tasks, width, height) {
        layers.push(opaque(
            mouse_area(Space::new().width(Fill).height(Fill)).on_press(Message::ClosePopover),
        ));
        layers.push(pin(popover).x(x).y(y).into());
    }
    stack(layers).width(Fill).height(Fill).into()
}

// ----------------------------------------------------------------- left column --

fn summary<'a>(app: &'a App, p: &'static Palette, done: usize, total: usize) -> Element<'a, Message> {
    let ui = app.texts.ui;
    let progress = if total > 0 {
        done as f32 / total as f32
    } else {
        0.0
    };
    let title = mouse_area(text(ui.title).size(DISPLAY).font(MONO_BOLD).color(to_iced(p.fg)))
        .on_press(Message::DragPanel);

    let donut = stack![
        canvas(Donut {
            value: progress,
            done: to_iced(p.accent),
            pending: to_iced(p.track),
            hole: to_iced(p.bg)
        })
        .width(220)
        .height(220),
        container(
            text(format!("{}%", (progress * 100.0).round() as u32))
                .size(DISPLAY_LARGE)
                .font(MONO_BOLD)
        )
        .width(220)
        .height(220)
        .center(220),
    ];

    let legend = column![
        legend_line("●", format!("{}: {done}", ui.done), to_iced(p.accent)),
        legend_line("●", format!("{}: {}", ui.pending, total - done), to_iced(p.muted)),
    ]
    .spacing(8);

    let sort_label = match app.sort {
        taskline_core::store::Sort::Due => ui.sort_due,
        taskline_core::store::Sort::Priority => ui.sort_priority,
    };
    let sort = button(
        row![
            icon(&icons::SORT, 16.0, to_iced(p.fg)),
            text(sort_label).size(TITLE)
        ]
        .spacing(10)
        .align_y(Alignment::Center),
    )
    .padding([7, 12])
    .style(style::flat(p, 0.08, 0.2, 6.0))
    .on_press(Message::ToggleSort);

    let help = text(ui.help)
        .size(CAPTION)
        .color(alpha(p.muted, 0.9))
        .line_height(1.45);

    column![title, donut, legend, sort, theme_switch(app, p), help]
        .spacing(20)
        .width(Length::Fixed(LEFT_W))
        .into()
}

fn legend_line<'a>(dot: &'a str, label: String, color: Color) -> Element<'a, Message> {
    row![
        text(dot).size(TITLE).color(color),
        text(label).size(HEADING).color(color)
    ]
    .spacing(10)
    .align_y(Alignment::Center)
    .into()
}

/// "Tema" + a pill switch (sun left = light, moon right = dark) with a knob, and an
/// "Auto" chip that follows the Windows theme (ThemeSwitch.qml).
fn theme_switch<'a>(app: &'a App, p: &'static Palette) -> Element<'a, Message> {
    let ui = app.texts.ui;
    let light = app.mode() == Mode::Light;
    let system = app.choice == taskline_core::theme::Choice::System;
    let sun = icon(
        &icons::SUN,
        13.0,
        if light { to_iced(p.bg) } else { to_iced(p.muted) },
    );
    let moon = icon(
        &icons::MOON,
        13.0,
        if light { to_iced(p.muted) } else { to_iced(p.bg) },
    );
    let knob = container(Space::new().width(22).height(22)).style(move |_| container::Style {
        background: Some(Background::Color(to_iced(p.accent))),
        border: Border::default().rounded(11),
        ..Default::default()
    });
    let pill = stack![
        container(Space::new().width(54).height(26)).style(move |_| container::Style {
            background: Some(Background::Color(alpha(p.accent, 0.18))),
            border: Border::default()
                .rounded(13)
                .width(1)
                .color(alpha(p.accent, 0.45)),
            ..Default::default()
        }),
        pin(knob).x(if light { 2.0 } else { 30.0 }).y(2.0),
        pin(sun).x(6.5).y(6.5),
        pin(moon).x(34.5).y(6.5),
    ]
    .width(54)
    .height(26);
    let label = if light { ui.light } else { ui.dark };
    let switch = button(
        row![text(ui.theme).size(TITLE), pill]
            .spacing(10)
            .align_y(Alignment::Center),
    )
    .padding([5, 12])
    .style(style::flat(p, 0.08, 0.2, 6.0))
    .on_press(Message::FlipTheme);

    let auto =
        button(
            text(ui.auto)
                .size(CAPTION)
                .color(if system { to_iced(p.fg) } else { to_iced(p.muted) }),
        )
        .padding([4, 9])
        .style(move |_, status| {
            let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
            button::Style {
                background: Some(Background::Color(alpha(
                    p.accent,
                    if system {
                        0.28
                    } else if hovered {
                        0.16
                    } else {
                        0.06
                    },
                ))),
                text_color: to_iced(p.fg),
                border: Border::default()
                    .rounded(11)
                    .width(if system { 1.0 } else { 0.0 })
                    .color(to_iced(p.accent)),
                ..Default::default()
            }
        })
        .on_press(Message::SetTheme(taskline_core::theme::Choice::System));

    row![tip(switch, label, p), tip(auto, ui.auto_tip, p)]
        .spacing(8)
        .align_y(Alignment::Center)
        .into()
}

// ---------------------------------------------------------------- right column --

fn list<'a>(app: &'a App, p: &'static Palette, tasks: &[Task]) -> Element<'a, Message> {
    let ui = app.texts.ui;
    let input = text_input(ui.new_task, &app.input)
        .id(INPUT_ID)
        .on_input(Message::Input)
        .on_submit(Message::Submit)
        .size(TITLE)
        .padding(9)
        .font(MONO)
        .style(style::field(p))
        .width(Fill);
    let due = text_input(ui.due_placeholder, &app.due)
        .id(DUE_ID)
        .on_input(Message::Due)
        .on_submit(Message::DueSubmit)
        .size(TITLE)
        .padding(iced::Padding {
            left: 34.0,
            ..iced::Padding::new(9.0)
        })
        .font(MONO)
        .style(style::field(p))
        .width(Length::Fixed(DUE_W));
    let due = stack![
        due,
        pin(icon(&icons::CALENDAR, 15.0, to_iced(p.muted)))
            .x(11.0)
            .y(10.5)
    ];

    let priority_name = app
        .new_priority
        .map(|v| app.texts.priority_name(Some(v)).to_lowercase())
        .unwrap_or_else(|| ui.no_priority_lower.to_owned());
    let priority_face: Element<'_, Message> = match app.new_priority.and_then(icons::priority) {
        Some(handle) => icon(
            handle,
            20.0,
            to_iced(p.priority(app.new_priority.unwrap_or("")).unwrap_or(p.fg)),
        )
        .into(),
        None => text("–")
            .size(TITLE)
            .font(MONO_BOLD)
            .color(to_iced(p.muted))
            .into(),
    };
    let priority = button(container(priority_face).center_x(Fill).center_y(Fill))
        .width(Length::Fixed(PRIORITY_W))
        .height(Length::Fixed(FIELD_H))
        .padding(0)
        .style(style::flat(p, 0.08, 0.2, 6.0))
        .on_press(Message::OpenPriority(Target::New));
    let priority = tip(priority, format!("{}: {priority_name}", ui.priority), p);

    let inputs = row![input, due, priority]
        .spacing(8)
        .height(Length::Fixed(FIELD_H))
        .align_y(Alignment::Center);

    let mut items = column![].spacing(ROW_GAP);
    for task in tasks {
        items = items.push(task_row(app, p, task));
    }
    let body: Element<'_, Message> = if tasks.is_empty() {
        container(text(ui.empty).size(TITLE).color(to_iced(p.muted)))
            .center_x(Fill)
            .padding(40)
            .into()
    } else {
        scrollable(items)
            .on_scroll(|viewport| Message::Scrolled(viewport.absolute_offset().y))
            .style(style::scrollbar(p))
            .height(Fill)
            .into()
    };

    let mut right = column![inputs].spacing(12).width(Fill);
    if let Some(problem) = app.error.as_ref().or(app.store.load_error.as_ref()) {
        let shown = if app.store.read_only && app.error.is_none() {
            ui.read_only.to_owned()
        } else {
            problem.clone()
        };
        right = right.push(text(shown).size(BODY).color(to_iced(p.red)));
    }
    right.push(body).into()
}

fn task_row<'a>(app: &'a App, p: &'static Palette, task: &Task) -> Element<'a, Message> {
    let ui = app.texts.ui;
    let uuid = task.uuid().to_owned();
    let done = task.is_done();
    let editing = app.editing.as_deref() == Some(task.uuid());
    let hovered = app.hovered.as_deref() == Some(task.uuid());
    let strength = if hovered || editing { 1.0 } else { 0.35 };

    let check: Element<'_, Message> = if done {
        container(icon(&icons_check(), 12.0, to_iced(p.bg)))
            .center(18)
            .style(move |_| container::Style {
                background: Some(Background::Color(to_iced(p.accent))),
                border: Border::default().rounded(9),
                ..Default::default()
            })
            .into()
    } else {
        container(Space::new().width(18).height(18))
            .style(move |_| container::Style {
                border: Border::default().rounded(9).width(2).color(to_iced(p.fg)),
                ..Default::default()
            })
            .into()
    };
    let check = tip(
        button(check)
            .padding(2)
            .style(style::bare(p))
            .on_press(Message::ToggleDone(uuid.clone())),
        if done { ui.reopen } else { ui.complete },
        p,
    );

    let mut line = row![check]
        .spacing(8)
        .align_y(Alignment::Center)
        .height(Length::Fixed(ROW_H));

    if editing {
        line = line.push(
            text_input("", &app.edit_text)
                .id(EDIT_ID)
                .on_input(Message::EditText)
                .on_submit(Message::SaveEdit)
                .size(TITLE)
                .padding([6, 8])
                .font(MONO)
                .style(style::field(p))
                .width(Fill),
        );
        let current = app.edit_priority();
        let face = match current {
            Some(value) => icon(
                icons::priority(value).unwrap_or(&icons::MEDIUM),
                20.0,
                to_iced(p.priority(value).unwrap_or(p.fg)),
            ),
            None => icon(&icons::MEDIUM, 20.0, mix(p.muted, p.bg, 0.5)),
        };
        line = line.push(Space::new().width(8));
        line = line.push(tip(
            button(face)
                .padding(0)
                .style(style::bare(p))
                .on_press(Message::OpenPriority(Target::Edit)),
            ui.priority,
            p,
        ));
        line = line.push(Space::new().width(ACTION_GAP - 8.0));
        line = line.push(tip(
            button(icon(&icons::CALENDAR, ICON, to_iced(p.accent)))
                .padding(0)
                .style(style::bare(p))
                .on_press(Message::OpenCalendar(Target::Edit)),
            ui.date,
            p,
        ));
    } else {
        let marker: Element<'_, Message> = match task.priority().and_then(icons::priority) {
            Some(handle) => tip(
                icon(
                    handle,
                    20.0,
                    to_iced(p.priority(task.priority().unwrap_or("")).unwrap_or(p.fg)),
                ),
                format!(
                    "{} {}",
                    ui.priority,
                    app.texts.priority_name(task.priority()).to_lowercase()
                ),
                p,
            ),
            None => Space::new().width(20).height(20).into(),
        };
        line = line.push(marker);
        let mut spans: Vec<Span<'_, (), iced::Font>> = vec![
            span(task.description().to_owned())
                .color(to_iced(if done { p.muted } else { p.fg }))
                .strikethrough(done),
        ];
        if let Some(project) = task.project() {
            spans.push(span(format!("   · {project}")).color(to_iced(p.muted)));
        }
        line = line.push(Space::new().width(2));
        line = line.push(
            container(rich_text(spans).size(TITLE).wrapping(Wrapping::None))
                .width(Fill)
                .clip(true),
        );
        if let Some(due) = task.due() {
            let label = app
                .texts
                .due_label(app.store.local_date(due), app.store.today_date());
            let color = app.store.due_state(task).map(|s| p.due(s)).unwrap_or(p.muted);
            line = line.push(
                text(label)
                    .size(TITLE)
                    .color(to_iced(color))
                    .wrapping(Wrapping::None),
            );
            line = line.push(Space::new().width(ACTION_GAP - 8.0));
        }
    }

    line = line.push(Space::new().width(ACTION_GAP - 8.0));
    line = line.push(tip(
        button(icon(&icons::EDIT, ICON, mix(p.fg, p.bg, strength)))
            .padding(0)
            .style(style::bare(p))
            .on_press(Message::StartEdit(uuid.clone())),
        ui.edit,
        p,
    ));
    line = line.push(Space::new().width(ACTION_GAP - 8.0));
    line = line.push(tip(
        button(icon(&icons::TRASH, ICON, mix(p.red, p.bg, strength)))
            .padding(0)
            .style(style::bare(p))
            .on_press(Message::Delete(uuid.clone())),
        ui.delete,
        p,
    ));
    line = line.push(Space::new().width(0));

    let mut area = mouse_area(container(line).padding([0, 8]).style(style::row(p, hovered)))
        .on_enter(Message::Hover(Some(uuid.clone())))
        .on_exit(Message::Hover(None));
    if !editing {
        area = area.on_double_click(Message::StartEdit(uuid));
    }
    area.into()
}

fn icons_check() -> iced::widget::svg::Handle {
    static CHECK: std::sync::LazyLock<iced::widget::svg::Handle> = std::sync::LazyLock::new(|| {
        iced::widget::svg::Handle::from_memory(include_bytes!("../assets/icons/check.svg").as_slice())
    });
    CHECK.clone()
}

// ------------------------------------------------------------------- popovers --

/// The open popover and where it goes: under its anchor, or above when there is no room.
/// Where the calendar of the add row opens (the selftest hovers its days).
pub fn new_calendar_origin(width: f32) -> (f32, f32) {
    let right = width - RIGHT_PAD - PRIORITY_W - 8.0;
    (
        (right - CAL_W).clamp(12.0, (width - CAL_W - 12.0).max(12.0)),
        PAD + FIELD_H + 6.0,
    )
}

fn popover<'a>(
    app: &'a App,
    p: &'static Palette,
    tasks: &[Task],
    width: f32,
    height: f32,
) -> Option<(Element<'a, Message>, f32, f32)> {
    let right_edge = width - RIGHT_PAD;
    let edit_row_top = || {
        let index = tasks
            .iter()
            .position(|t| Some(t.uuid()) == app.editing.as_deref())
            .unwrap_or(0);
        LIST_TOP + index as f32 * (ROW_H + ROW_GAP) - app.scroll_y
    };
    let place = |anchor_right: f32, anchor_top: f32, anchor_h: f32, w: f32, h: f32| {
        let x = (anchor_right - w).clamp(12.0, (width - w - 12.0).max(12.0));
        let below = anchor_top + anchor_h + 6.0;
        let y = if below + h > height - 12.0 {
            (anchor_top - 6.0 - h).max(8.0)
        } else {
            below
        };
        (x, y)
    };
    // Right edges of the anchors (see `list` and `task_row`).
    let edit_actions = right_edge - 8.0 - ICON - ACTION_GAP - ICON - ACTION_GAP;
    match &app.popover {
        Popover::None => None,
        Popover::Calendar {
            target,
            year,
            month,
            cursor,
        } => {
            let (x, y) = match target {
                Target::New => place(right_edge - PRIORITY_W - 8.0, PAD, FIELD_H, CAL_W, CAL_H),
                Target::Edit => place(edit_actions, edit_row_top(), ROW_H, CAL_W, CAL_H),
            };
            Some((date_picker(app, p, *year, *month, *cursor), x, y))
        }
        Popover::Priority { target } => {
            let (x, y) = match target {
                Target::New => place(right_edge, PAD, FIELD_H, MENU_W, MENU_H),
                Target::Edit => place(
                    edit_actions - ICON - ACTION_GAP + 8.0,
                    edit_row_top(),
                    ROW_H,
                    MENU_W,
                    MENU_H,
                ),
            };
            let current = match target {
                Target::New => app.new_priority,
                Target::Edit => app.edit_priority(),
            };
            Some((priority_menu(app, p, current), x, y))
        }
    }
}

/// Month calendar (DatePicker.qml): Monday first, today outlined, the cursor filled,
/// a dot on days with pending tasks, arrows, and chips.
fn date_picker<'a>(
    app: &'a App,
    p: &'static Palette,
    year: i32,
    month: u32,
    cursor: NaiveDate,
) -> Element<'a, Message> {
    let texts = app.texts;
    let today = app.store.today_date();
    let due_days = app.store.due_days();
    let arrow = |handle: &iced::widget::svg::Handle, message: Message| {
        button(icon(handle, 16.0, to_iced(p.fg)))
            .padding(6)
            .style(style::flat(p, 0.0, 0.18, 6.0))
            .on_press(message)
    };
    let header = row![
        arrow(&icons::PREV, Message::CalendarMonth(-1)),
        container(text(texts.month_title(year, month)).size(TITLE).font(MONO_BOLD)).center_x(Fill),
        arrow(&icons::NEXT, Message::CalendarMonth(1)),
    ]
    .align_y(Alignment::Center);

    let weekdays = texts.weekdays.iter().fold(row![], |r, name| {
        r.push(container(text(*name).size(CAPTION).color(to_iced(p.muted))).center_x(Length::Fixed(39.0)))
    });

    let grid = month_grid(year, month);
    let mut weeks = column![].spacing(2);
    for week in grid.chunks(7) {
        let mut line = row![];
        for day in week {
            line = line.push(day_cell(
                p,
                *day,
                month,
                today,
                cursor,
                due_days.get(day).copied().unwrap_or(0),
            ));
        }
        weeks = weeks.push(line);
    }

    let chip = |label: &'static str, days: Option<i64>| {
        button(text(label).size(CAPTION))
            .padding([4, 7])
            .style(style::flat(p, 0.1, 0.24, 11.0))
            .on_press(Message::ChipDays(days))
    };
    let chips = row![
        chip(texts.chips[0], Some(0)),
        chip(texts.chips[1], Some(1)),
        chip(texts.chips[2], Some(7)),
        chip(texts.chips[3], None),
    ]
    .spacing(5)
    .wrap();

    container(column![header, weekdays, weeks, chips].spacing(6))
        .padding(12)
        .width(Length::Fixed(CAL_W))
        .style(style::popover(p))
        .into()
}

fn day_cell<'a>(
    p: &'static Palette,
    day: NaiveDate,
    month: u32,
    today: NaiveDate,
    cursor: NaiveDate,
    pending: usize,
) -> Element<'a, Message> {
    let in_month = day.month() == month;
    let is_cursor = day == cursor;
    let is_today = day == today;
    let color = if in_month {
        to_iced(p.fg)
    } else {
        alpha(p.muted, 0.7)
    };
    let dot: Element<'_, Message> = if pending > 0 {
        container(Space::new().width(4).height(4))
            .style(move |_| container::Style {
                background: Some(Background::Color(to_iced(p.orange))),
                border: Border::default().rounded(2),
                ..Default::default()
            })
            .into()
    } else {
        Space::new().width(4).height(4).into()
    };
    let face = column![
        text(day.day().to_string())
            .size(BODY)
            .color(color)
            .font(if is_cursor { MONO_BOLD } else { MONO }),
        dot
    ]
    .spacing(1)
    .align_x(Alignment::Center);
    button(container(face).center_x(Fill).padding(iced::Padding {
        top: 6.0,
        ..Default::default()
    }))
    .width(Length::Fixed(39.0))
    .height(Length::Fixed(34.0))
    .padding(0)
    .style(move |_: &Theme, status| {
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        let fill = if is_cursor {
            Some(alpha(p.accent, 0.22))
        } else if hovered {
            Some(alpha(p.accent, 0.14))
        } else {
            None
        };
        let border = if is_cursor {
            Border::default().rounded(7).width(1.5).color(to_iced(p.accent))
        } else if is_today {
            Border::default().rounded(7).width(1).color(alpha(p.accent, 0.8))
        } else {
            Border::default().rounded(7)
        };
        button::Style {
            background: fill.map(Background::Color),
            text_color: color,
            border,
            ..Default::default()
        }
    })
    .on_press(Message::PickDay(Some(day)))
    .into()
}

/// Priority choices with their icons (OptionMenu.qml).
fn priority_menu<'a>(
    app: &'a App,
    p: &'static Palette,
    current: Option<&'static str>,
) -> Element<'a, Message> {
    let texts = app.texts;
    let options: [(Option<&'static str>, &'static str); 4] = [
        (Some("H"), texts.priorities[0]),
        (Some("M"), texts.priorities[1]),
        (Some("L"), texts.priorities[2]),
        (None, texts.priorities[3]),
    ];
    let mut list = column![].spacing(0);
    for (value, label) in options {
        let is_current = value == current;
        let face: Element<'_, Message> = match value.and_then(icons::priority) {
            Some(handle) => icon(
                handle,
                20.0,
                to_iced(p.priority(value.unwrap_or("")).unwrap_or(p.fg)),
            )
            .into(),
            None => Space::new().width(20).height(20).into(),
        };
        let label = text(label)
            .size(BODY)
            .font(if is_current { MONO_BOLD } else { MONO })
            .color(if value.is_none() {
                to_iced(p.muted)
            } else {
                to_iced(p.fg)
            });
        list = list.push(
            button(row![face, label].spacing(10).align_y(Alignment::Center))
                .width(Fill)
                .height(Length::Fixed(34.0))
                .padding([0, 10])
                .style(style::flat(p, if is_current { 0.08 } else { 0.0 }, 0.18, 6.0))
                .on_press(Message::PickPriority(value)),
        );
    }
    container(list)
        .padding(6)
        .width(Length::Fixed(MENU_W))
        .style(style::popover(p))
        .into()
}

/// A themed hover hint above the control (Tip.qml).
fn tip<'a>(
    content: impl Into<Element<'a, Message>>,
    label: impl Into<String>,
    p: &'static Palette,
) -> Element<'a, Message> {
    tooltip(
        content,
        container(text(label.into()).size(CAPTION).color(to_iced(p.fg)))
            .padding([4, 8])
            .style(style::tip(p)),
        TipPosition::Top,
    )
    .gap(6)
    .delay(TIP_DELAY)
    .into()
}
