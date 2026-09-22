use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use windows_sys::core::GUID;
use windows_sys::Win32::Foundation::{HWND, POINT, RECT};
use windows_sys::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
};
use windows_sys::Win32::UI::Shell::TaskbarList;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, GetWindowLongPtrW, GetWindowRect, IsIconic, IsWindow, IsWindowVisible,
    SetForegroundWindow, SetWindowLongPtrW, SetWindowPlacement, SetWindowPos, ShowWindow,
    SystemParametersInfoW, GWL_EXSTYLE, HWND_TOP, SPI_GETWORKAREA, SWP_FRAMECHANGED,
    SWP_NOACTIVATE, SWP_NOZORDER, SWP_SHOWWINDOW, SW_SHOWNOACTIVATE, SW_SHOWNORMAL,
    WINDOWPLACEMENT, WS_EX_APPWINDOW, WS_EX_TOOLWINDOW,
};

use windows_sys::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromRect, MONITORINFO, MONITOR_DEFAULTTONEAREST,
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
    query_interface:
        unsafe extern "system" fn(*mut ITaskbarList, *const GUID, *mut *mut c_void) -> i32,
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
    parked: Arc<AtomicBool>,
    synced: Arc<Mutex<Option<(isize, bool)>>>,
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
            parked: Arc::new(AtomicBool::new(false)),
            synced: Arc::new(Mutex::new(None)),
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
        if self.restore_window(hwnd, true) {
            *self.synced.lock().unwrap_or_else(|e| e.into_inner()) = Some((hwnd, true));
        }
    }

    pub fn is_parked(&self) -> bool {
        self.parked.load(Ordering::Relaxed)
    }

    pub fn is_iconic(&self) -> bool {
        let hwnd = self.hwnd();
        hwnd != 0 && unsafe { IsIconic(hwnd as HWND) } != 0
    }

    pub fn hide(&self) {
        let hwnd = self.hwnd();
        if hwnd == 0 {
            crate::app_log("无法隐藏面板：没有 HWND。");
            return;
        }
        self.remember_rect(hwnd);
        self.park_window(hwnd);
        *self.synced.lock().unwrap_or_else(|e| e.into_inner()) = Some((hwnd, false));
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
        let mut synced = self.synced.lock().unwrap_or_else(|e| e.into_inner());
        if *synced == Some((hwnd, open)) {
            if open {
                self.remember_rect(hwnd);
            }
            return;
        }
        // Only transitions write native state. A failed show is retried by a new
        // user request, not by every paint of an unresponsive window.
        *synced = Some((hwnd, open));
        if open {
            if !self.is_parked() {
                self.remember_rect(hwnd);
            }
            unsafe {
                apply_shown_style(hwnd as HWND);
            }
            taskbar_add(hwnd, false);
            self.restore_window(hwnd, false);
        } else {
            if !self.is_parked() {
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

    fn restore_window(&self, hwnd: isize, activate: bool) -> bool {
        let rect = self.visible_rect();
        let already_ok = client_rect(hwnd).is_some_and(|current| {
            !needs_restore(
                current,
                rect,
                self.is_parked(),
                unsafe { IsIconic(hwnd as HWND) } != 0,
                unsafe { IsWindowVisible(hwnd as HWND) } != 0,
            )
        });
        if already_ok {
            return true;
        }
        let (monitor, work) = monitor_area(rect);
        let placement_rect = to_workspace(rect, monitor, work);
        unsafe {
            apply_shown_style(hwnd as HWND);
            let placement = WINDOWPLACEMENT {
                length: std::mem::size_of::<WINDOWPLACEMENT>() as u32,
                flags: 0,
                showCmd: if activate {
                    SW_SHOWNORMAL as u32
                } else {
                    SW_SHOWNOACTIVATE as u32
                },
                ptMinPosition: POINT { x: 0, y: 0 },
                ptMaxPosition: POINT { x: 0, y: 0 },
                rcNormalPosition: RECT {
                    left: placement_rect.x,
                    top: placement_rect.y,
                    right: placement_rect.x + placement_rect.w,
                    bottom: placement_rect.y + placement_rect.h,
                },
            };
            let placement_ok = SetWindowPlacement(hwnd as HWND, &placement);
            if placement_ok == 0 {
                crate::app_log("SetWindowPlacement 失败。");
            }
            let mut flags = SWP_SHOWWINDOW | SWP_FRAMECHANGED;
            if !activate {
                flags |= SWP_NOACTIVATE;
            }
            SetWindowPos(
                hwnd as HWND,
                HWND_TOP,
                rect.x,
                rect.y,
                rect.w,
                rect.h,
                flags,
            );
            if activate {
                BringWindowToTop(hwnd as HWND);
                let _ = SetForegroundWindow(hwnd as HWND);
            }
        }
        taskbar_add(hwnd, activate);
        let restored = client_rect(hwnd).is_some_and(|current| {
            !looks_parked(&current) && unsafe { IsIconic(hwnd as HWND) } == 0
        });
        if restored {
            self.parked.store(false, Ordering::Relaxed);
            if !already_ok {
                crate::app_log(&format!(
                    "面板已恢复 {}x{} @ {},{}",
                    rect.w, rect.h, rect.x, rect.y
                ));
            }
        }
        restored
    }

    fn park_window(&self, hwnd: isize) {
        unsafe {
            if IsIconic(hwnd as HWND) != 0 {
                ShowWindow(hwnd as HWND, SW_SHOWNOACTIVATE);
            }
            apply_hidden_style(hwnd as HWND);
            if !client_rect(hwnd).is_some_and(looks_parked_tiny) {
                SetWindowPos(
                    hwnd as HWND,
                    HWND_TOP,
                    PARK_X,
                    PARK_Y,
                    1,
                    1,
                    SWP_NOZORDER | SWP_NOACTIVATE | SWP_SHOWWINDOW | SWP_FRAMECHANGED,
                );
            }
        }
        taskbar_delete(hwnd);
        self.parked.store(true, Ordering::Relaxed);
    }

    fn remember_rect(&self, hwnd: isize) {
        if unsafe { IsIconic(hwnd as HWND) } != 0 {
            return;
        }
        let Some(rect) = client_rect(hwnd) else {
            return;
        };
        if looks_parked(&rect) {
            return;
        }
        *self.last_rect.lock().unwrap_or_else(|e| e.into_inner()) = rect;
    }

    fn visible_rect(&self) -> Rect {
        let mut stored = self.last_rect.lock().unwrap_or_else(|e| e.into_inner());
        let (_, work) = monitor_area(*stored);
        let clamped = clamp_to_work(*stored, work);
        *stored = clamped;
        clamped
    }
}

fn is_window(hwnd: isize) -> bool {
    unsafe { IsWindow(hwnd as HWND) != 0 }
}

fn looks_parked(rect: &Rect) -> bool {
    rect.x <= PARK_X + 1_000 || rect.y <= PARK_Y + 1_000 || rect.w < 80 || rect.h < 80
}

fn looks_parked_tiny(rect: Rect) -> bool {
    rect.x <= PARK_X + 1_000 && rect.w < 80
}

fn work_area() -> RECT {
    let mut work = RECT {
        left: 0,
        top: 0,
        right: 1920,
        bottom: 1080,
    };
    unsafe {
        let _ = SystemParametersInfoW(SPI_GETWORKAREA, 0, &mut work as *mut RECT as *mut c_void, 0);
    }
    work
}

fn monitor_area(rect: Rect) -> (RECT, RECT) {
    let r = if looks_parked(&rect) {
        Rect {
            x: DEFAULT_X,
            y: DEFAULT_Y,
            w: DEFAULT_W,
            h: DEFAULT_H,
        }
    } else {
        rect
    };
    let raw = RECT {
        left: r.x,
        top: r.y,
        right: r.x.saturating_add(r.w),
        bottom: r.y.saturating_add(r.h),
    };
    unsafe {
        let monitor = MonitorFromRect(&raw, MONITOR_DEFAULTTONEAREST);
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(monitor, &mut info) != 0 {
            return (info.rcMonitor, info.rcWork);
        }
    }
    let fallback = work_area();
    (fallback, fallback)
}

// WINDOWPLACEMENT uses workspace coordinates for the shown (non-tool) window.
// SetWindowPos and the remembered outer rectangle use physical screen pixels.
fn to_workspace(rect: Rect, monitor: RECT, work: RECT) -> Rect {
    Rect {
        x: rect.x - (work.left - monitor.left),
        y: rect.y - (work.top - monitor.top),
        ..rect
    }
}

fn same_rect(a: Rect, b: Rect) -> bool {
    a.x == b.x && a.y == b.y && a.w == b.w && a.h == b.h
}

fn needs_restore(current: Rect, desired: Rect, parked: bool, iconic: bool, visible: bool) -> bool {
    parked || iconic || !visible || !same_rect(current, desired)
}

fn clamp_to_work(rect: Rect, work: RECT) -> Rect {
    let parked = looks_parked(&rect);
    let w = (if parked { DEFAULT_W } else { rect.w.max(80) }).min((work.right - work.left).max(1));
    let h = (if parked { DEFAULT_H } else { rect.h.max(80) }).min((work.bottom - work.top).max(1));
    Rect {
        x: (if parked { DEFAULT_X } else { rect.x })
            .clamp(work.left, (work.right - w).max(work.left)),
        y: (if parked { DEFAULT_Y } else { rect.y })
            .clamp(work.top, (work.bottom - h).max(work.top)),
        w,
        h,
    }
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

fn taskbar_add(hwnd: isize, activate: bool) {
    let Some(list) = taskbar() else {
        return;
    };
    unsafe {
        let _ = ((*(*list).vtbl).add_tab)(list, hwnd as HWND);
        if activate {
            let _ = ((*(*list).vtbl).activate_tab)(list, hwnd as HWND);
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partially_offscreen_and_oversized_windows_are_fully_reachable() {
        let work = RECT {
            left: 40,
            top: 50,
            right: 1920,
            bottom: 1080,
        };
        let rect = clamp_to_work(
            Rect {
                x: 1800,
                y: 900,
                w: 2200,
                h: 1400,
            },
            work,
        );
        assert_eq!((rect.x, rect.y, rect.w, rect.h), (40, 50, 1880, 1030));
    }

    #[test]
    fn physical_geometry_is_idempotent_at_common_dpi_scales() {
        for scale in [1.0, 1.5, 2.0] {
            let work = RECT {
                left: -2560,
                top: 40,
                right: 0,
                bottom: 1440,
            };
            let original = Rect {
                x: -2000,
                y: 120,
                w: (420.0 * scale) as i32,
                h: (560.0 * scale) as i32,
            };
            let mut rect = original;
            for _ in 0..100 {
                rect = clamp_to_work(rect, work);
            }
            assert_eq!(
                (rect.x, rect.y, rect.w, rect.h),
                (original.x, original.y, original.w, original.h)
            );
        }
    }

    #[test]
    fn placement_accounts_for_top_and_left_taskbars_on_secondary_monitor() {
        let monitor = RECT {
            left: -1920,
            top: 0,
            right: 0,
            bottom: 1080,
        };
        let work = RECT {
            left: -1880,
            top: 50,
            right: 0,
            bottom: 1080,
        };
        let rect = to_workspace(
            Rect {
                x: -1800,
                y: 100,
                w: 600,
                h: 800,
            },
            monitor,
            work,
        );
        assert_eq!((rect.x, rect.y, rect.w, rect.h), (-1840, 50, 600, 800));
    }

    #[test]
    fn removed_monitor_and_parked_rect_return_inside_current_work_area() {
        let work = RECT {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1040,
        };
        for (original, expected) in [
            (
                Rect {
                    x: 2500,
                    y: 100,
                    w: 600,
                    h: 800,
                },
                (1320, 100, 600, 800),
            ),
            (
                Rect {
                    x: PARK_X,
                    y: PARK_Y,
                    w: 1,
                    h: 1,
                },
                (200, 200, 420, 560),
            ),
        ] {
            let rect = clamp_to_work(original, work);
            assert_eq!((rect.x, rect.y, rect.w, rect.h), expected);
        }
    }
    #[test]
    fn repeated_show_of_valid_visible_window_requires_no_native_restore() {
        let rect = Rect {
            x: 120,
            y: 100,
            w: 630,
            h: 840,
        };
        for _ in 0..100 {
            assert!(!needs_restore(rect, rect, false, false, true));
        }
        assert!(needs_restore(rect, rect, false, true, true));
        assert!(needs_restore(rect, rect, true, false, true));
        assert!(needs_restore(rect, rect, false, false, false));
    }
}
