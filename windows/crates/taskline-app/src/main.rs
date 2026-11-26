//! Taskline for Windows: an always-on-top progress bar, a task panel and a tray icon,
//! in Rust with iced. The interface is based on kazari (MIT, Santiago Otalvaro) and
//! wears the Tokyo Night palettes of the Omarchy panel.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod bar;
mod icons;
mod panel;
mod platform;
mod selftest;
mod shapes;
mod style;

use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::Ordering;

use iced::window;
use taskline_core::settings::{Settings, data_dir};
use taskline_core::store::TaskStore;

use app::{App, Boot, Message};

const LOGO_PNG: &[u8] = include_bytes!("../assets/logo-64.png");

const USAGE: &str = "Usage: taskline [--version] [--selftest REPORT [--screenshots DIR]]";

fn main() {
    if std::env::args().len() > 1 {
        attach_console();
    }
    let mut args = std::env::args().skip(1);
    let mut report: Option<PathBuf> = None;
    let mut screenshots: Option<PathBuf> = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--version" | "-V" => {
                println!("taskline {}", taskline_core::VERSION);
                return;
            }
            "--help" | "-h" => {
                println!("{USAGE}");
                return;
            }
            "--selftest" => report = args.next().map(PathBuf::from),
            "--screenshots" => screenshots = args.next().map(PathBuf::from),
            other => {
                eprintln!("taskline: unknown argument '{other}'\n{USAGE}");
                std::process::exit(2);
            }
        }
    }

    let texts = platform::language().texts();
    let boot = match report {
        Some(report) => {
            // A throwaway data folder: the selftest never sees real tasks or settings.
            let data = std::env::temp_dir().join(format!("taskline-selftest-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&data);
            std::fs::create_dir_all(&data).expect("temporary data folder");
            // SAFETY: single-threaded at this point; read by data_dir() below.
            unsafe { std::env::set_var("TASKLINE_DATA_DIR", &data) };
            let store = selftest::demo_store(&data);
            let settings = Settings::load(data.join("settings.json"));
            Boot {
                store,
                settings,
                texts,
                with_tray: true,
                selftest: Some(selftest::Selftest::new(report, screenshots, data)),
            }
        }
        None => {
            if !platform::single_instance() {
                return;
            }
            let dir = data_dir();
            Boot {
                store: TaskStore::open_system(dir.join("tasks.json"), taskline_core::VERSION),
                settings: Settings::load(dir.join("settings.json")),
                texts,
                with_tray: true,
                selftest: None,
            }
        }
    };
    let selftest = boot.selftest.is_some();

    let boot = Mutex::new(Some(boot));
    let result = iced::daemon(
        move || App::new(boot.lock().expect("boot").take().expect("booted once")),
        App::update,
        view,
    )
    .title(|_: &App, _| String::from("Taskline"))
    .theme(|app: &App, _| style::theme(app.mode()))
    .subscription(App::subscription)
    .font(style::FONT_REGULAR)
    .font(style::FONT_BOLD)
    .default_font(style::MONO)
    .antialiasing(true)
    .run();

    if let Err(error) = result {
        eprintln!("taskline: {error}");
        if selftest {
            std::process::exit(1);
        }
    }
    if selftest {
        std::process::exit(selftest::EXIT_CODE.load(Ordering::SeqCst));
    }
}

fn view(app: &App, id: window::Id) -> iced::Element<'_, Message> {
    if Some(id) == app.panel {
        panel::view(app, app.panel_size.width, app.panel_size.height)
    } else {
        bar::view(app)
    }
}

/// A GUI exe has no console: print `--version` and errors to the one it started from.
fn attach_console() {
    #[cfg(windows)]
    // SAFETY: plain call; failing (no parent console) is harmless.
    unsafe {
        windows_sys::Win32::System::Console::AttachConsole(u32::MAX);
    }
}

/// The logo as RGBA pixels (for the tray and window icons).
pub fn logo_rgba() -> (Vec<u8>, u32) {
    let decoder = png::Decoder::new(std::io::Cursor::new(LOGO_PNG));
    let mut reader = decoder.read_info().expect("bundled logo");
    let mut buffer = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buffer).expect("bundled logo");
    buffer.truncate(info.buffer_size());
    let rgba = match info.color_type {
        png::ColorType::Rgba => buffer,
        png::ColorType::Rgb => buffer
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|c| [c[0], c[1], c[2], 255])
            .collect(),
        _ => [0x7a, 0xa2, 0xf7, 255].repeat((info.width * info.height) as usize),
    };
    (rgba, info.width)
}

pub fn window_icon() -> Option<window::Icon> {
    let (rgba, size) = logo_rgba();
    window::icon::from_rgba(rgba, size, size).ok()
}

