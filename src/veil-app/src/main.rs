#![windows_subsystem = "windows"]

mod elevation;
mod operation_worker;
mod panel_window;
mod session;
mod startup;
mod tray;
mod ui;
mod update;
mod wake;

use std::cell::RefCell;
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use tray_icon::TrayIcon;

use operation_worker::{Action, OperationWorker};
use panel_window::PanelWindow;
use session::{
    after_ui_session, classify_tray_commands, idle_response, merge_effect, IdleAction, LoopNext,
    PanelEffect, SessionState, SessionStop, TrayCommand, DEFAULT_DETAIL,
};
use startup::{set_startup, startup_enabled};
use tray::{build_tray, install_tray_handlers, should_update_tray_icon, tray_icon_for};
use veil_engine::{CcdApi, CcdConstants, OpenSessionRelease, VddModePreference, Win32CcdApi};
use wake::UiWake;
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, HANDLE, HWND, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows_sys::Win32::System::Threading::{
    CreateEventW, CreateMutexW, OpenEventW, ReleaseMutex, SetEvent, WaitForSingleObject,
    EVENT_MODIFY_STATE,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    MessageBoxW, IDCANCEL, MB_ICONWARNING, MB_OK, MB_OKCANCEL,
};

const MUTEX_NAME: &str = "Local\\Veil";
const SHOW_EVENT_NAME: &str = "Local\\Veil.ShowPanel";
const SHOW_EVENT_WAIT_MS: u32 = 500;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RendererChoice {
    Wgpu,
    Glow,
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        eprintln!("Veil.App [--restore-and-exit]");
        return;
    }
    if args.iter().any(|a| a == "--restore-and-exit") {
        let result = veil_engine::maintenance::require_single_interactive_session()
            .map_err(veil_engine::session::RestoreError::Failed)
            .and_then(|_| OpenSessionRelease::request_all_and_wait(Duration::from_secs(20)))
            .and_then(|outcome| {
                let snapshot = Win32CcdApi
                    .query_snapshot(CcdConstants::QUERY_FLAGS)
                    .map_err(veil_engine::session::RestoreError::Failed)?;
                if snapshot.active_physical().next().is_none() {
                    return Err(veil_engine::session::RestoreError::Failed(
                        "未确认活动物理输出，禁止移除辅助 VDD。".into(),
                    ));
                }
                Ok(outcome)
            });
        match result {
            Ok(_) => std::process::exit(0),
            Err(e) => {
                app_log(&e.to_string());
                std::process::exit(e.exit_code());
            }
        }
    }
    let Some(mutex) = try_acquire_mutex() else {
        app_log("单实例互斥失败，已有 Veil 在跑。");
        if try_signal_show() {
            app_log("已请前一实例打开面板。");
        } else {
            message_box(
                "Veil 已在运行。请看任务栏右下角托盘，或点开隐藏图标。",
                false,
            );
        }
        return;
    };
    let show_event = create_show_event();
    app_log("互斥已拿到，准备打开面板。");
    match OpenSessionRelease::prune_concluded(veil_engine::SessionPaths::root(), 10) {
        Ok(removed) if removed > 0 => app_log(&format!("已清理 {removed} 个历史会话目录。")),
        Ok(_) => {}
        Err(err) => app_log(&format!("清理历史会话目录失败：{err}")),
    }

    if let Err(err) = run_panel(mutex, show_event) {
        app_log(&format!("面板未能打开：{err}"));
        message_box(
            &format!("Veil 面板未能打开：{err}\n\n细节已写入 %TEMP%\\Veil-app.log"),
            false,
        );
    }
}

