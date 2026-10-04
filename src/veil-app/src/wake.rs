use std::sync::{Mutex, MutexGuard};

use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::System::Threading::{CreateEventW, SetEvent};

/// Wakes the interface thread without retaining an `egui::Context` after the
/// window session ends. A bound context is only used to request a repaint
/// while that session is alive.
pub(crate) struct UiWake {
    event: usize,
    repaint: Mutex<Option<egui::Context>>,
}

impl UiWake {
    pub(crate) fn new() -> Self {
        let event = unsafe { CreateEventW(std::ptr::null(), 0, 0, std::ptr::null()) };
        Self {
            event: event as usize,
            repaint: Mutex::new(None),
        }
    }

    pub(crate) fn handle(&self) -> HANDLE {
        self.event as HANDLE
    }

    pub(crate) fn bind(&self, ctx: &egui::Context) {
        *self.repaint_slot() = Some(ctx.clone());
    }

    pub(crate) fn unbind(&self) {
        *self.repaint_slot() = None;
    }

    pub(crate) fn ping(&self) {
        if let Some(ctx) = self.repaint_slot().clone() {
            ctx.request_repaint();
        }
        let event = self.handle();
        if !event.is_null() {
            unsafe {
                SetEvent(event);
            }
        }
    }

    fn repaint_slot(&self) -> MutexGuard<'_, Option<egui::Context>> {
        self.repaint.lock().unwrap_or_else(|err| err.into_inner())
    }
}
