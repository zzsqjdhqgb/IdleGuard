//! Minimal best-effort diagnostics.
//!
//! Release builds have no console, so failures from startup, tray creation, or
//! the shutdown API are also appended to a small log next to the config file.

use std::fmt::Write as _;
use std::fs::OpenOptions;
use std::io::Write as _;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn log(message: &str) {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    let line = format!("[{timestamp}] {message}\n");

    #[cfg(debug_assertions)]
    eprint!("{line}");

    let path = crate::config::config_dir().join("idle-shutdown.log");
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = file.write_all(line.as_bytes());
    }
}

pub fn error(context: &str, error: &str) {
    let mut message = String::new();
    let _ = write!(message, "{context}: {error}");
    log(&message);
}