struct AppSession {
    mutex: HANDLE,
    show_event: HANDLE,
    wake: Arc<UiWake>,
    tray: Option<TrayIcon>,
    tray_busy: Option<bool>,
    commands: Arc<Mutex<Vec<TrayCommand>>>,
    deferred: Vec<TrayCommand>,
    worker: OperationWorker,
    panel: PanelWindow,
    state: SessionState,
    show_pending: Arc<AtomicBool>,
    show_stop: Arc<AtomicBool>,
    show_thread: Option<std::thread::JoinHandle<()>>,
    stop: SessionStop,
    renderer: Option<RendererChoice>,
    startup: bool,
    vdd_mode: VddModePreference,
    update_offer: Option<update::UpdateOffer>,
    update_inflight: bool,
    update_arm: bool,
    update_slot: Arc<Mutex<Option<Option<update::UpdateOffer>>>>,
}

impl Drop for AppSession {
    fn drop(&mut self) {
        self.wake.unbind();
        self.show_stop.store(true, Ordering::SeqCst);
        if !self.show_event.is_null() {
            unsafe {
                SetEvent(self.show_event);
            }
        }
        if let Some(thread) = self.show_thread.take() {
            let _ = thread.join();
        }
        if !self.show_event.is_null() {
            unsafe {
                CloseHandle(self.show_event);
            }
            self.show_event = std::ptr::null_mut();
        }
        if !self.mutex.is_null() {
            unsafe {
                ReleaseMutex(self.mutex);
                CloseHandle(self.mutex);
            }
            self.mutex = std::ptr::null_mut();
        }
    }
}

fn run_panel(mutex: HANDLE, show_event: HANDLE) -> Result<(), String> {
    let wake = Arc::new(UiWake::new());
    let (tray, open_id, restore_id, exit_id) = build_tray();
    let commands = Arc::new(Mutex::new(Vec::new()));
    install_tray_handlers(&wake, &commands, open_id, restore_id, exit_id);
    let (show_pending, show_stop, show_thread) = watch_show_event(show_event, Arc::clone(&wake));
    let session = Rc::new(RefCell::new(AppSession {
        mutex,
        show_event,
        wake: Arc::clone(&wake),
        tray,
        tray_busy: Some(false),
        commands,
        deferred: Vec::new(),
        worker: OperationWorker::start(wake),
        panel: PanelWindow::new(),
        state: SessionState::new(
            DEFAULT_DETAIL.into(),
            format!("{}：未知", CcdConstants::HOTKEY_TEXT),
        ),
        show_pending,
        show_stop,
        show_thread,
        stop: SessionStop::Dismiss,
        renderer: None,
        startup: startup_enabled(),
        vdd_mode: VddModePreference::load(),
        update_offer: None,
        update_inflight: false,
        update_arm: true,
        update_slot: Arc::new(Mutex::new(None)),
    }));
    let mut opened_once = false;
    loop {
        if opened_once {
            let _ = session
                .borrow_mut()
                .worker
                .send(Action::SetPanelOpen(false), None);
            log_process_memory("面板已收起");
            match idle_until_present(&session) {
                IdleAction::Exit => break,
                IdleAction::Present => log_process_memory("准备重新打开面板"),
                IdleAction::Wait => continue,
            }
        }
        opened_once = true;
        log_process_memory("面板会话开始");
        {
            let stop = run_ui_once(&session)?;
            match after_ui_session(stop) {
                LoopNext::Idle => continue,
                LoopNext::ExitProcess => break,
            }
        }
    }
    Ok(())
}

