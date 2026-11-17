//! Per-user Windows auto-start registration.
//!
//! The application is registered under
//! `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` with `--autostart`, so
//! it starts for the current user after login without requiring elevation.

use std::os::windows::ffi::OsStrExt;

use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS, WIN32_ERROR};
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ, RRF_RT_REG_SZ,
    RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegGetValueW, RegOpenKeyExW, RegSetValueExW,
};
use windows::core::w;

const RUN_KEY: windows::core::PCWSTR = w!(r"Software\Microsoft\Windows\CurrentVersion\Run");
const VALUE_NAME: windows::core::PCWSTR = w!("IdleShutdown");

struct RegKey(HKEY);

impl Drop for RegKey {
    fn drop(&mut self) {
        unsafe {
            let _ = RegCloseKey(self.0);
        }
    }
}

fn error_message(action: &str, code: WIN32_ERROR) -> String {
    format!("{action}失败（Windows 错误码 {}）", code.0)
}

fn command_for_current_exe() -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|error| format!("无法获取程序路径：{error}"))?;
    let exe = exe
        .to_str()
        .ok_or_else(|| "程序路径包含无效字符。".to_string())?;
    if exe.contains('"') {
        return Err("程序路径包含引号，无法写入开机自启命令。".to_string());
    }
    Ok(format!("\"{exe}\" --autostart"))
}

fn wide_bytes(value: &str) -> Vec<u8> {
    let mut utf16: Vec<u16> = std::ffi::OsStr::new(value).encode_wide().collect();
    utf16.push(0);
    // SAFETY: `u16` and `u8` both have alignment 1; reading the initialized
    // vector as bytes is valid for the duration of the call.
    let bytes = unsafe {
        std::slice::from_raw_parts(utf16.as_ptr() as *const u8, utf16.len() * size_of::<u16>())
    };
    bytes.to_vec()
}

/// Return whether the current user has an auto-start entry. A missing value is
/// reported as `false`, not as an error.
pub fn is_enabled() -> Result<bool, String> {
    let mut buffer = [0u16; 2048];
    let mut size = (buffer.len() * size_of::<u16>()) as u32;
    let result = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            VALUE_NAME,
            RRF_RT_REG_SZ,
            None,
            Some(buffer.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };

    match result {
        ERROR_SUCCESS => {
            let units = (size as usize / size_of::<u16>()).min(buffer.len());
            let value = String::from_utf16_lossy(&buffer[..units]);
            Ok(!value.trim_matches('\0').trim().is_empty())
        }
        ERROR_FILE_NOT_FOUND => Ok(false),
        code => Err(error_message("读取开机自启设置", code)),
    }
}

/// Create or replace the auto-start entry.
pub fn enable() -> Result<(), String> {
    let command = command_for_current_exe()?;
    let data = wide_bytes(&command);
    let mut key = HKEY::default();
    let result = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            None,
            None,
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            None,
            &mut key,
            None,
        )
    };
    if result != ERROR_SUCCESS {
        return Err(error_message("打开开机自启注册表项", result));
    }
    let key = RegKey(key);

    let result = unsafe { RegSetValueExW(key.0, VALUE_NAME, None, REG_SZ, Some(&data)) };
    if result != ERROR_SUCCESS {
        return Err(error_message("写入开机自启设置", result));
    }
    Ok(())
}

/// Remove the auto-start entry. A missing key or value is a successful no-op.
pub fn disable() -> Result<(), String> {
    let mut key = HKEY::default();
    let result =
        unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, RUN_KEY, None, KEY_SET_VALUE, &mut key) };
    if result == ERROR_FILE_NOT_FOUND {
        return Ok(());
    }
    if result != ERROR_SUCCESS {
        return Err(error_message("打开开机自启注册表项", result));
    }
    let key = RegKey(key);
    let result = unsafe { RegDeleteValueW(key.0, VALUE_NAME) };
    if result == ERROR_SUCCESS || result == ERROR_FILE_NOT_FOUND {
        Ok(())
    } else {
        Err(error_message("删除开机自启设置", result))
    }
}

/// Apply the requested auto-start state.
pub fn set_enabled(enabled: bool) -> Result<(), String> {
    if enabled { enable() } else { disable() }
}
