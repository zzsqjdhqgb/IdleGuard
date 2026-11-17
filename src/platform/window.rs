//! Thin, centralized Win32 window control.
//!
//! GPUI owns the actual window procedure and rendering. This module only
//! adjusts the window styles after creation and wraps the show/hide/topmost
//! operations needed by the tray and overlay logic.

use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::{Mutex, OnceLock};

use gpui::Window;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    CallWindowProcW, DefWindowProcW, GWL_EXSTYLE, GWLP_WNDPROC, GetWindowLongPtrW, HTTRANSPARENT,
    HWND_TOPMOST, ICON_BIG, ICON_SMALL, IsIconic, IsWindowVisible, MA_NOACTIVATE, MB_ICONERROR,
    MB_OK, MessageBoxW, SPI_GETWORKAREA, SW_HIDE, SW_RESTORE, SW_SHOW, SW_SHOWNOACTIVATE,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW, SendMessageW, SetForegroundWindow,
    SetWindowLongPtrW, SetWindowPos, ShowWindow, SystemParametersInfoW, WM_MOUSEACTIVATE,
    WM_NCHITTEST, WM_SETICON, WNDPROC, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT,
};

/// Original GPUI window procedures for sub-classed overlay windows, keyed by
/// the raw HWND. The subclass only overrides hit testing/activation and
/// forwards everything else to GPUI.
static OVERLAY_PROCEDURES: OnceLock<Mutex<HashMap<isize, WNDPROC>>> = OnceLock::new();

fn overlay_procedures() -> &'static Mutex<HashMap<isize, WNDPROC>> {
    OVERLAY_PROCEDURES.get_or_init(|| Mutex::new(HashMap::new()))
}

unsafe extern "system" fn overlay_window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        // Even though WS_EX_TRANSPARENT is set, returning HTTRANSPARENT makes
        // hit testing explicit and covers windows from this process beneath
        // the overlay.
        WM_NCHITTEST => return LRESULT(HTTRANSPARENT as isize),
        WM_MOUSEACTIVATE => return LRESULT(MA_NOACTIVATE as isize),
        _ => {}
    }

    let previous = overlay_procedures()
        .lock()
        .ok()
        .and_then(|procedures| procedures.get(&(hwnd.0 as isize)).copied());
    match previous {
        Some(previous) => unsafe { CallWindowProcW(previous, hwnd, message, wparam, lparam) },
        None => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}

/// Extract the native HWND from a GPUI window without changing its ownership.
pub fn hwnd_of(window: &Window) -> Option<HWND> {
    let handle = <Window as HasWindowHandle>::window_handle(window).ok()?;
    match handle.as_raw() {
        RawWindowHandle::Win32(win32) => Some(HWND(win32.hwnd.get() as *mut c_void)),
        _ => None,
    }
}

/// Reconstruct an HWND that was stored as an `isize` for cross-thread command
/// passing. Native window handles are valid for the process lifetime.
pub fn hwnd_from_isize(raw: isize) -> HWND {
    HWND(raw as *mut c_void)
}

pub fn hwnd_to_isize(hwnd: HWND) -> isize {
    hwnd.0 as isize
}

pub fn is_visible(hwnd: HWND) -> bool {
    unsafe { IsWindowVisible(hwnd).as_bool() }
}

/// Whether the main window should be considered "on screen". A minimized
/// window is visible at the Win32 level, but the tray toggle should restore it
/// rather than hide it.
pub fn is_main_shown(hwnd: HWND) -> bool {
    is_visible(hwnd) && !unsafe { IsIconic(hwnd).as_bool() }
}

/// Hide the main window without destroying it. Used by the close button and by
/// the tray's toggle action.
pub fn hide_window(hwnd: HWND) {
    unsafe {
        let _ = ShowWindow(hwnd, SW_HIDE);
    }
}

/// Show the main window, activate it, and bring it to the foreground.
pub fn show_main_window(hwnd: HWND) {
    unsafe {
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        } else {
            let _ = ShowWindow(hwnd, SW_SHOW);
        }
        let _ = SetForegroundWindow(hwnd);
    }
}

/// Toggle the main window, matching the tray's left-click behavior.
pub fn toggle_main_window(hwnd: HWND) {
    if is_main_shown(hwnd) {
        hide_window(hwnd);
    } else {
        show_main_window(hwnd);
    }
}