fn run_ui_once(session: &Rc<RefCell<AppSession>>) -> Result<SessionStop, String> {
    let attempts = {
        let mut app = session.borrow_mut();
        app.stop = SessionStop::Dismiss;
        app.state.dismiss = false;
        app.state.exiting = false;
        let _ = app.worker.send(Action::SetPanelOpen(true), None);
        app.panel.release_hwnd(0);
        match app.renderer {
            Some(choice) => vec![choice],
            None => vec![RendererChoice::Wgpu, RendererChoice::Glow],
        }
    };
    let mut last = String::from("没有可用的窗口后端");
    for choice in attempts {
        let name = match choice {
            RendererChoice::Wgpu => "wgpu",
            RendererChoice::Glow => "glow",
        };
        app_log(&format!("尝试 eframe {name}"));
        let app_session = Rc::clone(session);
        match eframe::run_native(
            "Veil",
            native_options(choice),
            Box::new(move |cc| {
                let light = ui::prefers_light_theme();
                ui::apply_theme(&cc.egui_ctx, light);
                install_cjk_fonts(&cc.egui_ctx);
                {
                    let app = app_session.borrow();
                    app.wake.bind(&cc.egui_ctx);
                }
                Ok(Box::new(VeilApp {
                    session: app_session,
                    palette: ui::ThemePalette::for_mode(light),
                    close_armed: false,
                    minimize_armed: false,
                    dismiss_logged: false,
                }))
            }),
        ) {
            Ok(()) => {
                let mut app = session.borrow_mut();
                app.wake.unbind();
                if app.renderer.is_none() {
                    app.renderer = Some(choice);
                    app_log(&format!("渲染器已选定：{name}"));
                }
                drain_thread_messages();
                return Ok(app.stop);
            }
            Err(err) => {
                session.borrow().wake.unbind();
                last = format!("{name}: {err}");
                app_log(&format!("eframe {name} 失败：{err}"));
                drain_thread_messages();
            }
        }
    }
    Err(last)
}

fn native_options(choice: RendererChoice) -> eframe::NativeOptions {
    let (renderer, accel) = match choice {
        RendererChoice::Wgpu => (
            eframe::Renderer::Wgpu,
            eframe::HardwareAcceleration::Preferred,
        ),
        RendererChoice::Glow => (eframe::Renderer::Glow, eframe::HardwareAcceleration::Off),
    };
    eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([420.0, 640.0])
            .with_min_inner_size([360.0, 360.0])
            .with_title("Veil")
            .with_icon(window_icon())
            .with_visible(true)
            .with_active(true),
        renderer,
        hardware_acceleration: accel,
        persist_window: false,
        ..Default::default()
    }
}

fn idle_until_present(session: &Rc<RefCell<AppSession>>) -> IdleAction {
    app_log("面板已退回托盘，渲染器会话已结束。");
    loop {
        let action = {
            let mut app = session.borrow_mut();
            let mut commands = app.take_commands();
            if app.poll_show() {
                commands.push(TrayCommand::Open);
            }
            let (present, deferred) = classify_tray_commands(&commands);
            app.deferred.extend(deferred);
            let effect = app.drain_worker();
            let idle = idle_response(effect);
            if matches!(idle, IdleAction::Exit) || app.stop == SessionStop::Exit {
                IdleAction::Exit
            } else if present || matches!(idle, IdleAction::Present) || !app.deferred.is_empty() {
                IdleAction::Present
            } else {
                IdleAction::Wait
            }
        };
        if action != IdleAction::Wait {
            return action;
        }
        let handle = session.borrow().wake.handle();
        wait_for_wake(handle);
    }
}

struct VeilApp {
    session: Rc<RefCell<AppSession>>,
    palette: ui::ThemePalette,
    close_armed: bool,
    minimize_armed: bool,
    dismiss_logged: bool,
}

impl AppSession {
    fn take_commands(&mut self) -> Vec<TrayCommand> {
        let mut out = std::mem::take(&mut self.deferred);
        let mut pending = self.commands.lock().unwrap_or_else(|err| err.into_inner());
        for command in pending.drain(..) {
            if !out.contains(&command) {
                out.push(command);
            }
        }
        out
    }

    fn poll_show(&self) -> bool {
        if self.show_thread.is_some() {
            return self.show_pending.swap(false, Ordering::SeqCst);
        }
        if self.show_event.is_null() {
            return false;
        }
        unsafe { WaitForSingleObject(self.show_event, 0) == WAIT_OBJECT_0 }
    }

