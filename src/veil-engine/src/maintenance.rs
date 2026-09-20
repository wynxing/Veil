//! 卸载事务门禁：持久标记跨 MSI 子进程，互斥锁串行化标记与关屏 APPLY。
use std::path::PathBuf;
use std::ptr;
use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_FILE_NOT_FOUND, HANDLE, WAIT_ABANDONED, WAIT_OBJECT_0,
};
use windows_sys::Win32::System::Registry::*;
use windows_sys::Win32::System::Threading::{CreateMutexW, ReleaseMutex, WaitForSingleObject};
const KEY: &str = "Software\\Veil\\Maintenance";
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
pub struct OperationLock(HANDLE);
impl OperationLock {
    pub fn acquire() -> Result<Self, String> {
        Self::named("Global\\Veil.DisplayOperation.v2")
    }
    pub fn named(name: &str) -> Result<Self, String> {
        let h = unsafe { CreateMutexW(ptr::null(), 0, wide(name).as_ptr()) };
        if h.is_null() {
            return Err(format!(
                "无法取得显示操作锁：{}",
                std::io::Error::last_os_error()
            ));
        }
        let rc = unsafe { WaitForSingleObject(h, 10_000) };
        if rc != WAIT_OBJECT_0 && rc != WAIT_ABANDONED {
            unsafe {
                CloseHandle(h);
            }
            return Err("显示操作锁等待超时。".into());
        }
        Ok(Self(h))
    }
    pub fn for_keep_off() -> Result<Self, String> {
        let lock = Self::acquire()?;
        require_available(is_active())?;
        Ok(lock)
    }
}
impl Drop for OperationLock {
    fn drop(&mut self) {
        unsafe {
            ReleaseMutex(self.0);
            CloseHandle(self.0);
        }
    }
}
pub fn is_active() -> Result<bool, String> {
    let mut value = 0u32;
    let mut size = 4;
    let rc = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            wide(KEY).as_ptr(),
            wide("Active").as_ptr(),
            RRF_RT_REG_DWORD | RRF_SUBKEY_WOW6464KEY,
            ptr::null_mut(),
            (&mut value as *mut u32).cast(),
            &mut size,
        )
    };
    if rc == ERROR_FILE_NOT_FOUND {
        return Ok(false);
    }
    if rc != 0 {
        return Err(format!("无法读取维护门禁：{rc}"));
    }
    Ok(value != 0)
}
// Only elevated installer actions write this marker; rollback/commit use an embedded binary.
pub fn set_active(active: bool) -> Result<(), String> {
    let _lock = OperationLock::acquire()?;
    let mut key = ptr::null_mut();
    let rc = unsafe {
        RegCreateKeyExW(
            HKEY_LOCAL_MACHINE,
            wide(KEY).as_ptr(),
            0,
            ptr::null(),
            0,
            KEY_SET_VALUE | KEY_WOW64_64KEY,
            ptr::null(),
            &mut key,
            ptr::null_mut(),
        )
    };
    if rc != 0 {
        return Err(format!("无法写入维护门禁：{rc}"));
    }
    let value = active as u32;
    let rc = unsafe {
        RegSetValueExW(
            key,
            wide("Active").as_ptr(),
            0,
            REG_DWORD,
            (&value as *const u32).cast(),
            4,
        )
    };
    unsafe {
        RegCloseKey(key);
    }
    if rc != 0 {
        return Err(format!("无法写入维护门禁：{rc}"));
    }
    Ok(())
}

/// Returns logged-on sessions, including disconnected sessions; callers refuse ambiguity.
fn logged_on_sessions() -> Result<Vec<u32>, String> {
    use windows_sys::Win32::System::RemoteDesktop::*;
    let mut entries = ptr::null_mut();
    let mut count = 0;
    if unsafe { WTSEnumerateSessionsW(ptr::null_mut(), 0, 1, &mut entries, &mut count) } == 0 {
        return Err("无法确认其它用户会话，已阻止卸载。".into());
    }
    if entries.is_null() || count == 0 {
        return Ok(vec![]);
    }
    let result = (|| {
        let mut sessions = Vec::new();
        for session in unsafe { std::slice::from_raw_parts(entries, count as usize) } {
            if session.SessionId == 0 {
                continue;
            }
            let mut name = ptr::null_mut();
            let mut bytes = 0;
            if unsafe {
                WTSQuerySessionInformationW(
                    ptr::null_mut(),
                    session.SessionId,
                    WTSUserName,
                    &mut name,
                    &mut bytes,
                )
            } == 0
            {
                return Err("无法确认会话用户，已阻止卸载。".into());
            }
            let has_user = bytes > 2 && !name.is_null() && unsafe { *name } != 0;
            unsafe {
                WTSFreeMemory(name.cast());
            }
            if has_user {
                sessions.push(session.SessionId);
            }
        }
        Ok(sessions)
    })();
    unsafe {
        WTSFreeMemory(entries.cast());
    }
    result
}

pub fn select_restore_session(sessions: &[u32]) -> Result<u32, String> {
    match sessions {
        [id] if *id != 0 => Ok(*id),
        _ => Err("无法唯一确认交互用户；请先在各用户会话恢复显示并注销其它会话后卸载。".into()),
    }
}

pub fn can_act_as_restore_proxy(current: u32, interactive: u32) -> Result<(), String> {
    if current == 0 || (interactive != 0 && current == interactive) {
        Ok(())
    } else {
        Err("当前进程不属于待恢复用户会话。".into())
    }
}

