// Portions of this file are adapted from kazari (https://github.com/Santiago-0t/kazari),
// crates/kazari-app/src/main.rs: the custom iced `Theme` built from a palette, the
// `to_iced` color conversion and the closure-based container styling (rounded
// containers with a background and a border).
// Copyright (c) 2026 Santiago Otalvaro. Licensed under the MIT License (see NOTICE).

//! Colors, fonts and widget styles. Every widget is styled from the Tokyo Night or Tokyo
//! Night Day palette of the Omarchy panel (`taskline_core::theme`).

use iced::font::{Family, Weight};
use iced::widget::{button, container, scrollable, text_input};
use iced::{Background, Border, Color, Font, Shadow, Theme};
use taskline_core::theme::{Mode, Palette, Rgb};

pub const MONO: Font = Font {
    family: Family::Name("JetBrains Mono"),
    ..Font::DEFAULT
};
pub const MONO_BOLD: Font = Font {
    weight: Weight::Bold,
    ..MONO
};

pub const FONT_REGULAR: &[u8] = include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf");
pub const FONT_BOLD: &[u8] = include_bytes!("../assets/fonts/JetBrainsMono-Bold.ttf");

// Type scale of the Omarchy panel (Style.font.*).
pub const CAPTION: f32 = 11.0;
pub const BODY: f32 = 13.0;
pub const TITLE: f32 = 14.0;
pub const HEADING: f32 = 17.0;
pub const DISPLAY: f32 = 24.0;
pub const DISPLAY_LARGE: f32 = 30.0;

pub fn to_iced(c: Rgb) -> Color {
    Color::from_rgb8(c.0, c.1, c.2)
}

pub fn alpha(c: Rgb, a: f32) -> Color {
    Color { a, ..to_iced(c) }
}

/// `c` faded towards `bg` (icons are tinted with opaque colors, so fading is a mix).
pub fn mix(c: Rgb, bg: Rgb, weight: f32) -> Color {
    let blend = |a: u8, b: u8| (a as f32 * weight + b as f32 * (1.0 - weight)).round() as u8;
    Color::from_rgb8(blend(c.0, bg.0), blend(c.1, bg.1), blend(c.2, bg.2))
}

/// The app wears the palette of the chosen mode.
pub fn theme(mode: Mode) -> Theme {
    let p = mode.palette();
    Theme::custom(
        match mode {
            Mode::Dark => "Tokyo Night",
            Mode::Light => "Tokyo Night Day",
        },
        iced::theme::Palette {
            background: to_iced(p.bg),
            text: to_iced(p.fg),
            primary: to_iced(p.accent),
            success: to_iced(p.green),
            warning: to_iced(p.yellow),
            danger: to_iced(p.red),
        },
    )
}

pub fn background(p: &'static Palette) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(Background::Color(to_iced(p.bg))),
        text_color: Some(to_iced(p.fg)),
        ..Default::default()
    }
}

/// A popover surface (calendar, priority menu): opaque background and a border.
///
/// No blurred shadow: iced's software renderer draws shadows outside the damage clip,
/// so every partial repaint (hovering a day) stacked another shadow on top until the
/// calendar turned black (v2.0.0 on Windows). `popover_ring` adds a soft outline
/// instead, drawn as an ordinary clipped border.
pub fn popover(p: &'static Palette) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(Background::Color(to_iced(p.bg))),
        border: Border::default().rounded(10).color(to_iced(p.border)).width(1),
        shadow: Shadow::default(),
        text_color: Some(to_iced(p.fg)),
        ..Default::default()
    }
}

/// The outline around a popover: a translucent ring, safe on every renderer.
pub fn popover_ring(_p: &'static Palette) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: None,
        border: Border::default()
            .rounded(13)
            .color(Color {
                a: 0.16,
                ..Color::BLACK
            })
            .width(3),
        shadow: Shadow::default(),
        ..Default::default()
    }
}

/// The hover hint in the panel's own style (`Tip.qml`).
pub fn tip(p: &'static Palette) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(Background::Color(to_iced(p.surface))),
        border: Border::default().rounded(6).color(to_iced(p.border)).width(1),
        text_color: Some(to_iced(p.fg)),
        ..Default::default()
    }
}

/// A row of the task list: accent tint while hovered.
pub fn row(p: &'static Palette, hovered: bool) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: hovered.then(|| Background::Color(alpha(p.accent, 0.12))),
        border: Border::default().rounded(8),
        ..Default::default()
    }
}

/// A flat clickable area whose background follows hover (chips, menu options, days).
pub fn flat(
    p: &'static Palette,
    rest: f32,
    hover: f32,
    radius: f32,
) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let strength = match status {
            button::Status::Hovered | button::Status::Pressed => hover,
            _ => rest,
        };
        button::Style {
            background: (strength > 0.0).then(|| Background::Color(alpha(p.accent, strength))),
            text_color: to_iced(p.fg),
            border: Border::default().rounded(radius),
            shadow: Shadow::default(),
            snap: true,
        }
    }
}

/// A bare icon button (edit, delete, close, arrows): no background at all.
pub fn bare(p: &'static Palette) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, _| button::Style {
        background: None,
        text_color: to_iced(p.fg),
        border: Border::default(),
        shadow: Shadow::default(),
        snap: true,
    }
}

/// The text fields of the Omarchy panel: a faint fill, a thin border, accent on focus.
pub fn field(p: &'static Palette) -> impl Fn(&Theme, text_input::Status) -> text_input::Style {
    move |_, status| {
        let focused = matches!(status, text_input::Status::Focused { .. });
        let hovered = matches!(status, text_input::Status::Hovered);
        text_input::Style {
            background: Background::Color(alpha(p.surface, 0.55)),
            border: Border::default().rounded(4).width(1).color(if focused {
                to_iced(p.accent)
            } else if hovered {
                alpha(p.accent, 0.6)
            } else {
                to_iced(p.border)
            }),
            icon: to_iced(p.muted),
            placeholder: to_iced(p.muted),
            value: to_iced(p.fg),
            selection: alpha(p.accent, 0.35),
        }
    }
}

pub fn scrollbar(p: &'static Palette) -> impl Fn(&Theme, scrollable::Status) -> scrollable::Style {
    move |theme, status| {
        let mut style = scrollable::default(theme, status);
        let rail = scrollable::Rail {
            background: None,
            border: Border::default(),
            scroller: scrollable::Scroller {
                background: Background::Color(alpha(p.track, 0.6)),
                border: Border::default().rounded(3),
            },
        };
        style.vertical_rail = rail;
        style.horizontal_rail = rail;
        style
    }
}
