//! Process-wide single-instance guard.
//!
//! A named mutex prevents two copies from running simultaneously. A second
//! launch brings the existing main window to the foreground and exits.

use std::time::Duration;

use windows::Win32::Foundation::{
    CloseHandle, ERROR_ALREADY_EXISTS, ERROR_SUCCESS, HANDLE, SetLastError,
};
use windows::Win32::System::Threading::CreateMutexW;
use windows::core::w;

use crate::platform::window;

const MUTEX_NAME: windows::core::PCWSTR =
    w!(r"Local\IdleShutdownTool-A3E7D4E0-6A45-4C9A-9C09-0F0D5B6E7F11");
const MAIN_WINDOW_TITLE: &str = "无操作自动关机";

/// Keeps the named mutex alive for the process lifetime.
pub struct InstanceGuard {
    handle: HANDLE,
}

impl Drop for InstanceGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.handle);
        }
    }
}

/// Returns `Some(guard)` when this is the first instance. `None` means another
/// instance owns the mutex; its window has been asked to come to the front.
pub fn acquire() -> Result<Option<InstanceGuard>, String> {
    unsafe { SetLastError(ERROR_SUCCESS) };
    let handle = unsafe { CreateMutexW(None, true, MUTEX_NAME) }
        .map_err(|error| format!("无法创建单实例互斥体：{error}"))?;
    let last_error = unsafe { windows::Win32::Foundation::GetLastError() };
    if last_error == ERROR_ALREADY_EXISTS {
        unsafe {
            let _ = CloseHandle(handle);
        }
        focus_existing_instance();
        Ok(None)
    } else {
        Ok(Some(InstanceGuard { handle }))
    }
}

fn focus_existing_instance() {
    // The first instance may still be creating its window when the second one
    // starts. Retry briefly before giving up.
    for _ in 0..20 {
        if let Some(hwnd) = window::find_window_by_title(MAIN_WINDOW_TITLE) {
            window::show_main_window(hwnd);
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}