/// Apply the click-through, no-activation, tool-window styles to the overlay.
///
/// The window is also marked topmost. `WS_EX_TRANSPARENT` makes the whole
/// window transparent to hit testing, so both mouse clicks and the cursor pass
/// through to the window underneath. `WS_EX_NOACTIVATE` keeps it out of the
/// focus chain; `WS_EX_TOOLWINDOW` keeps it out of Alt+Tab and the taskbar.
pub fn configure_overlay(hwnd: HWND) {
    unsafe {
        let current = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let extra = (WS_EX_NOACTIVATE | WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW).0 as isize;
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, current | extra);

        let already_subclassed = overlay_procedures()
            .lock()
            .map(|procedures| procedures.contains_key(&(hwnd.0 as isize)))
            .unwrap_or(false);
        if !already_subclassed {
            let previous = SetWindowLongPtrW(
                hwnd,
                GWLP_WNDPROC,
                overlay_window_proc as *const () as isize,
            );
            if previous != 0 {
                let previous: WNDPROC = std::mem::transmute(previous);
                if let Ok(mut procedures) = overlay_procedures().lock() {
                    procedures.insert(hwnd.0 as isize, previous);
                }
            }
        }

        let _ = SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        );
    }
}

/// Position the hidden overlay in physical pixels near the bottom of the
/// primary work area. GPUI's window bounds are logical, so this avoids relying
/// on placement of a window that was created hidden at non-100% DPI.
pub fn place_overlay_bottom(hwnd: HWND, logical_width: f32, logical_height: f32) {
    unsafe {
        let dpi = GetDpiForWindow(hwnd).max(96);
        let scale = dpi as f32 / 96.0;
        let width = (logical_width * scale).round() as i32;
        let height = (logical_height * scale).round() as i32;

        let mut work_area = RECT::default();
        let mut have_work_area = false;
        if SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            Some(&mut work_area as *mut RECT as *mut std::ffi::c_void),
            Default::default(),
        )
        .is_ok()
        {
            have_work_area = work_area.right > work_area.left;
        }
        if !have_work_area {
            work_area = RECT {
                left: 0,
                top: 0,
                right: width,
                bottom: height + (28.0 * scale) as i32,
            };
        }

        let work_width = work_area.right - work_area.left;
        let x = work_area.left + (work_width - width) / 2;
        let y = work_area.bottom - height - (24.0 * scale) as i32;
        let _ = SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            x,
            y,
            width,
            height,
            SWP_NOACTIVATE,
        );
    }
}

/// Present the overlay without activating it. Also refreshes topmost status, in
/// case another topmost window appeared since the last display.
pub fn show_overlay(hwnd: HWND) {
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        let _ = SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
    }
}

/// Hide the overlay. This never changes which application owns keyboard focus.
pub fn hide_overlay(hwnd: HWND) {
    unsafe {
        let _ = ShowWindow(hwnd, SW_HIDE);
    }
}

/// Assign the application icon to an already-created GPUI window.
pub fn set_window_icon(hwnd: HWND, icon: windows::Win32::UI::WindowsAndMessaging::HICON) {
    if icon.is_invalid() {
        return;
    }
    unsafe {
        SendMessageW(
            hwnd,
            WM_SETICON,
            Some(windows::Win32::Foundation::WPARAM(ICON_BIG as usize)),
            Some(windows::Win32::Foundation::LPARAM(icon.0 as isize)),
        );
        SendMessageW(
            hwnd,
            WM_SETICON,
            Some(windows::Win32::Foundation::WPARAM(ICON_SMALL as usize)),
            Some(windows::Win32::Foundation::LPARAM(icon.0 as isize)),
        );
    }
}

/// A blocking error dialog used only for startup failures where no UI can be
/// created.
pub fn show_error_dialog(title: &str, message: &str) {
    use windows::core::HSTRING;
    let title = HSTRING::from(title);
    let message = HSTRING::from(message);
    unsafe {
        MessageBoxW(None, &message, &title, MB_OK | MB_ICONERROR);
    }
}

/// Find a hidden or visible top-level window by its exact title. Used only for
/// the single-instance hand-off.
pub fn find_window_by_title(title: &str) -> Option<HWND> {
    use windows::Win32::UI::WindowsAndMessaging::FindWindowW;
    use windows::core::HSTRING;
    let title = HSTRING::from(title);
    unsafe { FindWindowW(None, &title) }.ok()
}