    fn drain_worker(&mut self) -> PanelEffect {
        let mut effect = PanelEffect::Stay;
        loop {
            let event = match self.worker.events.try_recv() {
                Ok(event) => event,
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    effect = merge_effect(effect, self.state.note_worker_disconnected());
                    break;
                }
            };
            effect = merge_effect(effect, self.state.apply_event(event));
        }
        if effect == PanelEffect::Exit {
            self.state.exiting = true;
            self.stop = SessionStop::Exit;
        }
        self.update_tray_icon();
        if self.state.announce_panel_shown {
            self.state.announce_panel_shown = false;
            let _ = self.worker.send(Action::PanelShown, None);
        }
        effect
    }

    fn update_tray_icon(&mut self) {
        let busy = self.state.holding || !self.state.pending.is_empty() || self.state.auto_stage;
        if !should_update_tray_icon(self.tray_busy, busy) {
            return;
        }
        if let Some(tray) = &self.tray {
            if tray.set_icon(Some(tray_icon_for(busy))).is_ok() {
                self.tray_busy = Some(busy);
            }
        }
    }

    fn show_window(&mut self, ctx: &egui::Context) {
        self.panel.show();
        self.state.hide_before_apply = false;
        self.state.dismiss = false;
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.request_repaint();
    }

    fn submit(&mut self, action: Action, target: Option<veil_engine::ScreenIdentity>, label: &str) {
        if self.state.worker_failed {
            self.state.detail = "显示操作线程不可用，无法确认恢复状态。".into();
            return;
        }
        if self
            .state
            .pending
            .iter()
            .any(|(_, pending)| *pending == action)
        {
            return;
        }
        if matches!(action, Action::Cancel | Action::RestoreAll | Action::Exit) {
            if let Some((_, reply)) = self.state.confirmation.take() {
                let _ = reply.send(false);
            }
        }
        let was_idle = self.state.pending.is_empty();
        match self.worker.send(action, target) {
            Ok(id) => {
                self.state.pending.push((id, action));
                if action != Action::Cancel {
                    self.state.last_operation_error = None;
                }
                if was_idle {
                    self.state.operation_started = Instant::now();
                }
                self.state.stage_started = Instant::now();
                self.state.stage = label.into();
                self.state.auto_stage = false;
            }
            Err(error) => self.state.detail = error,
        }
    }

    fn consider_update(&mut self) {
        if self.state.exiting || self.state.dismiss {
            return;
        }
        if self.update_inflight {
            let finished = self
                .update_slot
                .lock()
                .unwrap_or_else(|err| err.into_inner())
                .take();
            if let Some(offer) = finished {
                self.update_inflight = false;
                self.update_offer = offer;
            }
            return;
        }
        if !self.update_arm {
            return;
        }
        self.update_arm = false;
        match update::fresh_cached_offer(update::unix_now()) {
            update::CacheRead::Fresh(offer) => self.update_offer = offer,
            update::CacheRead::Due => {
                self.update_inflight = true;
                let slot = Arc::clone(&self.update_slot);
                let wake = Arc::clone(&self.wake);
                std::thread::spawn(move || {
                    let offer = update::check_remote();
                    *slot.lock().unwrap_or_else(|err| err.into_inner()) = Some(offer);
                    wake.ping();
                });
            }
        }
    }
}

