#![windows_subsystem = "windows"]

mod operation_worker;
mod panel_window;
mod update;

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tray_icon::menu::{Menu, MenuEvent, MenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

use operation_worker::{Action, Event as OperationEvent, OperationWorker};
use panel_window::PanelWindow;
use veil_engine::{
    AuxiliaryInstallItem, CcdApi, CcdConstants, OpenSessionRelease, ScreenItem, Win32CcdApi,
};
use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, HANDLE, HWND};
use windows_sys::Win32::System::Threading::{
    CreateEventW, CreateMutexW, OpenEventW, ReleaseMutex, SetEvent, WaitForSingleObject,
    EVENT_MODIFY_STATE,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    MessageBoxW, IDCANCEL, MB_ICONWARNING, MB_OK, MB_OKCANCEL,
};

const MUTEX_NAME: &str = "Local\\Veil";
const SHOW_EVENT_NAME: &str = "Local\\Veil.ShowPanel";
const DEFAULT_DETAIL: &str = "托盘常驻。关面板退回托盘，不退出。黑色画面不是关屏成功。";
static PENDING_HELPER: Mutex<Option<(usize, String)>> = Mutex::new(None);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TrayCommand {
    Open,
    RestoreAll,
    Exit,
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

fn run_panel(mutex: HANDLE, show_event: HANDLE) -> Result<(), String> {
    let attempts = [
        (
            "wgpu",
            eframe::Renderer::Wgpu,
            eframe::HardwareAcceleration::Preferred,
        ),
        (
            "glow",
            eframe::Renderer::Glow,
            eframe::HardwareAcceleration::Off,
        ),
    ];
    let mut last = String::from("没有可用的窗口后端");
    for (name, renderer, accel) in attempts {
        let native = eframe::NativeOptions {
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
        };
        app_log(&format!("尝试 eframe {name}"));
        match eframe::run_native(
            "Veil",
            native,
            Box::new(move |cc| {
                install_cjk_fonts(&cc.egui_ctx);
                Ok(Box::new(VeilApp::new(mutex, show_event, &cc.egui_ctx)))
            }),
        ) {
            Ok(()) => return Ok(()),
            Err(err) => {
                last = format!("{name}: {err}");
                app_log(&format!("eframe {name} 失败：{err}"));
            }
        }
    }
    Err(last)
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
    ctx: &egui::Context,
    panel: &PanelWindow,
    commands: &Arc<Mutex<Vec<TrayCommand>>>,
    open_id: tray_icon::menu::MenuId,
    restore_id: tray_icon::menu::MenuId,
    exit_id: tray_icon::menu::MenuId,
) {
    let menu_ctx = ctx.clone();
    let menu_panel = panel.clone();
    let menu_commands = commands.clone();
    MenuEvent::set_event_handler(Some(move |ev: MenuEvent| {
        let command = if ev.id == open_id {
            menu_panel.show();
            Some(TrayCommand::Open)
        } else if ev.id == restore_id {
            menu_panel.show();
            Some(TrayCommand::RestoreAll)
        } else if ev.id == exit_id {
            menu_panel.show();
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
        menu_ctx.request_repaint();
    }));

    let icon_ctx = ctx.clone();
    let icon_panel = panel.clone();
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
            icon_panel.show();
            icon_commands
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(TrayCommand::Open);
            icon_ctx.request_repaint();
        }
    }));
}

struct VeilApp {
    mutex: HANDLE,
    show_event: HANDLE,
    open_id: Option<tray_icon::menu::MenuId>,
    restore_id: Option<tray_icon::menu::MenuId>,
    exit_id: Option<tray_icon::menu::MenuId>,
    _tray: Option<TrayIcon>,
    panel: PanelWindow,
    commands: Arc<Mutex<Vec<TrayCommand>>>,
    worker: OperationWorker,
    pending: Vec<(u64, Action)>,
    operation_started: Instant,
    stage_started: Instant,
    stage: String,
    auto_stage: bool,
    cancel_requested: bool,
    last_operation_error: Option<String>,
    worker_failed: bool,
    confirmation: Option<(String, std::sync::mpsc::Sender<bool>)>,
    has_session: bool,
    screens: Vec<ScreenItem>,
    auxiliary: AuxiliaryInstallItem,
    detail: String,
    hotkey_status: String,
    startup: bool,
    last_refresh: Instant,
    hide_before_apply: bool,
    panel_open: bool,
    close_armed: bool,
    minimize_armed: bool,
    holding: bool,
    topology_fingerprint: String,
    exiting: bool,
    update_offer: Option<update::UpdateOffer>,
    update_inflight: bool,
    update_arm: bool,
    update_slot: Arc<Mutex<Option<Option<update::UpdateOffer>>>>,
}

