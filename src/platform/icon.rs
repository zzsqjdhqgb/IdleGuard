//! Loads the embedded icon into a Win32 `HICON`.

use windows::Win32::UI::WindowsAndMessaging::{
    CreateIconFromResourceEx, DestroyIcon, HICON, LR_DEFAULTCOLOR,
};

use crate::assets;

/// Create an `HICON` from the embedded ICO payload. The caller owns the handle
/// and should call [`destroy_icon`] during shutdown.
pub fn create_app_icon(preferred_size: u32) -> Option<HICON> {
    let image = assets::best_icon_image(preferred_size)?;
    unsafe { CreateIconFromResourceEx(image.data, true, 0x0003_0000, 0, 0, LR_DEFAULTCOLOR).ok() }
}

/// Release an icon created by [`create_app_icon`].
pub fn destroy_icon(icon: HICON) {
    if !icon.is_invalid() {
        unsafe {
            let _ = DestroyIcon(icon);
        }
    }
}