pub fn require_single_interactive_session() -> Result<(), String> {
    use windows_sys::Win32::System::RemoteDesktop::ProcessIdToSessionId;
    let mut current = 0;
    if unsafe { ProcessIdToSessionId(std::process::id(), &mut current) } == 0 || current == 0 {
        return Err("恢复必须在对应的交互用户会话中执行。".into());
    }
    if select_restore_session(&logged_on_sessions()?)? != current {
        return Err("当前进程不属于待恢复用户会话。".into());
    }
    Ok(())
}

/// MSI SYSTEM action launches the fixed installed application with the actual user's
/// primary token and environment. No user-controlled executable or arguments are accepted.
pub fn restore_in_user_session(app: &std::path::Path) -> Result<i32, String> {
    use windows_sys::Win32::System::Environment::{
        CreateEnvironmentBlock, DestroyEnvironmentBlock,
    };
    use windows_sys::Win32::System::RemoteDesktop::{ProcessIdToSessionId, WTSQueryUserToken};
    use windows_sys::Win32::System::Threading::*;
    let mut current = 0;
    if unsafe { ProcessIdToSessionId(std::process::id(), &mut current) } == 0 {
        return Err("无法确认安装恢复代理会话。".into());
    }
    if !is_active()? {
        return Err("缺少卸载维护门禁。".into());
    }
    let session = select_restore_session(&logged_on_sessions()?)?;
    can_act_as_restore_proxy(current, session)?;
    struct Handle(HANDLE);
    impl Drop for Handle {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
    let mut token = ptr::null_mut();
    if unsafe { WTSQueryUserToken(session, &mut token) } == 0 {
        return Err(format!(
            "无法取得交互用户令牌：{}",
            std::io::Error::last_os_error()
        ));
    }
    let token = Handle(token);
    let mut environment = ptr::null_mut();
    if unsafe { CreateEnvironmentBlock(&mut environment, token.0, 0) } == 0 {
        return Err(format!(
            "无法创建用户环境：{}",
            std::io::Error::last_os_error()
        ));
    }
    let executable = wide(&app.to_string_lossy());
    let mut command = wide(&format!("\"{}\" --restore-and-exit", app.display()));
    let mut desktop = wide("winsta0\\default");
    let mut startup: STARTUPINFOW = unsafe { std::mem::zeroed() };
    startup.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
    startup.lpDesktop = desktop.as_mut_ptr();
    let mut process: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
    let created = unsafe {
        CreateProcessAsUserW(
            token.0,
            executable.as_ptr(),
            command.as_mut_ptr(),
            ptr::null(),
            ptr::null(),
            0,
            CREATE_UNICODE_ENVIRONMENT | CREATE_NO_WINDOW,
            environment,
            ptr::null(),
            &startup,
            &mut process,
        )
    };
    let error = std::io::Error::last_os_error();
    unsafe {
        DestroyEnvironmentBlock(environment);
    }
    if created == 0 {
        return Err(format!("无法在用户会话启动恢复：{error}"));
    }
    let process_handle = Handle(process.hProcess);
    let _thread_handle = Handle(process.hThread);
    if unsafe { WaitForSingleObject(process_handle.0, 30_000) } != WAIT_OBJECT_0 {
        return Ok(2); // Never proceed with removal after a timed-out recovery.
    }
    let mut code = 2;
    if unsafe { GetExitCodeProcess(process_handle.0, &mut code) } == 0 {
        return Err("无法读取用户会话恢复结果。".into());
    }
    // Recheck the session set before accepting a successful response.
    if select_restore_session(&logged_on_sessions()?)? != session {
        return Err("恢复期间用户会话已变化。".into());
    }
    Ok(code as i32)
}

/// LOCALAPPDATA\Veil of the single interactive user. SYSTEM actions use this when
/// `SessionPaths::root()` would resolve the service profile.
pub fn interactive_user_veil_dir() -> Result<PathBuf, String> {
    use windows_sys::Win32::System::Environment::{
        CreateEnvironmentBlock, DestroyEnvironmentBlock,
    };
    use windows_sys::Win32::System::RemoteDesktop::WTSQueryUserToken;
    let session = select_restore_session(&logged_on_sessions()?)?;
    let mut token = ptr::null_mut();
    if unsafe { WTSQueryUserToken(session, &mut token) } == 0 {
        return Err(format!(
            "无法取得交互用户令牌：{}",
            std::io::Error::last_os_error()
        ));
    }
    struct Handle(HANDLE);
    impl Drop for Handle {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
    let token = Handle(token);
    let mut environment = ptr::null_mut();
    if unsafe { CreateEnvironmentBlock(&mut environment, token.0, 0) } == 0 {
        return Err(format!(
            "无法创建用户环境：{}",
            std::io::Error::last_os_error()
        ));
    }
    let local = parse_env_value(environment, "LOCALAPPDATA");
    unsafe {
        DestroyEnvironmentBlock(environment);
    }
    let local = local.ok_or_else(|| "无法读取交互用户 LOCALAPPDATA。".to_string())?;
    Ok(PathBuf::from(local).join("Veil"))
}

fn parse_env_value(block: *mut core::ffi::c_void, key: &str) -> Option<String> {
    if block.is_null() {
        return None;
    }
    let prefix = format!("{key}=");
    let mut p = block.cast::<u16>();
    unsafe {
        loop {
            if *p == 0 {
                break;
            }
            let mut end = p;
            while *end != 0 {
                end = end.add(1);
            }
            let len = end.offset_from(p) as usize;
            let s = String::from_utf16_lossy(std::slice::from_raw_parts(p, len));
            if let Some(value) = s.strip_prefix(prefix.as_str()) {
                return Some(value.to_string());
            }
            p = end.add(1);
        }
    }
    None
}

pub fn require_available(active: Result<bool, String>) -> Result<(), String> {
    if active? {
        Err("安装维护中，禁止新的关屏操作。".into())
    } else {
        Ok(())
    }
}
