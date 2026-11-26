//! Windows integration: single instance, Start with Windows, display language, dialogs,
//! the tray icon and its menu, and the detached update script. On other systems these
//! are no-ops so the app can be developed and its screens rendered on Linux.

use std::path::PathBuf;

use taskline_core::i18n::{Language, Ui};

/// The `Run` value name v1.x used; it must not change, or Windows would start an old
/// path (or nothing) after an update.
pub const RUN_VALUE: &str = "Taskline";
pub const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

/// `TASKLINE_LANG`, else the Windows display language, else `LANG`.
pub fn language() -> Language {
    if let Ok(tag) = std::env::var("TASKLINE_LANG")
        && !tag.is_empty()
    {
        return Language::from_tag(&tag);
    }
    #[cfg(windows)]
    {
        // SAFETY: plain query without arguments.
        let langid = unsafe { windows_sys::Win32::Globalization::GetUserDefaultUILanguage() };
        Language::from_langid(langid)
    }
    #[cfg(not(windows))]
    {
        Language::from_tag(&std::env::var("LANG").unwrap_or_default())
    }
}

/// The command line stored in the Run key: the quoted path of this exe (as v1.x).
pub fn launch_command() -> String {
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("taskline.exe"));
    format!("\"{}\"", exe.display())
}

/// HTTPS (or plain HTTP for the selftest's local server) GET with a size limit.
pub fn http_get(url: &str) -> Result<Vec<u8>, String> {
    use std::io::Read;
    use ureq::tls::{RootCerts, TlsConfig, TlsProvider};
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(30)))
        .user_agent("taskline-updater")
        .tls_config(
            TlsConfig::builder()
                .provider(TlsProvider::NativeTls)
                .root_certs(RootCerts::PlatformVerifier)
                .build(),
        )
        .build()
        .into();
    let response = agent
        .get(url)
        .header("Accept", "application/vnd.github+json, */*")
        .call()
        .map_err(|e| e.to_string())?;
    let mut data = Vec::new();
    response
        .into_body()
        .into_reader()
        .take(taskline_core::updater::MAX_DOWNLOAD + 1)
        .read_to_end(&mut data)
        .map_err(|e| e.to_string())?;
    if data.len() as u64 > taskline_core::updater::MAX_DOWNLOAD {
        return Err(format!("{url} is larger than expected"));
    }
    Ok(data)
}

#[cfg(windows)]
mod imp {
    use std::os::windows::process::CommandExt;
    use std::path::{Path, PathBuf};