impl VeilApp {
    fn new(mutex: HANDLE, show_event: HANDLE, ctx: &egui::Context) -> Self {
        let worker = OperationWorker::start(ctx.clone());
        let app = Self {
            mutex,
            show_event,
            open_id: None,
            restore_id: None,
            exit_id: None,
            _tray: None,
            panel: PanelWindow::new(),
            commands: Arc::new(Mutex::new(Vec::new())),
            worker,
            pending: Vec::new(),
            operation_started: Instant::now(),
            stage_started: Instant::now(),
            stage: String::new(),
            auto_stage: false,
            cancel_requested: false,
            last_operation_error: None,
            worker_failed: false,
            confirmation: None,
            has_session: false,
            screens: vec![],
            auxiliary: AuxiliaryInstallItem::from_availability(false),
            detail: DEFAULT_DETAIL.into(),
            hotkey_status: format!("{}：未知", CcdConstants::HOTKEY_TEXT),
            startup: startup_enabled(),
            last_refresh: Instant::now() - Duration::from_secs(1),
            hide_before_apply: false,
            panel_open: true,
            close_armed: false,
            minimize_armed: false,
            holding: false,
            topology_fingerprint: String::new(),
            exiting: false,
            update_offer: None,
            update_inflight: false,
            update_arm: true,
            update_slot: Arc::new(Mutex::new(None)),
        };
        app_log("界面对象已创建。");
        ctx.request_repaint();
        app
    }

    fn ensure_tray(&mut self, ctx: &egui::Context) {
        if self._tray.is_some() || self.open_id.is_some() {
            return;
        }
        let (tray, open_id, restore_id, exit_id) = build_tray();
        install_tray_handlers(
            ctx,
            &self.panel,
            &self.commands,
            open_id.clone(),
            restore_id.clone(),
            exit_id.clone(),
        );
        self.open_id = Some(open_id);
        self.restore_id = Some(restore_id);
        self.exit_id = Some(exit_id);
        self._tray = tray;
    }

    fn queue(&self, command: TrayCommand) {
        self.commands
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(command);
    }

    fn take_commands(&self) -> Vec<TrayCommand> {
        let mut pending = self.commands.lock().unwrap_or_else(|e| e.into_inner());
        let mut out = Vec::new();
        for command in pending.drain(..) {
            if !out.contains(&command) {
                out.push(command);
            }
        }
        out
    }

    fn open_panel(&mut self, ctx: &egui::Context) {
        self.panel.show();
        self.panel_open = true;
        self.hide_before_apply = false;
        self.minimize_armed = false;
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.request_repaint();
    }

    fn hide_panel(&mut self) {
        if !self.panel_open {
            return;
        }
        self.panel.hide();
        self.panel_open = false;
        self.update_arm = true;
    }

    fn restore_all_from_tray(&mut self, ctx: &egui::Context) {
        self.open_panel(ctx);
        self.submit(Action::RestoreAll, None, "正在恢复全部物理屏");
    }

