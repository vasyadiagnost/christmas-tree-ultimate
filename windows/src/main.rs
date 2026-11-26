#![windows_subsystem = "windows"]

mod app;
mod audio;
mod christmas;
mod decorations;
mod desktop;
mod halloween;
mod light;
mod log;
mod menu;
mod prefs;
mod render;
mod screen;
mod snow;
mod tracker;
mod welcome;

use windows::Win32::UI::HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext};
use windows::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW};
use windows::core::{HSTRING, w};

fn main() {
    christmas_tree_ultimate::language::choose();
    // The installer registers this command to remove this app, including a running copy.
    if std::env::args().skip(1).any(|argument| argument == "--uninstall") {
        if let Err(error) = christmas_tree_ultimate::install::uninstall() {
            if !std::env::args().skip(1).any(|argument| argument.eq_ignore_ascii_case("--silent") || argument.eq_ignore_ascii_case("/s")) {
                startup_error(&error);
            }
            std::process::exit(1);
        }
        return;
    }
    let _instance = match christmas_tree_ultimate::identity::InstanceGuard::app() {
        Ok(Some(guard)) => guard,
        Ok(None) => return,
        Err(error) => {
            startup_error(&error);
            return;
        }
    };
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
    log::start();
    log!("{} {}", snowhush_core::text::tr("app.name"), env!("CARGO_PKG_VERSION"));
    if let Err(error) = christmas_tree_ultimate::identity::migrate_settings() {
        log!("settings migration failed: {error}");
        startup_error(&error);
        return;
    }
    if let Err(error) = app::run() {
        log!("failed: {error}");
    }
}

fn startup_error(error: &str) {
    let text = HSTRING::from(snowhush_core::text::fill("startup.failed", &[("error", error)]));
    unsafe {
        MessageBoxW(None, &text, w!("christmas tree ultimate"), MB_OK | MB_ICONERROR);
    }
}
