use std::sync::Mutex;
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows_sys::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject};

use crate::{app_log, to_wide};

static PENDING_HELPER: Mutex<Option<(usize, String)>> = Mutex::new(None);

pub(crate) fn run_helper_elevated(verb: &str) -> i32 {
    // A timed-out helper can still be running. Retain its handle and never launch
    // a second device operation until the first one has actually ended.
    let mut pending = PENDING_HELPER.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((raw, previous_verb)) = pending.as_ref() {
        let handle = *raw as HANDLE;
        let wait = unsafe { WaitForSingleObject(handle, 0) };
        if wait != WAIT_OBJECT_0 {
            app_log(&format!("helper pending verb={previous_verb} wait={wait}"));
            return 1460;
        }
        let mut code = 1;
        let ok = unsafe { GetExitCodeProcess(handle, &mut code) };
        let same = previous_verb == verb;
        unsafe {
            CloseHandle(handle);
        }
        *pending = None;
        app_log(&format!(
            "helper late-end verb={verb} exit={code} query={ok}"
        ));
        if same {
            return if ok != 0 { code as i32 } else { 1 };
        }
    }
    app_log(&format!("helper start verb={verb}"));
    let raw = match launch_helper_elevated(verb) {
        Ok(raw) => raw,
        Err(error) => {
            app_log(&format!("helper launch-failed verb={verb} win32={error}"));
            return error;
        }
    };
    let handle = raw as HANDLE;
    let wait = unsafe { WaitForSingleObject(handle, 60_000) };
    if wait != WAIT_OBJECT_0 {
        let error = unsafe { GetLastError() };
        *pending = Some((raw, verb.to_string()));
        app_log(&format!(
            "helper wait-incomplete verb={verb} wait={wait} win32={error}"
        ));
        return if wait == WAIT_TIMEOUT { 1460 } else { 1 };
    }
    let mut code = 1;
    let ok = unsafe { GetExitCodeProcess(handle, &mut code) };
    let error = if ok == 0 {
        unsafe { GetLastError() }
    } else {
        0
    };
    unsafe {
        CloseHandle(handle);
    }
    app_log(&format!("helper end verb={verb} exit={code} win32={error}"));
    if ok != 0 {
        code as i32
    } else {
        error.max(1) as i32
    }
}

pub(crate) fn helper_operation_unfinished() -> bool {
    let pending = PENDING_HELPER.lock().unwrap_or_else(|e| e.into_inner());
    pending.as_ref().is_some_and(|(raw, _)| {
        (unsafe { WaitForSingleObject(*raw as HANDLE, 0) }) != WAIT_OBJECT_0
    })
}

fn launch_helper_elevated(verb: &str) -> Result<usize, i32> {
    use windows_sys::Win32::System::Com::{
        CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
    };
    use windows_sys::Win32::UI::Shell::{
        ShellExecuteExW, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;

    let exe = veil_engine::ProcessLaunch::driver_helper_exe_path();
    let exe_w = to_wide(&exe.to_string_lossy());
    let params = to_wide(verb);
    let launcher = std::thread::Builder::new()
        .name("veil-elevated-launch".into())
        .spawn(move || {
            // ShellExecuteEx can load STA shell extensions. Keep its apartment on
            // this short-lived launcher; the operation thread waits on the handle.
            let hr = unsafe {
                CoInitializeEx(
                    std::ptr::null(),
                    (COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) as u32,
                )
            };
            if hr < 0 {
                return Err(hr);
            }
            let verb_w = to_wide("runas");
            let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
            info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
            info.fMask = SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC;
            info.lpVerb = verb_w.as_ptr();
            info.lpFile = exe_w.as_ptr();
            info.lpParameters = params.as_ptr();
            info.nShow = SW_HIDE;
            let result = if unsafe { ShellExecuteExW(&mut info) } == 0 {
                Err(unsafe { GetLastError() }.max(1) as i32)
            } else if info.hProcess.is_null() {
                Err(1)
            } else {
                Ok(info.hProcess as usize)
            };
            unsafe { CoUninitialize() };
            result
        })
        .map_err(|_| 1)?;
    launcher.join().unwrap_or(Err(1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::System::Threading::{CreateEventW, SetEvent};

    #[test]
    fn unfinished_helper_blocks_new_device_work_until_process_exits() {
        let handle = unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) };
        assert!(!handle.is_null());
        *PENDING_HELPER.lock().unwrap() = Some((handle as usize, "test".into()));
        assert!(helper_operation_unfinished());
        assert_ne!(unsafe { SetEvent(handle) }, 0);
        assert!(!helper_operation_unfinished());
        PENDING_HELPER.lock().unwrap().take();
        unsafe { CloseHandle(handle) };
    }
}
