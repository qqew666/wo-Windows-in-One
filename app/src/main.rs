#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

#[cfg(any(windows, test))]
mod license;
#[cfg(windows)]
mod native;
#[cfg(any(windows, test))]
mod profile;
#[cfg(windows)]
mod ui;

#[cfg(windows)]
fn main() -> eframe::Result {
    ui::run()
}

#[cfg(not(windows))]
fn main() {
    eprintln!("Two Windows in One запускается в Windows.");
}