    use windows_sys::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError};
    use windows_sys::Win32::Globalization::{CP_ACP, WideCharToMultiByte};
    use windows_sys::Win32::System::Threading::CreateMutexW;
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE};

    use super::{RUN_KEY, RUN_VALUE};

    /// False when another Taskline already runs (same mutex name as v1.x).
    pub fn single_instance() -> bool {
        let name: Vec<u16> = "Local\\TasklineSingleInstance\0".encode_utf16().collect();
        // SAFETY: a valid, NUL-terminated name; the handle is kept for the process lifetime.
        unsafe {
            let handle = CreateMutexW(std::ptr::null(), 0, name.as_ptr());
            !handle.is_null() && GetLastError() != ERROR_ALREADY_EXISTS
        }
    }

    pub fn autostart_enabled() -> bool {
        RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey_with_flags(RUN_KEY, KEY_READ)
            .and_then(|key| key.get_value::<String, _>(RUN_VALUE))
            .is_ok()
    }

    pub fn autostart_command() -> Option<String> {
        RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey_with_flags(RUN_KEY, KEY_READ)
            .and_then(|key| key.get_value::<String, _>(RUN_VALUE))
            .ok()
    }

    pub fn set_autostart(enabled: bool) -> Result<(), String> {
        let (key, _) = RegKey::predef(HKEY_CURRENT_USER)
            .create_subkey(RUN_KEY)
            .map_err(|e| e.to_string())?;
        let key = RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey_with_flags(RUN_KEY, KEY_SET_VALUE | KEY_READ)
            .unwrap_or(key);
        if enabled {
            key.set_value(RUN_VALUE, &super::launch_command())
                .map_err(|e| e.to_string())
        } else {
            match key.delete_value(RUN_VALUE) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
                _ => Ok(()),
            }
        }
    }

    /// The batch file in the ANSI code page, like v1.x (`mbcs`), so cmd reads paths
    /// with accents correctly.
    fn ansi(text: &str) -> Vec<u8> {
        let wide: Vec<u16> = text.encode_utf16().collect();
        if wide.is_empty() {
            return Vec::new();
        }
        // SAFETY: lengths come from the buffers passed.
        unsafe {
            let len = WideCharToMultiByte(
                CP_ACP,
                0,
                wide.as_ptr(),
                wide.len() as i32,
                std::ptr::null_mut(),
                0,
                std::ptr::null(),
                std::ptr::null_mut(),
            );
            let mut out = vec![0u8; len.max(0) as usize];
            WideCharToMultiByte(
                CP_ACP,
                0,
                wide.as_ptr(),
                wide.len() as i32,
                out.as_mut_ptr(),
                len,
                std::ptr::null(),
                std::ptr::null_mut(),
            );
            out
        }
    }

    pub fn write_script(text: &str, directory: &Path) -> Result<PathBuf, String> {
        let (path, file) = taskline_core::store::tempfile_in(directory, "taskline-update-", ".cmd")
            .map_err(|e| e.to_string())?;
        drop(file);
        std::fs::write(&path, ansi(text)).map_err(|e| e.to_string())?;
        Ok(path)
    }

    /// Starts the batch file detached with a hidden console, so it outlives us.
    pub fn launch_script(script: &Path) -> Result<(), String> {
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        std::process::Command::new("cmd.exe")
            .args(["/d", "/c"])
            .arg(script)
            .current_dir(script.parent().unwrap_or(Path::new(".")))
            .creation_flags(CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    pub fn message(level: rfd::MessageLevel, text: &str) {
        let _ = rfd::MessageDialog::new()
            .set_title("Taskline")
            .set_description(text)
            .set_level(level)
            .set_buttons(rfd::MessageButtons::Ok)
            .show();
    }

    /// Top-left corner of a window on screen (physical pixels) and its DPI scale.
    pub fn window_origin(hwnd: isize) -> Option<(i32, i32, f32)> {
        use windows_sys::Win32::Foundation::RECT;
        use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
        use windows_sys::Win32::UI::WindowsAndMessaging::GetWindowRect;
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        // SAFETY: a live HWND of ours and a valid RECT.
        unsafe {
            if GetWindowRect(hwnd as _, &mut rect) == 0 {
                return None;
            }
            let dpi = GetDpiForWindow(hwnd as _).max(96);
            Some((rect.left, rect.top, dpi as f32 / 96.0))
        }
    }

    /// Moves the real mouse pointer (selftest only), so the window gets hover events.
    pub fn move_cursor(x: i32, y: i32) {
        // SAFETY: plain call.
        unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::SetCursorPos(x, y);
        }
    }

    /// A real left click where the pointer is (selftest only; move it first and let the
    /// window see the move, or the press lands where the pointer was before).
    pub fn click() {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
            MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, mouse_event,
        };
        // SAFETY: plain calls.
        unsafe {
            mouse_event(MOUSEEVENTF_LEFTDOWN, 0, 0, 0, 0);
            mouse_event(MOUSEEVENTF_LEFTUP, 0, 0, 0, 0);
        }
    }

    /// What is on screen inside a window right now (RGBA), as the user sees it, and
    /// not a fresh render: partial-redraw defects show up here.
    pub fn capture_window(hwnd: isize) -> Option<(u32, u32, Vec<u8>)> {
        use windows_sys::Win32::Foundation::RECT;
        use windows_sys::Win32::Graphics::Gdi::*;
        use windows_sys::Win32::UI::WindowsAndMessaging::GetWindowRect;
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        // SAFETY: standard GDI screen copy; every handle is released.
        unsafe {
            if GetWindowRect(hwnd as _, &mut rect) == 0 {
                return None;
            }
            let (w, h) = (rect.right - rect.left, rect.bottom - rect.top);
            if w <= 0 || h <= 0 {
                return None;
            }
            let screen = GetDC(std::ptr::null_mut());
            let memory = CreateCompatibleDC(screen);
            let bitmap = CreateCompatibleBitmap(screen, w, h);
            let old = SelectObject(memory, bitmap as _);
            BitBlt(memory, 0, 0, w, h, screen, rect.left, rect.top, SRCCOPY);
            let mut info: BITMAPINFO = std::mem::zeroed();
            info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
            info.bmiHeader.biWidth = w;
            info.bmiHeader.biHeight = -h;
            info.bmiHeader.biPlanes = 1;
            info.bmiHeader.biBitCount = 32;
            info.bmiHeader.biCompression = BI_RGB;
            let mut bgra = vec![0u8; (w * h * 4) as usize];
            let lines = GetDIBits(
                memory,
                bitmap,
                0,
                h as u32,
                bgra.as_mut_ptr() as _,
                &mut info,
                DIB_RGB_COLORS,
            );
            SelectObject(memory, old);
            DeleteObject(bitmap as _);
            DeleteDC(memory);
            ReleaseDC(std::ptr::null_mut(), screen);
            if lines == 0 {
                return None;
            }
            for px in bgra.as_chunks_mut::<4>().0 {
                px.swap(0, 2);
                px[3] = 255;
            }
            Some((w as u32, h as u32, bgra))
        }
    }

    pub fn info(text: &str) {
        message(rfd::MessageLevel::Info, text);
    }

    pub fn error(text: &str) {
        message(rfd::MessageLevel::Error, text);
    }

    pub fn warn(text: &str) {
        message(rfd::MessageLevel::Warning, text);
    }

    pub fn pick_import() -> Option<PathBuf> {
        rfd::FileDialog::new()
            .set_title("Taskwarrior JSON")
            .add_filter("JSON", &["json"])
            .add_filter("*", &["*"])
            .pick_file()
    }

    pub fn pick_export() -> Option<PathBuf> {
        rfd::FileDialog::new()
            .set_title("Taskwarrior JSON")
            .set_file_name("taskline-export.json")
            .add_filter("JSON", &["json"])
            .save_file()
    }
}