impl eframe::App for VeilApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        let hwnd = PanelWindow::hwnd_from_frame(frame);
        let mut app = self.session.borrow_mut();
        app.wake.bind(ctx);
        app.consider_update();
        if app.poll_show() {
            app.commands
                .lock()
                .unwrap_or_else(|err| err.into_inner())
                .push(TrayCommand::Open);
        }
        let commands = app.take_commands();
        let wants_foreground = commands.iter().any(|command| {
            matches!(
                command,
                TrayCommand::Open | TrayCommand::RestoreAll | TrayCommand::Exit
            )
        });
        if ctx.input(|i| i.viewport().close_requested()) && !app.state.exiting && !app.state.dismiss
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            if self.close_armed && !wants_foreground {
                app.state.dismiss = true;
            }
        }
        let minimized =
            ctx.input(|i| i.viewport().minimized.unwrap_or(false)) || app.panel.is_iconic();
        if !minimized {
            self.minimize_armed = true;
        }
        if !app.state.exiting
            && !app.state.dismiss
            && !wants_foreground
            && minimized
            && self.minimize_armed
        {
            app.state.dismiss = true;
            self.minimize_armed = false;
        }
        self.close_armed = true;
        if !app.state.dismiss && !app.state.exiting {
            app.panel.sync_open(hwnd);
        }
        for command in commands {
            if app.state.exiting {
                break;
            }
            match command {
                TrayCommand::Open => {
                    app.show_window(ctx);
                    self.minimize_armed = false;
                }
                TrayCommand::RestoreAll => {
                    app.show_window(ctx);
                    self.minimize_armed = false;
                    app.submit(Action::RestoreAll, None, "正在恢复全部物理屏");
                }
                TrayCommand::Exit => {
                    app.show_window(ctx);
                    self.minimize_armed = false;
                    app.submit(Action::Exit, None, "正在恢复显示，确认安全后退出");
                }
            }
        }
        let effect = app.drain_worker();
        if effect == PanelEffect::Present && !app.state.exiting {
            app.show_window(ctx);
            self.minimize_armed = false;
        } else if effect == PanelEffect::Dismiss && !wants_foreground {
            app.state.dismiss = true;
        }
        if app.state.dismiss && !app.state.exiting {
            app.panel.release_hwnd(hwnd);
            app.update_arm = true;
            app.stop = SessionStop::Dismiss;
            if !self.dismiss_logged {
                self.dismiss_logged = true;
                app_log("面板会话即将结束，释放渲染器。");
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        } else if app.state.exiting {
            app.stop = SessionStop::Exit;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        } else if needs_timer_repaint(!app.state.pending.is_empty(), app.state.auto_stage) {
            ctx.request_repaint_after(Duration::from_millis(500));
        }

        let ui_commands = ui::render_panel(
            ctx,
            self.palette,
            &app.state,
            app.vdd_mode,
            app.startup,
            app.update_offer.as_ref(),
        );
        for command in ui_commands {
            match command {
                ui::UiCommand::Submit {
                    action,
                    target,
                    label,
                } => app.submit(action, target, label),
                ui::UiCommand::CancelKeepOff => {
                    if let Some((_, reply)) = app.state.confirmation.take() {
                        let _ = reply.send(false);
                    }
                    app.submit(Action::Cancel, None, "取消已收到，正在确认恢复");
                    app.state.cancel_requested = true;
                }
                ui::UiCommand::ConfirmReply(accepted) => {
                    if let Some((_, reply)) = app.state.confirmation.take() {
                        let _ = reply.send(accepted);
                    }
                }
                ui::UiCommand::SetVddMode(vdd_mode) => match vdd_mode.save() {
                    Ok(()) => app.vdd_mode = vdd_mode,
                    Err(error) => {
                        app.state.detail = format!("没能保存分辨率选择：{error}");
                    }
                },
                ui::UiCommand::SetStartup(startup) => match set_startup(startup) {
                    Ok(()) => app.startup = startup,
                    Err(error) => {
                        app.state.detail = format!("没能更新开机自启：{error}");
                    }
                },
                ui::UiCommand::OpenUpdate(url) => {
                    if !update::open_release_page(&url) {
                        app.state.detail = "没能打开发布页。".into();
                    }
                }
                ui::UiCommand::ExitApp => {
                    app.show_window(ctx);
                    app.submit(Action::Exit, None, "正在恢复显示，确认安全后退出");
                }
                ui::UiCommand::OpenLogLocation => {
                    if !ui::open_log_location() {
                        app.state.detail = "没能打开日志目录。".into();
                    }
                }
                ui::UiCommand::OpenSessionsDirectory => {
                    if !ui::open_sessions_directory() {
                        app.state.detail = "没能打开会话目录。".into();
                    }
                }
                ui::UiCommand::CopyDiagnosticSummary => {
                    let summary =
                        ui::build_diagnostic_summary(&app.state, app.vdd_mode, app.startup);
                    ctx.copy_text(summary);
                    app.state.detail = "已复制诊断摘要（不含敏感设备路径）。".into();
                }
            }
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.session.borrow().wake.unbind();
    }
}

