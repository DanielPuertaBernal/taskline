//! The always-on-top bar: `▰▰▰▱▱▱▱▱▱▱  1/4` as rounded segments and a count. Left
//! click opens the panel, dragging moves it, right click opens the menu.

use iced::widget::{Space, canvas, container, mouse_area, row, text};
use iced::{Alignment, Background, Border, Element, Fill, Length};

use crate::app::{App, Message};
use crate::shapes::{Segments, filled_segments};
use crate::style::{MONO_BOLD, alpha, to_iced};

pub fn view(app: &App) -> Element<'_, Message> {
    let p = app.palette();
    let (done, total) = app.store.progress();
    let all_done = total > 0 && done == total;
    let content: Element<'_, Message> = match &app.notice {
        Some(notice) => text(notice.clone())
            .size(14)
            .font(MONO_BOLD)
            .color(to_iced(p.green))
            .into(),
        None => {
            let segments = app.segments();
            row![
                canvas(Segments {
                    filled: filled_segments(done, total, segments),
                    total: segments,
                    on: to_iced(p.accent),
                    off: alpha(p.track, 0.95),
                })
                .width(Length::Fixed(Segments::width(segments)))
                .height(Fill),
                Space::new().width(10),
                text(app.bar_text())
                    .size(14)
                    .font(MONO_BOLD)
                    .color(to_iced(if all_done { p.accent } else { p.fg })),
            ]
            .align_y(Alignment::Center)
            .into()
        }
    };
    let bar = container(content)
        .padding([0, 12])
        .width(Fill)
        .height(Fill)
        .align_y(Alignment::Center)
        .style(move |_| container::Style {
            background: Some(Background::Color(to_iced(p.bg))),
            border: Border::default().rounded(8).width(1).color(alpha(p.border, 0.9)),
            text_color: Some(to_iced(p.fg)),
            ..Default::default()
        });
    mouse_area(bar)
        .on_press(Message::BarPressed)
        .on_move(Message::BarMoved)
        .on_release(Message::BarReleased)
        .on_right_press(Message::BarMenu)
        .interaction(iced::mouse::Interaction::Pointer)
        .into()
}
