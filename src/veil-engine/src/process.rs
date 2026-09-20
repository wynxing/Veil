use crate::native::CcdConstants;
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::PathBuf;
use windows_sys::Win32::Foundation::{CloseHandle, GetLastError};
use windows_sys::Win32::System::Threading::{
    CreateProcessW, CREATE_BREAKAWAY_FROM_JOB, CREATE_NEW_PROCESS_GROUP, CREATE_NO_WINDOW,
    PROCESS_INFORMATION, STARTUPINFOW,
};

pub struct ProcessLaunch;

impl ProcessLaunch {
    pub fn start_detached(file_name: &str, arguments: &str) -> Result<i32, String> {
        let command = format!("\"{file_name}\" {arguments}");
        let mut wide: Vec<u16> = OsStr::new(&command)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let mut flags = CREATE_BREAKAWAY_FROM_JOB | CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW;
        if let Some(pid) = try_create(&mut wide, flags) {
            return Ok(pid);
        }
        flags = CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW;
        if let Some(pid) = try_create(&mut wide, flags) {
            return Ok(pid);
        }
        Err(format!("CreateProcess failed: {}", unsafe {
            GetLastError()
        }))
    }

    pub fn recovery_exe_path() -> PathBuf {
        first_existing(&["Veil.Recovery.exe", "veil_recovery.exe"])
            .unwrap_or_else(|| exe_dir().join("Veil.Recovery.exe"))
    }

    pub fn driver_helper_exe_path() -> PathBuf {
        first_existing(&["Veil.DriverHelper.exe", "veil_driver_helper.exe"])
            .unwrap_or_else(|| exe_dir().join("Veil.DriverHelper.exe"))
    }
}

fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn first_existing(names: &[&str]) -> Option<PathBuf> {
    let mut dirs = vec![exe_dir()];
    if let Ok(cwd) = std::env::current_dir() {
        if !dirs.iter().any(|d| d == &cwd) {
            dirs.push(cwd);
        }
    }
    for dir in dirs {
        for name in names {
            let candidate = dir.join(name);
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }
    None
}

fn try_create(command: &mut [u16], flags: u32) -> Option<i32> {
    let mut si: STARTUPINFOW = unsafe { std::mem::zeroed() };
    si.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
    let mut pi: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
    let created = unsafe {
        CreateProcessW(
            std::ptr::null(),
            command.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            0,
            flags,
            std::ptr::null(),
            std::ptr::null(),
            &si,
            &mut pi,
        )
    };
    if created == 0 {
        return None;
    }
    unsafe {
        CloseHandle(pi.hThread);
        CloseHandle(pi.hProcess);
    }
    Some(pi.dwProcessId as i32)
}

#[allow(dead_code)]
fn _flags_match() {
    let _ = CcdConstants::CREATE_BREAKAWAY_FROM_JOB;
}
