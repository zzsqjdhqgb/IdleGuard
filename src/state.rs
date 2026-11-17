//! State shared by the main window, the overlay, and the timer task.
//!
//! All access happens on the GPUI foreground thread; `Cell`/`RefCell` keep the
//! synchronization explicit without locking.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::time::Instant;

use crate::config::Settings;

pub struct Shared {
    pub config_path: PathBuf,
    pub settings: RefCell<Settings>,
    pub launch: Instant,
    pub main_hwnd: Cell<isize>,
    pub overlay_hwnd: Cell<isize>,
    pub exiting: Cell<bool>,
    pub app_icon: Cell<isize>,
}

impl Shared {
    pub fn main_hwnd(&self) -> windows::Win32::Foundation::HWND {
        crate::platform::window::hwnd_from_isize(self.main_hwnd.get())
    }

    pub fn overlay_hwnd(&self) -> windows::Win32::Foundation::HWND {
        crate::platform::window::hwnd_from_isize(self.overlay_hwnd.get())
    }
}