fn message_box(text: &str, cancel: bool) -> bool {
    let text_w = to_wide(text);
    let caption = to_wide("Veil");
    let flags = MB_ICONWARNING | if cancel { MB_OKCANCEL } else { MB_OK };
    let rc = unsafe { MessageBoxW(0 as HWND, text_w.as_ptr(), caption.as_ptr(), flags) };
    rc != IDCANCEL
}

fn try_acquire_mutex() -> Option<HANDLE> {
    let name = to_wide(MUTEX_NAME);
    unsafe {
        windows_sys::Win32::Foundation::SetLastError(0);
        let handle = CreateMutexW(std::ptr::null(), 1, name.as_ptr());
        if handle.is_null() {
            return None;
        }
        if GetLastError() == 183 {
            CloseHandle(handle);
            return None;
        }
        Some(handle)
    }
}

fn needs_timer_repaint(has_pending: bool, auto_stage: bool) -> bool {
    has_pending || auto_stage
}

fn watch_show_event(
    handle: HANDLE,
    wake: Arc<UiWake>,
) -> (
    Arc<AtomicBool>,
    Arc<AtomicBool>,
    Option<std::thread::JoinHandle<()>>,
) {
    let pending = Arc::new(AtomicBool::new(false));
    let stop = Arc::new(AtomicBool::new(false));
    if handle.is_null() {
        return (pending, stop, None);
    }
    let pending_bg = Arc::clone(&pending);
    let stop_bg = Arc::clone(&stop);
    let raw = handle as usize;
    let thread = std::thread::Builder::new()
        .name("veil-show-event".into())
        .spawn(move || {
            let handle = raw as HANDLE;
            while !stop_bg.load(Ordering::SeqCst) {
                let wait = unsafe { WaitForSingleObject(handle, SHOW_EVENT_WAIT_MS) };
                if stop_bg.load(Ordering::SeqCst) {
                    break;
                }
                if wait == WAIT_OBJECT_0 {
                    pending_bg.store(true, Ordering::SeqCst);
                    wake.ping();
                } else if wait != WAIT_TIMEOUT {
                    break;
                }
            }
        })
        .ok();
    (pending, stop, thread)
}

fn wait_for_wake(handle: HANDLE) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{MsgWaitForMultipleObjects, QS_ALLINPUT};
    if handle.is_null() {
        drain_thread_messages();
        return;
    }
    unsafe {
        MsgWaitForMultipleObjects(1, &handle, 0, 500, QS_ALLINPUT);
    }
    drain_thread_messages();
}

