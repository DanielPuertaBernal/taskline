//! Taskline core: the task model, storage, calendar, texts, palettes and updater logic.
//!
//! Everything here is plain Rust without a window, so it is tested on any OS. The GUI in
//! `taskline-app` only collects input and draws this state.

pub mod calendar;
pub mod dates;
pub mod i18n;
pub mod input;
pub mod settings;
pub mod store;
pub mod theme;
pub mod updater;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
