#![windows_subsystem = "windows"]

mod operation_worker;
mod panel_window;
mod session;
mod update;
mod wake;

use std::cell::RefCell;
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tray_icon::menu::{Menu, MenuEvent, MenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

use operation_worker::{Action, OperationWorker};
use panel_window::PanelWindow;
use session::{
    after_ui_session, classify_tray_commands, idle_response, merge_effect, IdleAction, LoopNext,
    PanelEffect, SessionState, SessionStop, TrayCommand, DEFAULT_DETAIL,
};
use veil_engine::{CcdApi, CcdConstants, OpenSessionRelease, Win32CcdApi};
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
static PENDING_HELPER: Mutex<Option<(usize, String)>> = Mutex::new(None);

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
    open_id: tray_icon::menu::MenuId,
    restore_id: tray_icon::menu::MenuId,
    exit_id: tray_icon::menu::MenuId,
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
    update_offer: Option<update::UpdateOffer>,
    update_inflight: bool,
    update_arm: bool,
    update_slot: Arc<Mutex<Option<Option<update::UpdateOffer>>>>,
    last_refresh: Instant,
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
    install_tray_handlers(
        &wake,
        &commands,
        open_id.clone(),
        restore_id.clone(),
        exit_id.clone(),
    );
    let (show_pending, show_stop, show_thread) = watch_show_event(show_event, Arc::clone(&wake));
    let session = Rc::new(RefCell::new(AppSession {
        mutex,
        show_event,
        wake: Arc::clone(&wake),
        tray,
        open_id,
        restore_id,
        exit_id,
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
        update_offer: None,
        update_inflight: false,
        update_arm: true,
        update_slot: Arc::new(Mutex::new(None)),
        last_refresh: Instant::now() - Duration::from_secs(1),
    }));
    let mut opened_once = false;
    loop {
        if opened_once {
            log_process_memory("面板已收起");
            match idle_until_present(&session) {
                IdleAction::Exit => break,
                IdleAction::Present => log_process_memory("准备重新打开面板"),
                IdleAction::Wait => continue,
            }
        }
        opened_once = true;
        log_process_memory("面板会话开始");
        match run_ui_once(&session) {
            Ok(stop) => match after_ui_session(stop) {
                LoopNext::Idle => continue,
                LoopNext::ExitProcess => break,
            },
            Err(err) => return Err(err),
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
        app.last_refresh = Instant::now() - Duration::from_secs(1);
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
                install_cjk_fonts(&cc.egui_ctx);
                {
                    let app = app_session.borrow();
                    app.wake.bind(&cc.egui_ctx);
                }
                Ok(Box::new(VeilApp {
                    session: app_session,
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
            .with_inner_size([420.0, 560.0])
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

fn build_tray() -> (
    Option<TrayIcon>,
    tray_icon::menu::MenuId,
    tray_icon::menu::MenuId,
    tray_icon::menu::MenuId,
) {
    let menu = Menu::new();
    let open_item = MenuItem::new("打开面板", true, None);
    let restore_item = MenuItem::new("恢复全部", true, None);
    let exit_item = MenuItem::new("退出", true, None);
    let open_id = open_item.id().clone();
    let restore_id = restore_item.id().clone();
    let exit_id = exit_item.id().clone();
    let _ = menu.append(&open_item);
    let _ = menu.append(&restore_item);
    let _ = menu.append(&exit_item);
    let tray = match TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("Veil")
        .with_icon(default_icon())
        .build()
    {
        Ok(icon) => Some(icon),
        Err(err) => {
            app_log(&format!("托盘图标未创建：{err}"));
            None
        }
    };
    (tray, open_id, restore_id, exit_id)
}

fn install_tray_handlers(
    wake: &Arc<UiWake>,
    commands: &Arc<Mutex<Vec<TrayCommand>>>,
    open_id: tray_icon::menu::MenuId,
    restore_id: tray_icon::menu::MenuId,
    exit_id: tray_icon::menu::MenuId,
) {
    let menu_wake = Arc::clone(wake);
    let menu_commands = commands.clone();
    MenuEvent::set_event_handler(Some(move |ev: MenuEvent| {
        let command = if ev.id == open_id {
            Some(TrayCommand::Open)
        } else if ev.id == restore_id {
            Some(TrayCommand::RestoreAll)
        } else if ev.id == exit_id {
            Some(TrayCommand::Exit)
        } else {
            None
        };
        if let Some(command) = command {
            menu_commands
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(command);
        }
        menu_wake.ping();
    }));

    let icon_wake = Arc::clone(wake);
    let icon_commands = commands.clone();
    TrayIconEvent::set_event_handler(Some(move |ev: TrayIconEvent| {
        if matches!(
            ev,
            TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } | TrayIconEvent::DoubleClick {
                button: MouseButton::Left,
                ..
            }
        ) {
            icon_commands
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(TrayCommand::Open);
            icon_wake.ping();
        }
    }));
}

struct VeilApp {
    session: Rc<RefCell<AppSession>>,
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
        self.last_refresh = Instant::now();
        effect
    }

    fn update_tray_icon(&self) {
        if let Some(tray) = &self.tray {
            let busy =
                self.state.holding || !self.state.pending.is_empty() || self.state.auto_stage;
            let _ = tray.set_icon(Some(tray_icon_for(busy)));
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

    fn submit(
        &mut self,
        action: Action,
        target: Option<veil_engine::ScreenIdentity>,
        label: &str,
    ) {
        if self.state.worker_failed {
            self.state.detail = "显示操作线程不可用，无法确认恢复状态。".into();
            return;
        }
        if self.state.pending.iter().any(|(_, pending)| *pending == action) {
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
                std::thread::spawn(move || {
                    let offer = update::check_remote();
                    *slot.lock().unwrap_or_else(|err| err.into_inner()) = Some(offer);
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
        while let Ok(ev) = MenuEvent::receiver().try_recv() {
            if ev.id == app.open_id {
                app.commands
                    .lock()
                    .unwrap_or_else(|err| err.into_inner())
                    .push(TrayCommand::Open);
            } else if ev.id == app.restore_id {
                app.commands
                    .lock()
                    .unwrap_or_else(|err| err.into_inner())
                    .push(TrayCommand::RestoreAll);
            } else if ev.id == app.exit_id {
                app.commands
                    .lock()
                    .unwrap_or_else(|err| err.into_inner())
                    .push(TrayCommand::Exit);
            }
        }
        while let Ok(ev) = TrayIconEvent::receiver().try_recv() {
            if matches!(
                ev,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } | TrayIconEvent::DoubleClick {
                    button: MouseButton::Left,
                    ..
                }
            ) {
                app.commands
                    .lock()
                    .unwrap_or_else(|err| err.into_inner())
                    .push(TrayCommand::Open);
            }
        }
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
        if ctx.input(|i| i.viewport().close_requested())
            && !app.state.exiting
            && !app.state.dismiss
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
            && !app.panel.is_parked()
        {
            app.state.dismiss = true;
            self.minimize_armed = false;
        }
        self.close_armed = true;
        if !app.state.dismiss && !app.state.exiting {
            app.panel.sync(hwnd, true);
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
        if app.last_refresh.elapsed() >= Duration::from_millis(400) {
            let effect = app.drain_worker();
            if effect == PanelEffect::Present && !app.state.exiting {
                app.show_window(ctx);
                self.minimize_armed = false;
            } else if effect == PanelEffect::Dismiss && !wants_foreground {
                app.state.dismiss = true;
            }
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
        } else if schedule_continuous_repaint(app.panel.is_parked()) {
            ctx.request_repaint_after(Duration::from_millis(400));
        }

        let startup_now = app.startup;
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Veil");
            ui.label(&app.state.hotkey_status);
            ui.label(&app.state.detail);
            if !app.state.pending.is_empty() || app.state.auto_stage {
                ui.group(|ui| {
                    ui.strong(&app.state.stage);
                    ui.label(format!(
                        "已等待 {} 秒（当前阶段 {} 秒）",
                        app.state.operation_started.elapsed().as_secs(),
                        app.state.stage_started.elapsed().as_secs()
                    ));
                    if app.state.cancel_requested {
                        ui.label("取消已收到；正在等待设备操作结束并确认恢复状态。");
                    } else if app
                        .state
                        .pending
                        .iter()
                        .any(|(_, action)| *action == Action::KeepOff)
                    {
                        if ui.button("取消本次操作").clicked() {
                            if let Some((_, reply)) = app.state.confirmation.take() {
                                let _ = reply.send(false);
                            }
                            app.submit(Action::Cancel, None, "取消已收到，正在确认恢复");
                            app.state.cancel_requested = true;
                        }
                    }
                });
            }
            if let Some(reason) = app
                .state
                .confirmation
                .as_ref()
                .map(|(reason, _)| reason.clone())
            {
                ui.group(|ui| {
                    ui.label(&reason);
                    ui.label("这是显示驱动操作，继续前需要你明确确认。");
                    if ui.button("继续").clicked() {
                        if let Some((_, reply)) = app.state.confirmation.take() {
                            let _ = reply.send(true);
                        }
                    }
                    if ui.button("取消").clicked() {
                        if let Some((_, reply)) = app.state.confirmation.take() {
                            let _ = reply.send(false);
                        }
                    }
                });
            }
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| {
                for item in app.state.screens.clone() {
                    ui.group(|ui| {
                        ui.horizontal(|ui| {
                            ui.strong(&item.name);
                            ui.label(format!("· {}", item.kind));
                        });
                        ui.label(&item.status_text);
                        if !item.block_reason.is_empty() {
                            ui.colored_label(
                                egui::Color32::from_rgb(180, 80, 40),
                                &item.block_reason,
                            );
                        }
                        ui.horizontal(|ui| {
                            if ui
                                .add_enabled(
                                    item.can_keep_off
                                        && app.state.pending.is_empty()
                                        && !app.state.auto_stage
                                        && !app.state.worker_failed,
                                    egui::Button::new("保持关闭"),
                                )
                                .clicked()
                            {
                                app.submit(
                                    Action::KeepOff,
                                    Some(item.identity.clone()),
                                    "正在检查显示状态和恢复能力",
                                );
                            }
                            if ui
                                .add_enabled(
                                    item.can_restore
                                        && app.state.pending.is_empty()
                                        && !app.state.auto_stage
                                        && !app.state.worker_failed,
                                    egui::Button::new("恢复"),
                                )
                                .clicked()
                            {
                                app.submit(
                                    Action::RestoreOne,
                                    Some(item.identity.clone()),
                                    "正在恢复所选物理屏",
                                );
                            }
                        });
                    });
                    ui.add_space(6.0);
                }
            });
            ui.separator();
            if app.state.auxiliary.visible {
                if !app.state.auxiliary.hint.is_empty() {
                    ui.colored_label(
                        egui::Color32::from_rgb(180, 80, 40),
                        &app.state.auxiliary.hint,
                    );
                }
                if ui
                    .add_enabled(
                        app.state.auxiliary.enabled
                            && app.state.pending.is_empty()
                            && !app.state.auto_stage
                            && !app.state.worker_failed,
                        egui::Button::new(&app.state.auxiliary.label),
                    )
                    .clicked()
                {
                    app.submit(Action::Install, None, "正在安装辅助虚拟输出");
                }
            }
            if ui.button("恢复全部").clicked() {
                app.submit(Action::RestoreAll, None, "正在恢复全部物理屏");
            }
            let mut startup = startup_now;
            if ui.checkbox(&mut startup, "开机自启（默认关）").changed() {
                set_startup(startup);
                app.startup = startup;
            }
            if let Some(offer) = app.update_offer.clone() {
                ui.label(format!("有新版本 {}", offer.version));
                if ui.button("查看更新").clicked() && !update::open_release_page(&offer.url) {
                    app.state.detail = "没能打开发布页。".into();
                }
            }
            if ui.button("退出").clicked() {
                app.show_window(ctx);
                app.submit(Action::Exit, None, "正在恢复显示，确认安全后退出");
            }
        });
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

fn run_helper_elevated(verb: &str) -> i32 {
    use windows_sys::Win32::Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT};
    use windows_sys::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject};
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

fn helper_operation_unfinished() -> bool {
    use windows_sys::Win32::Foundation::WAIT_OBJECT_0;
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

fn schedule_continuous_repaint(parked: bool) -> bool {
    !parked
}

const SHOW_EVENT_WAIT_MS: u32 = 500;

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

fn startup_path() -> std::path::PathBuf {
    let appdata = std::env::var("APPDATA").unwrap_or_default();
    std::path::PathBuf::from(appdata)
        .join("Microsoft")
        .join("Windows")
        .join("Start Menu")
        .join("Programs")
        .join("Startup")
        .join("Veil.lnk")
}

fn startup_enabled() -> bool {
    startup_path().exists()
}

fn set_startup(enabled: bool) {
    let path = startup_path();
    if !enabled {
        let _ = std::fs::remove_file(&path);
        return;
    }
    let exe = std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("Veil.App.exe"));
    let work = exe
        .parent()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    let script = format!(
        "$s=(New-Object -ComObject WScript.Shell).CreateShortcut('{}');$s.TargetPath='{}';$s.WorkingDirectory='{}';$s.Description='Veil';$s.Save()",
        path.display(),
        exe.display(),
        work
    );
    let _ = std::process::Command::new("powershell")
        .args(["-NoProfile", "-Command", &script])
        .status();
}

fn default_icon() -> Icon {
    tray_icon_for(false)
}

fn tray_icon_for(holding: bool) -> Icon {
    let bytes = if holding {
        include_bytes!("../../../assets/icon/tray-holding.png").as_slice()
    } else {
        include_bytes!("../../../assets/icon/tray-idle.png").as_slice()
    };
    let rgba = image::load_from_memory(bytes)
        .expect("tray image")
        .to_rgba8();
    Icon::from_rgba(rgba.into_raw(), 16, 16).expect("tray icon")
}

fn window_icon() -> std::sync::Arc<egui::IconData> {
    let rgba = image::load_from_memory(include_bytes!("../../../assets/icon/veil.png"))
        .expect("window image")
        .to_rgba8();
    std::sync::Arc::new(egui::IconData {
        rgba: rgba.into_raw(),
        width: 64,
        height: 64,
    })
}

fn install_cjk_fonts(ctx: &egui::Context) {
    let Some((name, data)) = load_windows_cjk_font() else {
        app_log("未找到系统中文字体，面板中文会显示为空框。");
        return;
    };
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert("veil_cjk".to_owned(), data.into());
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        if let Some(list) = fonts.families.get_mut(&family) {
            // egui 默认字体有 .notdef，CJK 放后面不会回退，中文会变成方框。
            list.insert(0, "veil_cjk".to_owned());
        }
    }
    ctx.set_fonts(fonts);
    app_log(&format!("已加载系统中文字体：{name}"));
}

fn load_windows_cjk_font() -> Option<(String, egui::FontData)> {
    let windir = std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".into());
    let dir = std::path::PathBuf::from(windir).join("Fonts");
    let candidates = [
        ("msyh.ttf", 0u32),
        ("simhei.ttf", 0),
        ("msyh.ttc", 0),
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
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        use std::io::Write;
        let _ = writeln!(f, "{} {line}", chrono_like_stamp());
    }
}

fn to_wide(s: &str) -> Vec<u16> {
    OsStr::new(s)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

fn chrono_like_stamp() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}", now.as_secs())
}

#[cfg(test)]
mod operation_tests {
    use super::*;

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

    #[test]
    fn parked_panel_does_not_schedule_continuous_repaint() {
        assert!(!schedule_continuous_repaint(true));
        assert!(schedule_continuous_repaint(false));
    }
}
