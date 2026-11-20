use std::path::{Path, PathBuf};

use crate::to_wide;

const RUN_REG_SUBKEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_REG_VALUE: &str = "Veil";

fn legacy_startup_path() -> PathBuf {
    let appdata = std::env::var("APPDATA").unwrap_or_default();
    PathBuf::from(appdata)
        .join("Microsoft")
        .join("Windows")
        .join("Start Menu")
        .join("Programs")
        .join("Startup")
        .join("Veil.lnk")
}

fn quote_startup_command(exe: &Path) -> String {
    format!("\"{}\"", exe.display())
}

struct RegKeyGuard(windows_sys::Win32::System::Registry::HKEY);

impl Drop for RegKeyGuard {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                windows_sys::Win32::System::Registry::RegCloseKey(self.0);
            }
        }
    }
}

fn registry_startup_exists(subkey: &str, value_name: &str) -> bool {
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{
        RegOpenKeyExW, RegQueryValueExW, HKEY_CURRENT_USER, KEY_QUERY_VALUE,
    };
    let subkey_w = to_wide(subkey);
    let value_w = to_wide(value_name);
    let mut key = std::ptr::null_mut();
    let open = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            subkey_w.as_ptr(),
            0,
            KEY_QUERY_VALUE,
            &mut key,
        )
    };
    if open != ERROR_SUCCESS || key.is_null() {
        return false;
    }
    let _guard = RegKeyGuard(key);
    let mut size = 0u32;
    let query = unsafe {
        RegQueryValueExW(
            key,
            value_w.as_ptr(),
            std::ptr::null(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut size,
        )
    };
    query == ERROR_SUCCESS && size > 0
}

fn set_registry_startup(
    subkey: &str,
    value_name: &str,
    command: Option<&str>,
) -> Result<(), String> {
    use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
    use windows_sys::Win32::System::Registry::{
        RegCreateKeyExW, RegDeleteValueW, RegOpenKeyExW, RegSetValueExW, HKEY_CURRENT_USER,
        KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ,
    };
    let subkey_w = to_wide(subkey);
    let value_w = to_wide(value_name);
    match command {
        Some(cmd) => {
            let mut key = std::ptr::null_mut();
            let rc = unsafe {
                RegCreateKeyExW(
                    HKEY_CURRENT_USER,
                    subkey_w.as_ptr(),
                    0,
                    std::ptr::null(),
                    REG_OPTION_NON_VOLATILE,
                    KEY_SET_VALUE,
                    std::ptr::null(),
                    &mut key,
                    std::ptr::null_mut(),
                )
            };
            if rc != ERROR_SUCCESS || key.is_null() {
                return Err(format!("无法打开自启注册表项（{rc}）"));
            }
            let _guard = RegKeyGuard(key);
            let data = to_wide(cmd);
            let byte_len = (data.len() * std::mem::size_of::<u16>()) as u32;
            let rc = unsafe {
                RegSetValueExW(
                    key,
                    value_w.as_ptr(),
                    0,
                    REG_SZ,
                    data.as_ptr() as *const u8,
                    byte_len,
                )
            };
            if rc != ERROR_SUCCESS {
                return Err(format!("写入自启项失败（{rc}）"));
            }
            Ok(())
        }
        None => {
            let mut key = std::ptr::null_mut();
            let open = unsafe {
                RegOpenKeyExW(
                    HKEY_CURRENT_USER,
                    subkey_w.as_ptr(),
                    0,
                    KEY_SET_VALUE,
                    &mut key,
                )
            };
            if open == ERROR_FILE_NOT_FOUND {
                return Ok(());
            }
            if open != ERROR_SUCCESS || key.is_null() {
                return Err(format!("无法打开自启注册表项（{open}）"));
            }
            let _guard = RegKeyGuard(key);
            let rc = unsafe { RegDeleteValueW(key, value_w.as_ptr()) };
            if rc == ERROR_SUCCESS || rc == ERROR_FILE_NOT_FOUND {
                Ok(())
            } else {
                Err(format!("移除自启项失败（{rc}）"))
            }
        }
    }
}

pub(crate) fn startup_enabled() -> bool {
    registry_startup_exists(RUN_REG_SUBKEY, RUN_REG_VALUE) || legacy_startup_path().exists()
}

pub(crate) fn set_startup(enabled: bool) -> Result<(), String> {
    let legacy = legacy_startup_path();
    if !enabled {
        set_registry_startup(RUN_REG_SUBKEY, RUN_REG_VALUE, None)?;
        if let Err(err) = std::fs::remove_file(&legacy) {
            if err.kind() != std::io::ErrorKind::NotFound {
                return Err(format!("移除旧快捷方式失败：{err}"));
            }
        }
        return Ok(());
    }
    let exe = std::env::current_exe().map_err(|err| format!("无法获取程序路径：{err}"))?;
    let command = quote_startup_command(&exe);
    set_registry_startup(RUN_REG_SUBKEY, RUN_REG_VALUE, Some(&command))?;
    // 清理旧版 Startup\Veil.lnk，避免迁移后开机启动两次。
    let _ = std::fs::remove_file(&legacy);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_command_quotes_paths_with_spaces_and_quotes() {
        let cmd = quote_startup_command(Path::new(r"C:\Program Files\Veil\O'Brien\Veil.App.exe"));
        assert_eq!(cmd, r#""C:\Program Files\Veil\O'Brien\Veil.App.exe""#);
    }

    #[test]
    fn registry_startup_roundtrip_on_isolated_test_key() {
        use windows_sys::Win32::System::Registry::{RegDeleteKeyW, HKEY_CURRENT_USER};
        let subkey = format!(r"Software\Veil\TestStartup-{}", std::process::id());
        let value = "VeilTest";
        assert!(!registry_startup_exists(&subkey, value));
        set_registry_startup(&subkey, value, Some(r#""C:\Veil\Veil.App.exe""#)).unwrap();
        assert!(registry_startup_exists(&subkey, value));
        set_registry_startup(&subkey, value, None).unwrap();
        assert!(!registry_startup_exists(&subkey, value));
        set_registry_startup(&subkey, value, None).unwrap();
        let subkey_w = to_wide(&subkey);
        unsafe {
            RegDeleteKeyW(HKEY_CURRENT_USER, subkey_w.as_ptr());
        }
    }
}