pub fn platform_settings(bar: bool) -> window::settings::PlatformSpecific {
    #[cfg(windows)]
    {
        window::settings::PlatformSpecific {
            skip_taskbar: bar,
            undecorated_shadow: !bar,
            corner_preference: window::settings::platform::CornerPreference::Round,
            ..Default::default()
        }
    }
    #[cfg(not(windows))]
    {
        let _ = bar;
        window::settings::PlatformSpecific::default()
    }
}

/// The HWND of a window, for the native context menu.
pub fn raw_hwnd(window: &dyn window::Window) -> isize {
    #[cfg(windows)]
    {
        use raw_window_handle::RawWindowHandle;
        match window.window_handle().map(|h| h.as_raw()) {
            Ok(RawWindowHandle::Win32(handle)) => handle.hwnd.get(),
            _ => 0,
        }
    }
    #[cfg(not(windows))]
    {
        let _ = window;
        0
    }
}

#[cfg(test)]
mod preview {
    //! Renders the panel and the bar headlessly (no window, no desktop) into PNGs:
    //! `TASKLINE_PREVIEW_DIR=/some/dir cargo test -p taskline preview -- --nocapture`.

    use iced::Size;
    use iced_test::simulator::Simulator;

    use super::*;

    fn app(dir: &std::path::Path, language: &str) -> App {
        let store = selftest::demo_store(dir);
        let settings = Settings::load(dir.join("settings.json"));
        let texts = taskline_core::i18n::Language::from_tag(language).texts();
        let (mut app, _) = App::new(Boot {
            store,
            settings,
            texts,
            with_tray: false,
            selftest: None,
        });
        app.panel = Some(window::Id::unique());
        app
    }

    /// The selftest's interaction checks (add, due, priority, sort, calendar keys, edit,
    /// theme), through the app's messages, without windows.
    #[test]
    fn interaction_logic() {
        let tmp = std::env::temp_dir().join(format!("taskline-logic-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let mut app = app(&tmp, "es");
        app.selftest = Some(selftest::Selftest::new(tmp.join("report.txt"), None, tmp.clone()));
        selftest::logic(&mut app);
        let failures: Vec<String> = app
            .selftest
            .as_ref()
            .unwrap()
            .failures()
            .iter()
            .map(|s| s.to_string())
            .collect();
        let ran = app
            .selftest
            .as_ref()
            .unwrap()
            .lines()
            .iter()
            .any(|l| l.starts_with("theme:"));
        let _ = std::fs::remove_dir_all(&tmp);
        assert!(failures.is_empty(), "{failures:#?}");
        assert!(ran, "every check ran");
    }

    #[test]
    fn render_previews() {
        let Ok(out) = std::env::var("TASKLINE_PREVIEW_DIR") else {
            return;
        };
        let out = PathBuf::from(out);
        let tmp = std::env::temp_dir().join(format!("taskline-preview-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        use taskline_core::theme::Choice::{Dark, Light};
        let scenes = [
            ("es", Dark, "calendar"),
            ("en", Light, "calendar"),
            ("es", Dark, "list"),
            ("en", Dark, "edit"),
            ("es", Light, "list"),
        ];
        for (language, mode, scene) in scenes {
            let mut app = app(&tmp, language);
            let _ = app.update(Message::SetTheme(mode));
            let second = app.store.today(app.sort).get(1).map(|t| t.uuid().to_owned());
            app.hovered = second.clone();
            match scene {
                "calendar" => {
                    let _ = app.update(Message::OpenCalendar(app::Target::New));
                }
                "edit" => {
                    let _ = app.update(Message::StartEdit(second.unwrap()));
                    let _ = app.update(Message::OpenPriority(app::Target::Edit));
                }
                _ => {}
            }
            let settings = iced::Settings {
                fonts: vec![style::FONT_REGULAR.into(), style::FONT_BOLD.into()],
                default_font: style::MONO,
                ..Default::default()
            };
            let theme = style::theme(app.mode());
            let name = format!("panel-{language}-{}-{scene}.png", mode.key());
            let path = out.join(&name);
            let _ = std::fs::remove_file(&path);
            let mut ui = Simulator::with_size(
                settings.clone(),
                Size::new(875.0, 600.0),
                panel::view(&app, 875.0, 600.0),
            );
            let snapshot = ui.snapshot(&theme).unwrap();
            let _ = snapshot.matches_image(&path);
            let bar_path = out.join(format!("bar-{}.png", mode.key()));
            let _ = std::fs::remove_file(&bar_path);
            let mut ui = Simulator::with_size(settings, app.bar_size(), bar::view(&app));
            let _ = ui.snapshot(&theme).unwrap().matches_image(&bar_path);
            println!("{}", path.display());
            let _ = std::fs::remove_dir_all(&tmp);
            std::fs::create_dir_all(&tmp).unwrap();
        }
    }
}