fn drain_thread_messages() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE, WM_QUIT,
    };
    unsafe {
        let mut msg = std::mem::zeroed::<MSG>();
        while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
            if msg.message == WM_QUIT {
                continue;
            }
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

fn log_process_memory(reason: &str) {
    use windows_sys::Win32::System::ProcessStatus::{
        GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX,
    };
    use windows_sys::Win32::System::Threading::GetCurrentProcess;
    unsafe {
        let mut counters = std::mem::zeroed::<PROCESS_MEMORY_COUNTERS_EX>();
        counters.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
        let ok = GetProcessMemoryInfo(
            GetCurrentProcess(),
            &mut counters as *mut PROCESS_MEMORY_COUNTERS_EX as *mut PROCESS_MEMORY_COUNTERS,
            counters.cb,
        );
        if ok != 0 {
            app_log(&format!(
                "{reason} working_set_kb={} private_kb={}",
                counters.WorkingSetSize / 1024,
                counters.PrivateUsage / 1024
            ));
        }
    }
}

fn create_show_event() -> HANDLE {
    let name = to_wide(SHOW_EVENT_NAME);
    unsafe { CreateEventW(std::ptr::null(), 0, 0, name.as_ptr()) }
}

fn try_signal_show() -> bool {
    let name = to_wide(SHOW_EVENT_NAME);
    unsafe {
        let handle = OpenEventW(EVENT_MODIFY_STATE, 0, name.as_ptr());
        if handle.is_null() {
            return false;
        }
        let ok = SetEvent(handle) != 0;
        CloseHandle(handle);
        ok
    }
}

fn window_icon() -> Arc<egui::IconData> {
    static ICON: OnceLock<Arc<egui::IconData>> = OnceLock::new();
    Arc::clone(ICON.get_or_init(|| {
        let rgba = image::load_from_memory(include_bytes!("../../../assets/icon/veil.png"))
            .expect("window image")
            .to_rgba8();
        Arc::new(egui::IconData {
            rgba: rgba.into_raw(),
            width: 64,
            height: 64,
        })
    }))
}

fn install_cjk_fonts(ctx: &egui::Context) {
    let Some((_name, data)) = cached_windows_cjk_font() else {
        app_log("未找到系统中文字体，面板中文会显示为空框。");
        return;
    };
    let mut fonts = egui::FontDefinitions::default();
    fonts
        .font_data
        .insert("veil_cjk".to_owned(), Arc::clone(data));
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        if let Some(list) = fonts.families.get_mut(&family) {
            // egui 默认字体有 .notdef，CJK 放后面不会回退，中文会变成方框。
            list.insert(0, "veil_cjk".to_owned());
        }
    }
    ctx.set_fonts(fonts);
}

fn cached_windows_cjk_font() -> Option<&'static (String, Arc<egui::FontData>)> {
    static CJK_FONT: OnceLock<Option<(String, Arc<egui::FontData>)>> = OnceLock::new();
    CJK_FONT
        .get_or_init(|| {
            let loaded = load_windows_cjk_font();
            if let Some((name, _)) = &loaded {
                app_log(&format!("已加载系统中文字体：{name}"));
            }
            loaded.map(|(name, data)| (name, Arc::new(data)))
        })
        .as_ref()
}

fn load_windows_cjk_font() -> Option<(String, egui::FontData)> {
    let windir = std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".into());
    let dir = std::path::PathBuf::from(windir).join("Fonts");
    let candidates = [
        ("msyh.ttc", 0u32),
        ("msyh.ttf", 0),
        ("simhei.ttf", 0),
        ("simsun.ttc", 0),
    ];
    for (file, index) in candidates {
        let path = dir.join(file);
        match std::fs::read(&path) {
            Ok(bytes) if !bytes.is_empty() => {
                let mut data = egui::FontData::from_owned(bytes);
                data.index = index;
                return Some((path.display().to_string(), data));
            }
            Ok(_) => app_log(&format!("字体文件为空：{}", path.display())),
            Err(err) => app_log(&format!("未读取 {}: {err}", path.display())),
        }
    }
    None
}

pub(crate) fn app_log(line: &str) {
    let path = std::env::temp_dir().join("Veil-app.log");
    veil_engine::append_rotated_log(&path, line, veil_engine::DEFAULT_LOG_MAX_BYTES);
}

pub(crate) fn to_wide(s: &str) -> Vec<u16> {
    OsStr::new(s)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

#[cfg(test)]
mod operation_tests {
    use super::*;

    #[test]
    fn timer_repaint_only_runs_while_busy() {
        assert!(!needs_timer_repaint(false, false));
        assert!(needs_timer_repaint(true, false));
        assert!(needs_timer_repaint(false, true));
    }
}
