use std::ffi::c_void;
use std::sync::{Arc, Mutex, OnceLock};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows_sys::core::GUID;
use windows_sys::Win32::Foundation::{HWND, RECT};
use windows_sys::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED};
use windows_sys::Win32::UI::Shell::TaskbarList;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, GetWindowLongPtrW, GetWindowRect, IsIconic, IsWindow, SetForegroundWindow,
    SetWindowLongPtrW, SetWindowPos, ShowWindow, GWL_EXSTYLE, HWND_TOP, SWP_FRAMECHANGED,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SWP_SHOWWINDOW, SW_RESTORE,
    SW_SHOWNOACTIVATE, WS_EX_APPWINDOW, WS_EX_TOOLWINDOW,
};

const PARK_X: i32 = -32_000;
const PARK_Y: i32 = -32_000;
const DEFAULT_X: i32 = 200;
const DEFAULT_Y: i32 = 200;
const DEFAULT_W: i32 = 420;
const DEFAULT_H: i32 = 560;
const IID_ITASKBAR_LIST: GUID = GUID::from_u128(0x56FDF342_fd6d_11d0_958a_006097c9a090);

#[repr(C)]
struct ITaskbarList {
    vtbl: *const ITaskbarListVtbl,
}

#[repr(C)]
struct ITaskbarListVtbl {
    query_interface: unsafe extern "system" fn(*mut ITaskbarList, *const GUID, *mut *mut c_void) -> i32,
    add_ref: unsafe extern "system" fn(*mut ITaskbarList) -> u32,
    release: unsafe extern "system" fn(*mut ITaskbarList) -> u32,
    hr_init: unsafe extern "system" fn(*mut ITaskbarList) -> i32,
    add_tab: unsafe extern "system" fn(*mut ITaskbarList, HWND) -> i32,
    delete_tab: unsafe extern "system" fn(*mut ITaskbarList, HWND) -> i32,
    activate_tab: unsafe extern "system" fn(*mut ITaskbarList, HWND) -> i32,
    set_active_alt: unsafe extern "system" fn(*mut ITaskbarList, HWND) -> i32,
}

#[derive(Clone, Copy)]
struct Rect {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

#[derive(Clone)]
pub struct PanelWindow {
    cached: Arc<Mutex<isize>>,
    last_rect: Arc<Mutex<Rect>>,
}

impl PanelWindow {
    pub fn new() -> Self {
        Self {
            cached: Arc::new(Mutex::new(0)),
            last_rect: Arc::new(Mutex::new(Rect {
                x: DEFAULT_X,
                y: DEFAULT_Y,
                w: DEFAULT_W,
                h: DEFAULT_H,
            })),
        }
    }

    pub fn hwnd_from_frame(frame: &eframe::Frame) -> isize {
        let Ok(handle) = frame.window_handle() else {
            return 0;
        };
        match handle.as_raw() {
            RawWindowHandle::Win32(win) => win.hwnd.get(),
            _ => 0,
        }
    }

    pub fn show(&self) {
        let hwnd = self.hwnd();
        if hwnd == 0 {
            crate::app_log("无法显示面板：没有 HWND。");
            return;
        }
        self.restore_window(hwnd, true);
        crate::app_log("面板已拉回前台。");
    }

    pub fn hide(&self) {
        let hwnd = self.hwnd();
        if hwnd == 0 {
            crate::app_log("无法隐藏面板：没有 HWND。");
            return;
        }
        self.remember_rect(hwnd);
        self.park_window(hwnd);
        crate::app_log("面板已退回托盘，事件循环保持运行。");
    }

    pub fn sync(&self, hwnd: isize, open: bool) {
        if hwnd != 0 && is_window(hwnd) {
            self.set_hwnd(hwnd);
        }
        let hwnd = self.hwnd();
        if hwnd == 0 {
            return;
        }
        if open {
            if is_parked(hwnd) {
                self.restore_window(hwnd, false);
            } else {
                self.remember_rect(hwnd);
                unsafe {
                    apply_shown_style(hwnd as HWND);
                }
                taskbar_add(hwnd);
            }
        } else {
            if !is_parked(hwnd) {
                self.remember_rect(hwnd);
            }
            self.park_window(hwnd);
        }
    }

    fn hwnd(&self) -> isize {
        let cached = *self.cached.lock().unwrap_or_else(|e| e.into_inner());
        if cached != 0 && is_window(cached) {
            cached
        } else {
            0
        }
    }

    fn set_hwnd(&self, hwnd: isize) {
        *self.cached.lock().unwrap_or_else(|e| e.into_inner()) = hwnd;
    }

