//! Lucide icons (ISC, see assets/icons/LICENSE-lucide.txt), embedded in the exe and
//! tinted with the palette.

use std::sync::LazyLock;

use iced::widget::svg::{self, Svg};
use iced::{Color, Length};

macro_rules! icon {
    ($name:ident, $file:literal) => {
        pub static $name: LazyLock<svg::Handle> = LazyLock::new(|| {
            svg::Handle::from_memory(include_bytes!(concat!("../assets/icons/", $file)).as_slice())
        });
    };
}

icon!(CALENDAR, "calendar.svg");
icon!(EDIT, "square-pen.svg");
icon!(TRASH, "trash-2.svg");
icon!(SUN, "sun.svg");
icon!(MOON, "moon.svg");
icon!(CLOSE, "x.svg");
icon!(SORT, "arrow-up-down.svg");
icon!(PREV, "chevron-left.svg");
icon!(NEXT, "chevron-right.svg");
icon!(HIGH, "priority-high.svg");
icon!(MEDIUM, "priority-medium.svg");
icon!(LOW, "priority-low.svg");

/// An icon of `size` logical pixels in `color`.
pub fn icon<'a>(handle: &svg::Handle, size: f32, color: Color) -> Svg<'a> {
    Svg::new(handle.clone())
        .width(Length::Fixed(size))
        .height(Length::Fixed(size))
        .style(move |_, _| svg::Style { color: Some(color) })
}

/// The priority icon handle (chevrons-up, equal, chevron-down).
pub fn priority(priority: &str) -> Option<&'static svg::Handle> {
    match priority {
        "H" => Some(&HIGH),
        "M" => Some(&MEDIUM),
        "L" => Some(&LOW),
        _ => None,
    }
}
