#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

#[cfg(not(windows))]
mod assets;
#[cfg(not(windows))]
mod config;
#[cfg(not(windows))]
mod timer;

#[cfg(not(windows))]
fn main() {
    eprintln!("idle-shutdown targets Windows 10/11 and must be built for a Windows target.");
}

#[cfg(windows)]
mod app;
#[cfg(windows)]
mod assets;
#[cfg(windows)]
mod config;
#[cfg(windows)]
mod diag;
#[cfg(windows)]
mod main_view;
#[cfg(windows)]
mod overlay;
#[cfg(windows)]
mod platform;
#[cfg(windows)]
mod state;
#[cfg(windows)]
mod timer;
#[cfg(windows)]
mod ui;

#[cfg(windows)]
fn main() {
    app::run();
}
