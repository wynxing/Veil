use std::sync::{Arc, Mutex};
use windows_sys::Win32::Foundation::{HWND, RECT};
use windows_sys::Win32::System::Threading::GetCurrentProcessId;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, FindWindowW, GetWindowLongPtrW, GetWindowRect, GetWindowThreadProcessId,
    IsIconic, IsWindow, SetForegroundWindow, SetWindowLongPtrW, SetWindowPos, ShowWindow,
    GWL_EXSTYLE, HWND_TOP, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOZORDER, SWP_SHOWWINDOW,
    SW_RESTORE, SW_SHOW, WS_EX_TOOLWINDOW,
};

const PANEL_TITLE: &str = "Veil";
const PARK_X: i32 = -32_000;
const PARK_Y: i32 = -32_000;
const DEFAULT_X: i32 = 200;
const DEFAULT_Y: i32 = 200;
const DEFAULT_W: i32 = 420;
const DEFAULT_H: i32 = 560;

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

    pub fn ensure(&self) {
        let mut cached = self.cached.lock().unwrap_or_else(|e| e.into_inner());
        if *cached != 0 && is_window(*cached) {
            return;
        }
        *cached = find_veil_hwnd();
        if *cached == 0 {
            crate::app_log("未找到 Veil 面板 HWND。");
        } else {
            crate::app_log(&format!("面板 HWND={}", *cached));
            self.remember_rect(*cached);
        }
    }

    pub fn show(&self) {
        self.ensure();
        let hwnd = *self.cached.lock().unwrap_or_else(|e| e.into_inner());
        if hwnd == 0 {
            crate::app_log("无法显示面板：没有 HWND。");
            return;
        }
        let rect = *self.last_rect.lock().unwrap_or_else(|e| e.into_inner());
        unsafe {
            // 不使用 SW_HIDE / Visible(false)。停到屏幕外以保持 winit 事件循环。
            clear_tool_window(hwnd as HWND);
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
            BringWindowToTop(hwnd as HWND);
            let _ = SetForegroundWindow(hwnd as HWND);
        }
        crate::app_log(&format!(
            "面板已拉回 {}x{} @ {},{}",
            rect.w, rect.h, rect.x, rect.y
        ));
    }

    pub fn wake_soon() {
        std::thread::spawn(|| {
            for attempt in 0..40 {
                std::thread::sleep(std::time::Duration::from_millis(50));
                let hwnd = find_veil_hwnd();
                if hwnd == 0 {
                    continue;
                }
                unsafe {
                    show_hwnd(hwnd as HWND);
                }
                crate::app_log(&format!("后台唤醒面板 HWND={hwnd} attempt={attempt}"));
                return;
            }
            crate::app_log("后台唤醒面板超时，仍未找到 HWND。");
        });
    }

    pub fn hide(&self) {
        self.ensure();
        let hwnd = *self.cached.lock().unwrap_or_else(|e| e.into_inner());
        if hwnd == 0 {
            crate::app_log("无法隐藏面板：没有 HWND。");
            return;
        }
        self.remember_rect(hwnd);
        unsafe {
            set_tool_window(hwnd as HWND);
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
        crate::app_log("面板已停到屏幕外，事件循环保持运行。");
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

unsafe fn show_hwnd(hwnd: HWND) {
    ShowWindow(hwnd, SW_SHOW);
    if IsIconic(hwnd) != 0 {
        ShowWindow(hwnd, SW_RESTORE);
    }
    BringWindowToTop(hwnd);
    let _ = SetForegroundWindow(hwnd);
}

unsafe fn set_tool_window(hwnd: HWND) {
    let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
    SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style | WS_EX_TOOLWINDOW as isize);
}

unsafe fn clear_tool_window(hwnd: HWND) {
    let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
    SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style & !(WS_EX_TOOLWINDOW as isize));
}

fn find_veil_hwnd() -> isize {
    let title: Vec<u16> = PANEL_TITLE
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let hwnd = unsafe { FindWindowW(std::ptr::null(), title.as_ptr()) };
    if hwnd.is_null() {
        return 0;
    }
    let mut pid = 0u32;
    unsafe {
        GetWindowThreadProcessId(hwnd, &mut pid);
    }
    if pid != unsafe { GetCurrentProcessId() } {
        return 0;
    }
    hwnd as isize
}
