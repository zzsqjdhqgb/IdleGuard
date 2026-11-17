//! Low-overhead global idle detection using `GetLastInputInfo`.
//!
//! This API reports the tick count of the most recent keyboard or mouse input
//! without inspecting, intercepting, or installing hooks for that input.

use std::mem::size_of;
use std::time::Duration;

use windows::Win32::System::SystemInformation::GetTickCount;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};

/// Raw tick count of the last input event, useful for verifying that no input
/// arrived between two checks.
pub fn last_input_tick() -> Option<u32> {
    let mut info = LASTINPUTINFO {
        cbSize: size_of::<LASTINPUTINFO>() as u32,
        dwTime: 0,
    };
    if unsafe { GetLastInputInfo(&mut info) }.as_bool() {
        Some(info.dwTime)
    } else {
        None
    }
}

/// One consistent reading of the last-input tick and the resulting idle time.
#[derive(Clone, Copy, Debug)]
pub struct IdleSnapshot {
    pub last_input_tick: u32,
    pub idle: Duration,
}

/// Milliseconds since the last input event. `None` indicates that Windows
/// could not provide the information; callers should fail safe and not shut
/// down.
pub fn snapshot() -> Option<IdleSnapshot> {
    let last_input_tick = last_input_tick()?;
    let now = unsafe { GetTickCount() };
    Some(IdleSnapshot {
        last_input_tick,
        idle: Duration::from_millis(now.wrapping_sub(last_input_tick) as u64),
    })
}

/// Milliseconds since the last input event.
pub fn idle_duration() -> Option<Duration> {
    snapshot().map(|snapshot| snapshot.idle)
}
