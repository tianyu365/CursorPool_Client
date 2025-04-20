#[cfg(target_os = "windows")]
use std::ffi::OsStr;
#[cfg(target_os = "windows")]
use std::os::windows::ffi::OsStrExt;
#[cfg(target_os = "windows")]
use windows::core::PCWSTR;
#[cfg(target_os = "windows")]
use windows::Win32::Foundation::HWND;
#[cfg(target_os = "windows")]
use windows::Win32::UI::Shell::ShellExecuteW;
#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::SHOW_WINDOW_CMD;

#[cfg(target_os = "windows")]
pub fn check_admin_privileges() -> Result<bool, String> {
    use windows::Win32::UI::Shell::IsUserAnAdmin;

    unsafe { Ok(IsUserAnAdmin().as_bool()) }
}

#[cfg(target_os = "windows")]
pub fn request_admin_privileges(exe_path: &str) -> Result<bool, String> {
    let operation: Vec<u16> = OsStr::new("runas").encode_wide().chain(Some(0)).collect();
    let file: Vec<u16> = OsStr::new(exe_path).encode_wide().chain(Some(0)).collect();

    unsafe {
        let result = ShellExecuteW(
            HWND::default(),
            PCWSTR(operation.as_ptr()),
            PCWSTR(file.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SHOW_WINDOW_CMD(1)
        );

        if result.0 > 32 {
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

// 为macOS提供实现
#[cfg(target_os = "macos")]
pub fn check_admin_privileges() -> Result<bool, String> {
    Ok(false)
}

#[cfg(target_os = "macos")]
pub fn request_admin_privileges(_exe_path: &str) -> Result<bool, String> {
    Ok(false)
}

// 为Linux提供实现
#[cfg(target_os = "linux")]
pub fn check_admin_privileges() -> Result<bool, String> {
    Ok(false)
}

#[cfg(target_os = "linux")]
pub fn request_admin_privileges(_exe_path: &str) -> Result<bool, String> {
    Ok(false)
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
pub fn check_admin_privileges() -> Result<bool, String> {
    Err(format!("不支持的操作系统: {}", std::env::consts::OS))
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
pub fn request_admin_privileges(_exe_path: &str) -> Result<bool, String> {
    Err(format!("不支持的操作系统: {}", std::env::consts::OS))
}