    fn restore_window(&self, hwnd: isize, activate: bool) {
        let rect = *self.last_rect.lock().unwrap_or_else(|e| e.into_inner());
        unsafe {
            apply_shown_style(hwnd as HWND);
            SetWindowPos(
                hwnd as HWND,
                HWND_TOP,
                rect.x,
                rect.y,
                rect.w,
                rect.h,
                SWP_SHOWWINDOW | SWP_FRAMECHANGED,
            );
            if IsIconic(hwnd as HWND) != 0 {
                ShowWindow(hwnd as HWND, SW_RESTORE);
            }
            if activate {
                BringWindowToTop(hwnd as HWND);
                let _ = SetForegroundWindow(hwnd as HWND);
            }
        }
        taskbar_add(hwnd);
        crate::app_log(&format!(
            "面板已恢复 {}x{} @ {},{}",
            rect.w, rect.h, rect.x, rect.y
        ));
    }

    fn park_window(&self, hwnd: isize) {
        unsafe {
            if IsIconic(hwnd as HWND) != 0 {
                ShowWindow(hwnd as HWND, SW_SHOWNOACTIVATE);
            }
            apply_hidden_style(hwnd as HWND);
            if !is_parked(hwnd) {
                SetWindowPos(
                    hwnd as HWND,
                    HWND_TOP,
                    PARK_X,
                    PARK_Y,
                    1,
                    1,
                    SWP_NOZORDER | SWP_NOACTIVATE | SWP_SHOWWINDOW | SWP_FRAMECHANGED,
                );
            } else {
                SetWindowPos(
                    hwnd as HWND,
                    HWND_TOP,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
                );
            }
        }
        taskbar_delete(hwnd);
    }

    fn remember_rect(&self, hwnd: isize) {
        let Some(rect) = client_rect(hwnd) else {
            return;
        };
        if rect.w < 80 || rect.h < 80 {
            return;
        }
        *self.last_rect.lock().unwrap_or_else(|e| e.into_inner()) = rect;
    }
}

fn is_window(hwnd: isize) -> bool {
    unsafe { IsWindow(hwnd as HWND) != 0 }
}

fn is_parked(hwnd: isize) -> bool {
    let Some(rect) = client_rect(hwnd) else {
        return false;
    };
    rect.x <= PARK_X + 1_000 && rect.w < 80
}

fn client_rect(hwnd: isize) -> Option<Rect> {
    let mut raw = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    let ok = unsafe { GetWindowRect(hwnd as HWND, &mut raw) };
    if ok == 0 {
        return None;
    }
    Some(Rect {
        x: raw.left,
        y: raw.top,
        w: raw.right.saturating_sub(raw.left),
        h: raw.bottom.saturating_sub(raw.top),
    })
}

unsafe fn apply_hidden_style(hwnd: HWND) {
    let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
    let next = (style | WS_EX_TOOLWINDOW as isize) & !(WS_EX_APPWINDOW as isize);
    if next != style {
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, next);
    }
}

unsafe fn apply_shown_style(hwnd: HWND) {
    let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
    let next = (style | WS_EX_APPWINDOW as isize) & !(WS_EX_TOOLWINDOW as isize);
    if next != style {
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, next);
    }
}

fn taskbar() -> Option<*mut ITaskbarList> {
    static TASKBAR: OnceLock<usize> = OnceLock::new();
    let ptr = *TASKBAR.get_or_init(|| unsafe { create_taskbar() });
    if ptr == 0 {
        None
    } else {
        Some(ptr as *mut ITaskbarList)
    }
}

unsafe fn create_taskbar() -> usize {
    let hr = CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32);
    if hr < 0 && hr != -2147417850 {
        crate::app_log(&format!("CoInitializeEx 失败 HRESULT={hr}"));
        return 0;
    }
    let mut ptr: *mut c_void = std::ptr::null_mut();
    let hr = CoCreateInstance(
        &TaskbarList,
        std::ptr::null_mut(),
        CLSCTX_INPROC_SERVER,
        &IID_ITASKBAR_LIST,
        &mut ptr,
    );
    if hr < 0 || ptr.is_null() {
        crate::app_log(&format!("ITaskbarList 创建失败 HRESULT={hr}"));
        return 0;
    }
    let list = ptr as *mut ITaskbarList;
    let init = ((*(*list).vtbl).hr_init)(list);
    if init < 0 {
        crate::app_log(&format!("ITaskbarList::HrInit 失败 HRESULT={init}"));
        ((*(*list).vtbl).release)(list);
        return 0;
    }
    list as usize
}

fn taskbar_add(hwnd: isize) {
    let Some(list) = taskbar() else {
        return;
    };
    unsafe {
        let _ = ((*(*list).vtbl).add_tab)(list, hwnd as HWND);
        let _ = ((*(*list).vtbl).activate_tab)(list, hwnd as HWND);
    }
}

fn taskbar_delete(hwnd: isize) {
    let Some(list) = taskbar() else {
        return;
    };
    unsafe {
        let _ = ((*(*list).vtbl).delete_tab)(list, hwnd as HWND);
    }
}
