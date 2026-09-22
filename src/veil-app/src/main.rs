#![windows_subsystem = "windows"]

mod panel_window;

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tray_icon::menu::{Menu, MenuEvent, MenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

use panel_window::PanelWindow;
use veil_engine::{
    AuxiliaryInstallItem, BundledVddAvailability, CcdApi, CcdConstants, DriverStatus,
    OpenSessionRelease, ParentWatcher, RecoveryCoordinator, RecoveryCoordinatorHooks, ScreenItem,
    ScreenListBuilder, TopologyBlob, Win32CcdApi, Win32ParentWatcher,
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
    coordinator: RecoveryCoordinator,
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
}

impl VeilApp {
    fn new(mutex: HANDLE, show_event: HANDLE, ctx: &egui::Context) -> Self {
        let mut hooks = RecoveryCoordinatorHooks::production();
        hooks.confirm_enable_vdd = Some(Box::new(confirm_bundled_vdd));
        hooks.run_driver_helper = Box::new(|verb| run_helper_elevated(verb));
        hooks.is_alive = Box::new(|pid| Win32ParentWatcher.is_alive(pid).unwrap_or(false));
        let coordinator = RecoveryCoordinator::new(Box::new(Win32CcdApi), hooks);
        let app = Self {
            mutex,
            show_event,
            open_id: None,
            restore_id: None,
            exit_id: None,
            _tray: None,
            panel: PanelWindow::new(),
            commands: Arc::new(Mutex::new(Vec::new())),
            coordinator,
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
    }

    fn restore_all_from_tray(&mut self, ctx: &egui::Context) {
        self.open_panel(ctx);
        if let Some(e) = self.coordinator.restore_all() {
            self.detail = e;
        }
        self.refresh();
    }

    fn refresh(&mut self) {
        self.coordinator.poll();
        let snapshot =
            match veil_engine::CcdApi::query_snapshot(&Win32CcdApi, CcdConstants::QUERY_FLAGS) {
                Ok(s) => s,
                Err(e) => {
                    self.detail = format!("无法枚举显示器：{e}");
                    return;
                }
            };
        let hotkey = self.coordinator.hotkey_registered;
        self.hotkey_status = if hotkey {
            format!("{}：可用", CcdConstants::HOTKEY_TEXT)
        } else if !self.coordinator.has_session() {
            format!("{}：空闲，下次关屏前启用", CcdConstants::HOTKEY_TEXT)
        } else {
            format!("{}：不可用", CcdConstants::HOTKEY_TEXT)
        };
        let bundled = BundledVddAvailability::from_flags(
            DriverStatus::installed(),
            DriverStatus::payload_present(),
        );
        self.screens = ScreenListBuilder::build(
            &snapshot,
            self.coordinator.heartbeat.as_ref(),
            &self.coordinator.wanted(),
            bundled,
            self.coordinator.is_ready || !self.coordinator.has_session(),
            hotkey || !self.coordinator.has_session(),
        );
        if let Some(reason) = self.coordinator.recovery_block_reason() {
            for screen in &mut self.screens {
                screen.can_keep_off = false;
                screen.block_reason = reason.to_owned();
            }
        }
        self.auxiliary = AuxiliaryInstallItem::from_availability(bundled);
        if let Some(text) = &self.coordinator.status_text {
            if !text.is_empty() {
                self.detail = text.clone();
            }
        }
        self.holding = self
            .screens
            .iter()
            .any(|s| s.wanted == "保持关闭" || s.confirmed == "处理中");
        if let Some(tray) = &self._tray {
            let _ = tray.set_icon(Some(tray_icon_for(self.holding)));
        }
        self.last_refresh = Instant::now();
    }

    fn keep_off(&mut self, ctx: &egui::Context, item: ScreenItem) {
        if let Some(err) = self.coordinator.keep_off(item.identity) {
            self.open_panel(ctx);
            self.refresh();
            self.detail = err;
            return;
        }
        self.hide_panel();
        self.hide_before_apply = true;
        self.refresh();
    }

    fn try_exit(&mut self, ctx: &egui::Context) -> bool {
        match self
            .coordinator
            .restore_all_and_wait(Duration::from_secs(20))
        {
            Ok(_outcome) => {
                self.exiting = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                true
            }
            Err(msg) => {
                self.open_panel(ctx);
                message_box(&msg.to_string(), false);
                false
            }
        }
    }

    fn sync_topology_viewport(&mut self, ctx: &egui::Context) {
        let Ok(frame) = Win32CcdApi.capture(CcdConstants::QUERY_FLAGS) else {
            return;
        };
        let fingerprint = TopologyBlob::fingerprint(&frame.paths, &frame.modes);
        if self.topology_fingerprint.is_empty() {
            self.topology_fingerprint = fingerprint;
            return;
        }
        if fingerprint == self.topology_fingerprint {
            return;
        }
        self.topology_fingerprint = fingerprint;
        ctx.request_repaint();
        if self.hide_before_apply || self.coordinator.has_session() {
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
            self.refresh();
            self.sync_topology_viewport(ctx);
        }
        if self.coordinator.take_should_show_panel() {
            self.open_panel(ctx);
            self.coordinator.panel_show_attempt_finished();
        }
        ctx.request_repaint_after(Duration::from_millis(400));

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Veil");
            ui.label(&self.hotkey_status);
            ui.label(&self.detail);
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
                                .add_enabled(item.can_keep_off, egui::Button::new("保持关闭"))
                                .clicked()
                            {
                                self.keep_off(ctx, item.clone());
                            }
                            if ui
                                .add_enabled(item.can_restore, egui::Button::new("恢复"))
                                .clicked()
                            {
                                if let Some(err) = self.coordinator.restore_one(&item.identity) {
                                    self.detail = err;
                                }
                                self.refresh();
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
                        self.auxiliary.enabled,
                        egui::Button::new(&self.auxiliary.label),
                    )
                    .clicked()
                {
                    if let Some(e) = self.coordinator.install_auxiliary_output() {
                        self.detail = e;
                    } else {
                        self.detail = "辅助虚拟输出已安装，关最后一块物理屏时将启用。".into();
                    }
                    self.refresh();
                }
            }
            if ui.button("恢复全部").clicked() {
                if let Some(e) = self.coordinator.restore_all() {
                    self.detail = e;
                }
                self.refresh();
            }
            let mut startup = self.startup;
            if ui.checkbox(&mut startup, "开机自启（默认关）").changed() {
                set_startup(startup);
                self.startup = startup;
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
    fn poll_show(&self) -> bool {
        if self.show_event.is_null() {
            return false;
        }
        unsafe { WaitForSingleObject(self.show_event, 0) == 0 }
    }
}

fn confirm_bundled_vdd(reason: &str) -> bool {
    message_box(&format!("{reason}\n\n继续？取消则物理屏不改动。"), true)
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
    use windows_sys::Win32::UI::Shell::{
        ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;
    // A timed-out helper can still be running. Retain its handle and never launch
    // a second device operation until the first one has actually ended.
    static PENDING: std::sync::Mutex<Option<(usize, String)>> = std::sync::Mutex::new(None);
    let mut pending = PENDING.lock().unwrap_or_else(|e| e.into_inner());
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
    let exe = veil_engine::ProcessLaunch::driver_helper_exe_path();
    let exe_w = to_wide(&exe.to_string_lossy());
    let verb_w = to_wide("runas");
    let params = to_wide(verb);
    let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
    info.fMask = SEE_MASK_NOCLOSEPROCESS;
    info.lpVerb = verb_w.as_ptr();
    info.lpFile = exe_w.as_ptr();
    info.lpParameters = params.as_ptr();
    info.nShow = SW_HIDE;
    app_log(&format!("helper start verb={verb}"));
    if unsafe { ShellExecuteExW(&mut info) } == 0 {
        let error = unsafe { GetLastError() };
        app_log(&format!("helper launch-failed verb={verb} win32={error}"));
        return if error == 0 { 1 } else { error as i32 };
    }
    if info.hProcess.is_null() {
        app_log(&format!("helper missing-process verb={verb}"));
        return 1;
    }
    let wait = unsafe { WaitForSingleObject(info.hProcess, 60_000) };
    if wait != WAIT_OBJECT_0 {
        let error = unsafe { GetLastError() };
        *pending = Some((info.hProcess as usize, verb.to_string()));
        app_log(&format!(
            "helper wait-incomplete verb={verb} wait={wait} win32={error}"
        ));
        return if wait == WAIT_TIMEOUT { 1460 } else { 1 };
    }
    let mut code = 1;
    let ok = unsafe { GetExitCodeProcess(info.hProcess, &mut code) };
    let error = if ok == 0 {
        unsafe { GetLastError() }
    } else {
        0
    };
    unsafe {
        CloseHandle(info.hProcess);
    }
    app_log(&format!("helper end verb={verb} exit={code} win32={error}"));
    if ok != 0 {
        code as i32
    } else {
        error.max(1) as i32
    }
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
    let (fill, edge) = if holding {
        ([196u8, 72, 48, 255], [255u8, 220, 200, 255])
    } else {
        ([28u8, 140, 150, 255], [240u8, 252, 255, 255])
    };
    let mut rgba = Vec::with_capacity(16 * 16 * 4);
    for y in 0..16 {
        for x in 0..16 {
            let px = if x == 0 || y == 0 || x == 15 || y == 15 {
                edge
            } else {
                fill
            };
            rgba.extend_from_slice(&px);
        }
    }
    Icon::from_rgba(rgba, 16, 16).expect("icon")
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
