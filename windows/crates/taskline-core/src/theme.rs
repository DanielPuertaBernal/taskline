//! Tokyo Night (dark) and Tokyo Night Day (light), the exact palettes of the Omarchy
//! panel (`omarchy/danso.tasks/logic.js`). Day text colors are darkened just enough for
//! WCAG AA (4.5:1) on `bg` and `surface`; `muted` is the text-safe comment color and
//! `track` the decorative one (donut, borders).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    pub const fn hex(value: u32) -> Rgb {
        Rgb((value >> 16) as u8, (value >> 8) as u8, value as u8)
    }

    pub fn to_hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.0, self.1, self.2)
    }

    fn luminance(self) -> f64 {
        let channel = |c: u8| {
            let c = c as f64 / 255.0;
            if c <= 0.03928 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(self.0) + 0.7152 * channel(self.1) + 0.0722 * channel(self.2)
    }
}

/// WCAG contrast ratio of two colors.
pub fn contrast(a: Rgb, b: Rgb) -> f64 {
    let (la, lb) = (a.luminance(), b.luminance());
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    pub bg: Rgb,
    pub surface: Rgb,
    pub border: Rgb,
    pub hover: Rgb,
    pub fg: Rgb,
    pub muted: Rgb,
    pub track: Rgb,
    pub accent: Rgb,
    pub purple: Rgb,
    pub green: Rgb,
    pub orange: Rgb,
    pub red: Rgb,
    pub yellow: Rgb,
    pub cyan: Rgb,
}

pub const TOKYO_NIGHT: Palette = Palette {
    bg: Rgb::hex(0x1a1b26),
    surface: Rgb::hex(0x24283b),
    border: Rgb::hex(0x414868),
    hover: Rgb::hex(0x292e42),
    fg: Rgb::hex(0xc0caf5),
    muted: Rgb::hex(0x8990b5),
    track: Rgb::hex(0x565f89),
    accent: Rgb::hex(0x7aa2f7),
    purple: Rgb::hex(0xbb9af7),
    green: Rgb::hex(0x9ece6a),
    orange: Rgb::hex(0xff9e64),
    red: Rgb::hex(0xf7768e),
    yellow: Rgb::hex(0xe0af68),
    cyan: Rgb::hex(0x7dcfff),
};

pub const TOKYO_NIGHT_DAY: Palette = Palette {
    bg: Rgb::hex(0xe1e2e7),
    surface: Rgb::hex(0xd0d5e3),
    border: Rgb::hex(0xa8aecb),
    hover: Rgb::hex(0xc4c8da),
    fg: Rgb::hex(0x3257ad),
    muted: Rgb::hex(0x505985),
    track: Rgb::hex(0xa8aecb),
    accent: Rgb::hex(0x1357b5),
    purple: Rgb::hex(0x7519ec),
    green: Rgb::hex(0x4a6230),
    orange: Rgb::hex(0x8d4900),
    red: Rgb::hex(0xb6093b),
    yellow: Rgb::hex(0x705631),
    cyan: Rgb::hex(0x006283),
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Light,
    Dark,
}

impl Mode {
    pub fn palette(self) -> &'static Palette {
        match self {
            Mode::Light => &TOKYO_NIGHT_DAY,
            Mode::Dark => &TOKYO_NIGHT,
        }
    }
}

impl Palette {
    /// Due badge colors: overdue red, today orange, upcoming cyan.
    pub fn due(&self, state: crate::store::DueState) -> Rgb {
        match state {
            crate::store::DueState::Overdue => self.red,
            crate::store::DueState::Today => self.orange,
            crate::store::DueState::Upcoming => self.cyan,
        }
    }

    /// Priority icon colors: high red, medium yellow, low green.
    pub fn priority(&self, priority: &str) -> Option<Rgb> {
        match priority {
            "H" => Some(self.red),
            "M" => Some(self.yellow),
            "L" => Some(self.green),
            _ => None,
        }
    }
}

/// The `theme` setting: `light`, `dark` or `system` (follow Windows, the default).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Choice {
    Light,
    Dark,
    #[default]
    System,
}

impl Choice {
    pub fn key(self) -> &'static str {
        match self {
            Choice::Light => "light",
            Choice::Dark => "dark",
            Choice::System => "system",
        }
    }

    pub fn from_key(key: Option<&str>) -> Choice {
        match key {
            Some("light") => Choice::Light,
            Some("dark") => Choice::Dark,
            _ => Choice::System,
        }
    }

    pub fn resolve(self, system_light: bool) -> Mode {
        match self {
            Choice::Light => Mode::Light,
            Choice::Dark => Mode::Dark,
            Choice::System if system_light => Mode::Light,
            Choice::System => Mode::Dark,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_colors_meet_wcag_aa() {
        for palette in [&TOKYO_NIGHT, &TOKYO_NIGHT_DAY] {
            for background in [palette.bg, palette.surface] {
                for color in [
                    palette.fg,
                    palette.muted,
                    palette.accent,
                    palette.red,
                    palette.orange,
                    palette.yellow,
                    palette.green,
                    palette.cyan,
                ] {
                    let ratio = contrast(color, background);
                    assert!(
                        ratio >= 4.5,
                        "{} on {} is {ratio:.2}",
                        color.to_hex(),
                        background.to_hex()
                    );
                }
            }
        }
    }

    #[test]
    fn palettes_match_the_omarchy_panel() {
        let logic = include_str!("../../../../omarchy/danso.tasks/logic.js");
        for (palette, name) in [(&TOKYO_NIGHT, "dark"), (&TOKYO_NIGHT_DAY, "light")] {
            let block = logic.split(&format!("{name}: {{")).nth(1).expect("palette block");
            let block = &block[..block.find('}').unwrap()];
            for (key, color) in [
                ("bg", palette.bg),
                ("surface", palette.surface),
                ("border", palette.border),
                ("hover", palette.hover),
                ("fg", palette.fg),
                ("muted", palette.muted),
                ("track", palette.track),
                ("accent", palette.accent),
                ("purple", palette.purple),
                ("green", palette.green),
                ("orange", palette.orange),
                ("red", palette.red),
                ("yellow", palette.yellow),
                ("cyan", palette.cyan),
            ] {
                let needle = format!("{key}: \"{}\"", color.to_hex());
                assert!(block.contains(&needle), "{name}.{key} differs from logic.js");
            }
        }
    }

    #[test]
    fn choice_from_settings() {
        assert_eq!(Choice::from_key(Some("light")).resolve(false), Mode::Light);
        assert_eq!(Choice::from_key(Some("dark")).resolve(true), Mode::Dark);
        assert_eq!(Choice::from_key(None).resolve(true), Mode::Light);
        assert_eq!(Choice::from_key(Some("bogus")).resolve(false), Mode::Dark);
    }
}
