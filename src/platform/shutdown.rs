//! Normal Windows shutdown with a command-line fallback.
//!
//! The preferred path enables `SeShutdownPrivilege` for the current process and
//! calls `ExitWindowsEx(EWX_SHUTDOWN | EWX_FORCEIFHUNG)`. This is a regular
//! system shutdown (not a power-off or destructive operation); the FORCEIFHUNG
//! flag only prevents a hung application from blocking the shutdown. If the API
//! is refused, `shutdown.exe /s /t 0` is used as a second normal-shutdown path.

use std::os::windows::process::CommandExt;
use std::process::Command;

use windows::Win32::Foundation::{CloseHandle, ERROR_NOT_ALL_ASSIGNED, HANDLE, LUID};
use windows::Win32::Security::{
    AdjustTokenPrivileges, LUID_AND_ATTRIBUTES, LookupPrivilegeValueW, SE_PRIVILEGE_ENABLED,
    SE_SHUTDOWN_NAME, TOKEN_ADJUST_PRIVILEGES, TOKEN_PRIVILEGES, TOKEN_QUERY,
};
use windows::Win32::System::Shutdown::{
    EWX_FORCEIFHUNG, EWX_SHUTDOWN, EXIT_WINDOWS_FLAGS, ExitWindowsEx, SHTDN_REASON_FLAG_PLANNED,
    SHTDN_REASON_MAJOR_APPLICATION, SHUTDOWN_REASON,
};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Enable the shutdown privilege for the current process. Failure here is not
/// fatal: the fallback may still succeed for an interactive user.
fn enable_shutdown_privilege() -> Result<(), String> {
    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY,
            &mut token,
        )
        .map_err(|error| format!("无法打开进程令牌：{error}"))?;

        let mut luid = LUID::default();
        let lookup = LookupPrivilegeValueW(None, SE_SHUTDOWN_NAME, &mut luid);
        if let Err(error) = lookup {
            let _ = CloseHandle(token);
            return Err(format!("无法查询关机权限：{error}"));
        }

        let privileges = TOKEN_PRIVILEGES {
            PrivilegeCount: 1,
            Privileges: [LUID_AND_ATTRIBUTES {
                Luid: luid,
                Attributes: SE_PRIVILEGE_ENABLED,
            }],
        };
        let adjust = AdjustTokenPrivileges(token, false, Some(&privileges), 0, None, None);
        // AdjustTokenPrivileges can report success while not assigning the
        // privilege; check the last error explicitly.
        let last_error = windows::Win32::Foundation::GetLastError();
        let _ = CloseHandle(token);
        adjust.map_err(|error| format!("无法启用关机权限：{error}"))?;
        if last_error == ERROR_NOT_ALL_ASSIGNED {
            return Err("当前用户未被授予关机权限。".to_string());
        }
    }
    Ok(())
}

fn shutdown_via_exe() -> Result<(), String> {
    let status = Command::new("shutdown.exe")
        .args(["/s", "/t", "0"])
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map_err(|error| format!("无法启动 shutdown.exe：{error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("shutdown.exe 返回失败状态：{status}"))
    }
}

/// Attempt a normal Windows shutdown. This function blocks only for the short
/// API call; the actual system shutdown happens asynchronously.
pub fn request_shutdown() -> Result<(), String> {
    let reason = SHUTDOWN_REASON(SHTDN_REASON_MAJOR_APPLICATION.0 | SHTDN_REASON_FLAG_PLANNED.0);
    let flags = EXIT_WINDOWS_FLAGS(EWX_SHUTDOWN.0 | EWX_FORCEIFHUNG.0);

    let api_error = match enable_shutdown_privilege() {
        Ok(()) => match unsafe { ExitWindowsEx(flags, reason) } {
            Ok(()) => return Ok(()),
            Err(error) => format!("ExitWindowsEx 调用失败：{error}"),
        },
        Err(error) => error,
    };

    match shutdown_via_exe() {
        Ok(()) => Ok(()),
        Err(fallback_error) => Err(format!("{api_error}；备用关机方式也失败：{fallback_error}")),
    }
}

/// Show a blocking error dialog after both shutdown paths fail. This is only
/// reached when the timing loop has reached the shutdown deadline.
pub fn show_failure_dialog(message: &str) {
    use windows::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW};
    use windows::core::HSTRING;
    let text = HSTRING::from(message);
    let caption = HSTRING::from("无操作自动关机：关机失败");
    unsafe {
        MessageBoxW(None, &text, &caption, MB_OK | MB_ICONERROR);
    }
}