#[cfg(not(windows))]
mod imp {
    use std::path::{Path, PathBuf};

    pub fn single_instance() -> bool {
        true
    }
    pub fn autostart_enabled() -> bool {
        false
    }
    pub fn autostart_command() -> Option<String> {
        None
    }
    pub fn set_autostart(_enabled: bool) -> Result<(), String> {
        Err("Start with Windows is only available on Windows".into())
    }
    pub fn write_script(text: &str, directory: &Path) -> Result<PathBuf, String> {
        let path = directory.join("taskline-update.cmd");
        std::fs::write(&path, text).map_err(|e| e.to_string())?;
        Ok(path)
    }
    pub fn launch_script(_script: &Path) -> Result<(), String> {
        Err("updates install on Windows only".into())
    }
    pub fn window_origin(_hwnd: isize) -> Option<(i32, i32, f32)> {
        None
    }
    pub fn move_cursor(_x: i32, _y: i32) {}
    pub fn click() {}
    pub fn capture_window(_hwnd: isize) -> Option<(u32, u32, Vec<u8>)> {
        None
    }
    pub fn info(text: &str) {
        eprintln!("taskline: {text}");
    }
    pub fn error(text: &str) {
        eprintln!("taskline: {text}");
    }
    pub fn warn(text: &str) {
        eprintln!("taskline: {text}");
    }
    pub fn pick_import() -> Option<PathBuf> {
        None
    }
    pub fn pick_export() -> Option<PathBuf> {
        None
    }
}

pub use imp::*;

/// What the tray icon or the bar menu asked for.
#[cfg_attr(not(windows), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    OpenPanel,
    ToggleBar,
    Autostart,
    Import,
    Export,
    ResetPosition,
    ThemeLight,
    ThemeDark,
    ThemeSystem,
    AutoUpdate,
    CheckUpdates,
    About,
    Quit,
}

/// The checkmarks of the menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MenuState {
    pub autostart: bool,
    pub auto_update: bool,
    pub theme: Option<taskline_core::theme::Choice>,
}

#[cfg(windows)]
pub mod tray {
    use tray_icon::menu::{
        CheckMenuItem, ContextMenu, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu,
    };
    use tray_icon::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

    use super::{MenuAction, MenuState, Ui};
    use taskline_core::theme::Choice;

    /// The tray icon and the menu it shares with the bar's right click.
    pub struct Tray {
        icon: Option<TrayIcon>,
        menu: Menu,
        items: Vec<(tray_icon::menu::MenuId, MenuAction)>,
        autostart: CheckMenuItem,
        auto_update: CheckMenuItem,
        themes: [(CheckMenuItem, Choice); 3],
    }