    fn refresh(&mut self, ctx: &egui::Context) {
        loop {
            let event = match self.worker.events.try_recv() {
                Ok(event) => event,
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    if !self.worker_failed {
                        self.worker_failed = true;
                        self.pending.clear();
                        self.stage.clear();
                        self.detail = "显示操作线程已退出，当前恢复状态未知。请使用仍可用的紧急热键，并保留日志。".into();
                        self.open_panel(ctx);
                    }
                    break;
                }
            };
            match event {
                OperationEvent::Fault(error) => {
                    self.detail = error;
                    self.screens.clear();
                    self.auto_stage = false;
                }
                OperationEvent::View(view) => {
                    self.screens = view.screens;
                    self.auxiliary = view.auxiliary;
                    self.hotkey_status = view.hotkey_status;
                    self.holding = view.holding;
                    self.has_session = view.has_session;
                    if self.pending.is_empty() {
                        if let Some(detail) = view.detail.filter(|text| !text.is_empty()) {
                            self.detail = match &self.last_operation_error {
                                Some(error) if !detail.contains(error) => {
                                    format!("{error} 当前状态：{detail}")
                                }
                                _ => detail,
                            };
                        }
                    }
                    self.sync_topology_viewport(ctx, &view.topology_fingerprint);
                    self.auto_stage = false;
                }
                OperationEvent::Phase(phase) => {
                    if self.pending.is_empty() {
                        self.operation_started = Instant::now();
                    }
                    self.stage = phase;
                    self.stage_started = Instant::now();
                    self.auto_stage = self.pending.is_empty();
                }
                OperationEvent::Confirm { reason, reply } => {
                    self.stage = "等待你确认是否启用显示驱动".into();
                    self.stage_started = Instant::now();
                    self.confirmation = Some((reason, reply));
                    self.open_panel(ctx);
                }
                OperationEvent::Done { id, action, error } => {
                    self.pending.retain(|(pending_id, _)| *pending_id != id);
                    if let Some(error) = error {
                        self.last_operation_error = Some(error.clone());
                        self.detail = error;
                        self.open_panel(ctx);
                    } else {
                        if matches!(action, Action::Cancel | Action::RestoreAll | Action::Exit) {
                            self.last_operation_error = None;
                        }
                        match action {
                            Action::KeepOff
                                if !self.cancel_requested && self.pending.is_empty() =>
                            {
                                self.hide_panel();
                                self.hide_before_apply = true;
                            }
                            Action::RestoreAll => self.detail = "恢复全部已确认完成。".into(),
                            Action::Cancel => self.detail = "取消已处理；恢复结果已确认。".into(),
                            Action::Install => self.detail = "辅助虚拟输出已安装。".into(),
                            _ => {}
                        }
                    }
                    if self.pending.is_empty() {
                        self.stage.clear();
                        self.cancel_requested = false;
                    }
                }
                OperationEvent::ShowPanel => {
                    self.open_panel(ctx);
                    let _ = self.worker.send(Action::PanelShown, None);
                }
                OperationEvent::ExitReady => {
                    self.exiting = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
        if let Some(tray) = &self._tray {
            let _ = tray.set_icon(Some(tray_icon_for(
                self.holding || !self.pending.is_empty() || self.auto_stage,
            )));
        }
        self.last_refresh = Instant::now();
    }

    fn submit(&mut self, action: Action, target: Option<veil_engine::ScreenIdentity>, label: &str) {
        if self.worker_failed {
            self.detail = "显示操作线程不可用，无法确认恢复状态。".into();
            return;
        }
        if self.pending.iter().any(|(_, pending)| *pending == action) {
            return;
        }
        if matches!(action, Action::Cancel | Action::RestoreAll | Action::Exit) {
            if let Some((_, reply)) = self.confirmation.take() {
                let _ = reply.send(false);
            }
        }
        let was_idle = self.pending.is_empty();
        match self.worker.send(action, target) {
            Ok(id) => {
                self.pending.push((id, action));
                if action != Action::Cancel {
                    self.last_operation_error = None;
                }
                if was_idle {
                    self.operation_started = Instant::now();
                }
                self.stage_started = Instant::now();
                self.stage = label.into();
                self.auto_stage = false;
            }
            Err(error) => self.detail = error,
        }
    }

    fn keep_off(&mut self, _ctx: &egui::Context, item: ScreenItem) {
        self.submit(
            Action::KeepOff,
            Some(item.identity),
            "正在检查显示状态和恢复能力",
        );
    }

    fn try_exit(&mut self, ctx: &egui::Context) -> bool {
        self.open_panel(ctx);
        self.submit(Action::Exit, None, "正在恢复显示，确认安全后退出");
        false
    }

    fn sync_topology_viewport(&mut self, ctx: &egui::Context, fingerprint: &str) {
        if self.topology_fingerprint.is_empty() {
            self.topology_fingerprint = fingerprint.into();
            return;
        }
        if fingerprint == self.topology_fingerprint {
            return;
        }
        self.topology_fingerprint = fingerprint.into();
        ctx.request_repaint();
        if self.hide_before_apply || self.has_session {
            return;
        }
        if self.holding {
            self.detail = "显示拓扑已变化，保持关闭已结束。".into();
            self.holding = false;
            if let Some(tray) = &self._tray {
                let _ = tray.set_icon(Some(tray_icon_for(false)));
            }
            self.open_panel(ctx);
        }
    }
}

impl eframe::App for VeilApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        let hwnd = PanelWindow::hwnd_from_frame(frame);
        self.ensure_tray(ctx);
        self.consider_update();
        while let Ok(ev) = MenuEvent::receiver().try_recv() {
            if self.open_id.as_ref() == Some(&ev.id) {
                self.queue(TrayCommand::Open);
            } else if self.restore_id.as_ref() == Some(&ev.id) {
                self.queue(TrayCommand::RestoreAll);
            } else if self.exit_id.as_ref() == Some(&ev.id) {
                self.queue(TrayCommand::Exit);
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
                self.queue(TrayCommand::Open);
            }
        }
        if self.poll_show() {
            self.queue(TrayCommand::Open);
        }
        let commands = self.take_commands();
        let wants_foreground = commands.iter().any(|c| {
            matches!(
                c,
                TrayCommand::Open | TrayCommand::RestoreAll | TrayCommand::Exit
            )
        });
        if ctx.input(|i| i.viewport().close_requested()) && !self.exiting {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            if self.close_armed && !wants_foreground {
                self.hide_panel();
            }
        }
        let minimized =
            ctx.input(|i| i.viewport().minimized.unwrap_or(false)) || self.panel.is_iconic();
        if !minimized {
            self.minimize_armed = true;
        }
        if self.panel_open
            && !self.exiting
            && !wants_foreground
            && minimized
            && self.minimize_armed
            && !self.panel.is_parked()
        {
            self.hide_panel();
            self.minimize_armed = false;
        }
        self.close_armed = true;
        for command in commands {
            match command {
                TrayCommand::Open => self.open_panel(ctx),
                TrayCommand::RestoreAll => self.restore_all_from_tray(ctx),
                TrayCommand::Exit => {
                    if self.try_exit(ctx) {
                        return;
                    }
                }
            }
        }
        self.panel.sync(hwnd, self.panel_open && !self.exiting);
        if self.last_refresh.elapsed() >= Duration::from_millis(400) {
            self.refresh(ctx);
        }
        ctx.request_repaint_after(Duration::from_millis(400));

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Veil");
            ui.label(&self.hotkey_status);
            ui.label(&self.detail);
            if !self.pending.is_empty() || self.auto_stage {
                ui.group(|ui| {
                    ui.strong(&self.stage);
                    ui.label(format!(
                        "已等待 {} 秒（当前阶段 {} 秒）",
                        self.operation_started.elapsed().as_secs(),
                        self.stage_started.elapsed().as_secs()
                    ));
                    if self.cancel_requested {
                        ui.label("取消已收到；正在等待设备操作结束并确认恢复状态。");
                    } else if self
                        .pending
                        .iter()
                        .any(|(_, action)| *action == Action::KeepOff)
                    {
                        if ui.button("取消本次操作").clicked() {
                            if let Some((_, reply)) = self.confirmation.take() {
                                let _ = reply.send(false);
                            }
                            self.submit(Action::Cancel, None, "取消已收到，正在确认恢复");
                            self.cancel_requested = true;
                        }
                    }
                });
            }
            if let Some(reason) = self.confirmation.as_ref().map(|(reason, _)| reason.clone()) {
                ui.group(|ui| {
                    ui.label(&reason);
                    ui.label("这是显示驱动操作，继续前需要你明确确认。");
                    if ui.button("继续").clicked() {
                        if let Some((_, reply)) = self.confirmation.take() {
                            let _ = reply.send(true);
                        }
                    }
                    if ui.button("取消").clicked() {
                        if let Some((_, reply)) = self.confirmation.take() {
                            let _ = reply.send(false);
                        }
                    }
                });
            }
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| {
                for item in self.screens.clone() {
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
                                        && self.pending.is_empty()
                                        && !self.auto_stage
                                        && !self.worker_failed,
                                    egui::Button::new("保持关闭"),
                                )
                                .clicked()
                            {
                                self.keep_off(ctx, item.clone());
                            }
                            if ui
                                .add_enabled(
                                    item.can_restore
                                        && self.pending.is_empty()
                                        && !self.auto_stage
                                        && !self.worker_failed,
                                    egui::Button::new("恢复"),
                                )
                                .clicked()
                            {
                                self.submit(
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
            if self.auxiliary.visible {
                if !self.auxiliary.hint.is_empty() {
                    ui.colored_label(egui::Color32::from_rgb(180, 80, 40), &self.auxiliary.hint);
                }
                if ui
                    .add_enabled(
                        self.auxiliary.enabled
                            && self.pending.is_empty()
                            && !self.auto_stage
                            && !self.worker_failed,
                        egui::Button::new(&self.auxiliary.label),
                    )
                    .clicked()
                {
                    self.submit(Action::Install, None, "正在安装辅助虚拟输出");
                }
            }
            if ui.button("恢复全部").clicked() {
                self.submit(Action::RestoreAll, None, "正在恢复全部物理屏");
            }
            let mut startup = self.startup;
            if ui.checkbox(&mut startup, "开机自启（默认关）").changed() {
                set_startup(startup);
                self.startup = startup;
            }
            if let Some(offer) = self.update_offer.clone() {
                ui.label(format!("有新版本 {}", offer.version));
                if ui.button("查看更新").clicked() && !update::open_release_page(&offer.url) {
                    self.detail = "没能打开发布页。".into();
                }
            }
            if ui.button("退出").clicked() {
                self.try_exit(ctx);
            }
        });
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
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

impl VeilApp {
    fn consider_update(&mut self) {
        if self.exiting {
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
        if !self.panel_open {
            self.update_arm = true;
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

    fn poll_show(&self) -> bool {
        if self.show_event.is_null() {
            return false;
        }
        unsafe { WaitForSingleObject(self.show_event, 0) == 0 }
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
}
