#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

#[cfg(windows)]
mod account;
#[cfg(any(windows, test))]
mod license;
#[cfg(windows)]
mod native;
#[cfg(windows)]
mod online;
#[cfg(any(windows, test))]
mod profile;
#[cfg(windows)]
mod sandbox;
#[cfg(windows)]
mod ui;

#[cfg(windows)]
fn main() -> eframe::Result {
    let _installer_lock = native::installer_lock();
    ui::run()
}

#[cfg(not(windows))]
fn main() {
    eprintln!("Two Windows in One запускается в Windows.");
}