    impl Tray {
        pub fn new(ui: &Ui, rgba: Vec<u8>, size: u32, state: MenuState) -> Tray {
            let item = |text: &str| MenuItem::new(text, true, None);
            let open = item(ui.menu_open);
            let toggle = item(ui.menu_toggle_bar);
            let autostart = CheckMenuItem::new(ui.menu_autostart, true, state.autostart, None);
            let import = item(ui.menu_import);
            let export = item(ui.menu_export);
            let reset = item(ui.menu_reset);
            let light = CheckMenuItem::new(ui.light, true, state.theme == Some(Choice::Light), None);
            let dark = CheckMenuItem::new(ui.dark, true, state.theme == Some(Choice::Dark), None);
            let system = CheckMenuItem::new(ui.menu_system, true, state.theme == Some(Choice::System), None);
            let theme =
                Submenu::with_items(ui.menu_theme, true, &[&light, &dark, &system]).expect("theme menu");
            let auto_update = CheckMenuItem::new(ui.menu_auto_update, true, state.auto_update, None);
            let check = item(ui.menu_check);
            let about = item(ui.menu_about);
            let quit = item(ui.menu_quit);
            let separator = PredefinedMenuItem::separator;
            let menu = Menu::with_items(&[
                &open,
                &toggle,
                &separator(),
                &autostart,
                &import,
                &export,
                &reset,
                &theme,
                &separator(),
                &auto_update,
                &check,
                &about,
                &separator(),
                &quit,
            ])
            .expect("tray menu");
            let items = vec![
                (open.id().clone(), MenuAction::OpenPanel),
                (toggle.id().clone(), MenuAction::ToggleBar),
                (autostart.id().clone(), MenuAction::Autostart),
                (import.id().clone(), MenuAction::Import),
                (export.id().clone(), MenuAction::Export),
                (reset.id().clone(), MenuAction::ResetPosition),
                (light.id().clone(), MenuAction::ThemeLight),
                (dark.id().clone(), MenuAction::ThemeDark),
                (system.id().clone(), MenuAction::ThemeSystem),
                (auto_update.id().clone(), MenuAction::AutoUpdate),
                (check.id().clone(), MenuAction::CheckUpdates),
                (about.id().clone(), MenuAction::About),
                (quit.id().clone(), MenuAction::Quit),
            ];
            let icon = tray_icon::Icon::from_rgba(rgba, size, size)
                .ok()
                .and_then(|icon| {
                    TrayIconBuilder::new()
                        .with_menu(Box::new(menu.clone()))
                        .with_menu_on_left_click(false)
                        .with_tooltip("Taskline")
                        .with_icon(icon)
                        .build()
                        .ok()
                });
            Tray {
                icon,
                menu,
                items,
                autostart,
                auto_update,
                themes: [
                    (light, Choice::Light),
                    (dark, Choice::Dark),
                    (system, Choice::System),
                ],
            }
        }

        pub fn set_state(&self, state: MenuState) {
            self.autostart.set_checked(state.autostart);
            self.auto_update.set_checked(state.auto_update);
            for (item, choice) in &self.themes {
                item.set_checked(state.theme == Some(*choice));
            }
        }

        pub fn set_tooltip(&self, text: &str) {
            if let Some(icon) = &self.icon {
                let _ = icon.set_tooltip(Some(text));
            }
        }

        /// Shows the same menu at the cursor, for a right click on the bar.
        pub fn show_menu_for(&self, hwnd: isize) {
            // SAFETY: `hwnd` comes from the live bar window on this (the UI) thread.
            unsafe {
                self.menu.show_context_menu_for_hwnd(hwnd, None);
            }
        }

        /// Menu choices and left clicks on the icon since the last call.
        pub fn poll(&self) -> Vec<MenuAction> {
            let mut actions = Vec::new();
            while let Ok(event) = MenuEvent::receiver().try_recv() {
                if let Some((_, action)) = self.items.iter().find(|(id, _)| *id == event.id) {
                    actions.push(*action);
                }
            }
            while let Ok(event) = TrayIconEvent::receiver().try_recv() {
                if let TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } = event
                {
                    actions.push(MenuAction::OpenPanel);
                }
            }
            actions
        }
    }
}

#[cfg(not(windows))]
pub mod tray {
    use super::{MenuAction, MenuState, Ui};

    pub struct Tray;

    impl Tray {
        pub fn new(_ui: &Ui, _rgba: Vec<u8>, _size: u32, _state: MenuState) -> Tray {
            Tray
        }
        pub fn set_state(&self, _state: MenuState) {}
        pub fn set_tooltip(&self, _text: &str) {}
        pub fn show_menu_for(&self, _hwnd: isize) {}
        pub fn poll(&self) -> Vec<MenuAction> {
            Vec::new()
        }
    }
}
