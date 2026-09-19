#![windows_subsystem = "windows"]

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::time::{Duration, Instant};
use tray_icon::menu::{Menu, MenuEvent, MenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use veil_engine::{
    CcdApi, CcdConstants, DriverStatus, Gate, OpenSessionRelease, ParentWatcher, PathRole,
    RecoveryCoordinator, RecoveryCoordinatorHooks, ScreenItem, ScreenListBuilder, TopologyBlob,
    Win32CcdApi, Win32ParentWatcher,
};
use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, HANDLE, HWND};
use windows_sys::Win32::System::Threading::{CreateMutexW, ReleaseMutex};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    MessageBoxW, MB_ICONWARNING, MB_OK, MB_OKCANCEL, IDCANCEL,
};

const MUTEX_NAME: &str = "Local\\Veil";
const DEFAULT_DETAIL: &str = "托盘常驻。关面板不会退出。黑色画面不是关屏成功。";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        eprintln!("Veil.App [--restore-and-exit | --validate-keep-off-internal --seconds N --log <path>]");
        return;
    }
    if args.iter().any(|a| a == "--restore-and-exit") {
        OpenSessionRelease::request_all();
        return;
    }
    if args.iter().any(|a| a == "--validate-keep-off-internal") {
        std::process::exit(run_validate_keep_off(&args));
    }
    let Some(mutex) = try_acquire_mutex() else {
        return;
    };

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
    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("Veil")
        .with_icon(default_icon())
        .build()
        .ok();

    let native = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([420.0, 560.0])
            .with_min_inner_size([360.0, 360.0])
            .with_title("Veil")
            .with_active(true),
        renderer: eframe::Renderer::Glow,
        hardware_acceleration: eframe::HardwareAcceleration::Off,
        persist_window: false,
        ..Default::default()
    };

    let _ = eframe::run_native(
        "Veil",
        native,
        Box::new(move |_cc| Ok(Box::new(VeilApp::new(mutex, tray, open_id, restore_id, exit_id)))),
    );
}

struct VeilApp {
    mutex: HANDLE,
    open_id: tray_icon::menu::MenuId,
    restore_id: tray_icon::menu::MenuId,
    exit_id: tray_icon::menu::MenuId,
    _tray: Option<TrayIcon>,
    coordinator: RecoveryCoordinator,
    screens: Vec<ScreenItem>,
    detail: String,
    hotkey_status: String,
    startup: bool,
    last_refresh: Instant,
    hide_before_apply: bool,
    holding: bool,
    topology_fingerprint: String,
    exiting: bool,
}

impl VeilApp {
    fn new(
        mutex: HANDLE,
        tray: Option<TrayIcon>,
        open_id: tray_icon::menu::MenuId,
        restore_id: tray_icon::menu::MenuId,
        exit_id: tray_icon::menu::MenuId,
    ) -> Self {
        let mut hooks = RecoveryCoordinatorHooks::production();
        hooks.confirm_enable_vdd = Some(Box::new(confirm_enable_vdd));
        hooks.run_driver_helper = Box::new(|verb| run_helper_elevated(verb));
        hooks.is_alive = Box::new(|pid| Win32ParentWatcher.is_alive(pid).unwrap_or(false));
        let coordinator = RecoveryCoordinator::new(Box::new(Win32CcdApi), hooks);
        let mut app = Self {
            mutex,
            open_id,
            restore_id,
            exit_id,
            _tray: tray,
            coordinator,
            screens: vec![],
            detail: DEFAULT_DETAIL.into(),
            hotkey_status: format!("{}：未知", CcdConstants::HOTKEY_TEXT),
            startup: startup_enabled(),
            last_refresh: Instant::now() - Duration::from_secs(1),
            hide_before_apply: false,
            holding: false,
            topology_fingerprint: String::new(),
            exiting: false,
        };
        app.refresh();
        app
    }

    fn refresh(&mut self) {
        let snapshot = match veil_engine::CcdApi::query_snapshot(&Win32CcdApi, CcdConstants::QUERY_FLAGS) {
            Ok(s) => s,
            Err(e) => {
                self.detail = format!("无法枚举显示器：{e}");
                return;
            }
        };
        self.coordinator.poll();
        let hotkey = self.coordinator.hotkey_registered;
        self.hotkey_status = if hotkey {
            format!("{}：可用", CcdConstants::HOTKEY_TEXT)
        } else {
            format!("{}：不可用", CcdConstants::HOTKEY_TEXT)
        };
        self.screens = ScreenListBuilder::build(
            &snapshot,
            self.coordinator.heartbeat.as_ref(),
            &self.coordinator.wanted(),
            DriverStatus::installed(),
            self.coordinator.is_ready || !self.coordinator.has_session(),
            hotkey || !self.coordinator.has_session(),
        );
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
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        self.hide_before_apply = true;
        if let Some(err) = self.coordinator.keep_off(item.identity) {
            self.detail = err;
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            self.hide_before_apply = false;
        }
        self.refresh();
    }

    fn try_exit(&mut self, ctx: &egui::Context) -> bool {
        match self.coordinator.restore_all_and_wait(Duration::from_secs(20)) {
            Ok(msg) => {
                if msg.contains(RecoveryCoordinator::DISABLE_VDD_FAILED) {
                    message_box(&msg, false);
                }
                self.exiting = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                true
            }
            Err(msg) => {
                message_box(&msg, false);
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
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize([420.0, 560.0].into()));
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
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
    }
}

impl eframe::App for VeilApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        while let Ok(ev) = MenuEvent::receiver().try_recv() {
            if ev.id == self.open_id {
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                self.hide_before_apply = false;
            } else if ev.id == self.restore_id {
                let _ = self.coordinator.restore_all();
                self.refresh();
            } else if ev.id == self.exit_id && self.try_exit(ctx) {
                return;
            }
        }
        if let Ok(TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            ..
        }) = TrayIconEvent::receiver().try_recv()
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            self.hide_before_apply = false;
        }
        if self.last_refresh.elapsed() >= Duration::from_millis(800) {
            self.refresh();
        }
        self.sync_topology_viewport(ctx);
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
                            ui.colored_label(egui::Color32::from_rgb(180, 80, 40), &item.block_reason);
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
            if ui.button("恢复全部").clicked() {
                let _ = self.coordinator.restore_all();
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

        if ctx.input(|i| i.viewport().close_requested()) && !self.exiting {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if !self.mutex.is_null() {
            unsafe {
                ReleaseMutex(self.mutex);
                CloseHandle(self.mutex);
            }
        }
    }
}

fn confirm_enable_vdd() -> bool {
    message_box(&format!("{}\n\n继续？取消则物理屏不改动。", Gate::ENABLE_VDD_REASON), true)
}

fn message_box(text: &str, cancel: bool) -> bool {
    let text_w = to_wide(text);
    let caption = to_wide("Veil");
    let flags = MB_ICONWARNING | if cancel { MB_OKCANCEL } else { MB_OK };
    let rc = unsafe { MessageBoxW(0 as HWND, text_w.as_ptr(), caption.as_ptr(), flags) };
    rc != IDCANCEL
}

fn run_helper_elevated(verb: &str) -> i32 {
    use windows_sys::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;
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
    let ok = unsafe { ShellExecuteExW(&mut info) };
    if ok == 0 {
        return 1;
    }
    if !info.hProcess.is_null() {
        unsafe {
            windows_sys::Win32::System::Threading::WaitForSingleObject(info.hProcess, 60_000);
            let mut code = 1u32;
            windows_sys::Win32::System::Threading::GetExitCodeProcess(info.hProcess, &mut code);
            CloseHandle(info.hProcess);
            return code as i32;
        }
    }
    1
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
    let work = exe.parent().map(|p| p.display().to_string()).unwrap_or_default();
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
    let pixel = if holding {
        [196u8, 72, 48, 255]
    } else {
        [40u8, 40, 48, 255]
    };
    Icon::from_rgba(pixel.repeat(16 * 16), 16, 16).expect("icon")
}

fn to_wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

fn arg_value(args: &[String], name: &str) -> Option<String> {
    args.windows(2).find(|w| w[0] == name).map(|w| w[1].clone())
}

fn validate_log(path: Option<&std::path::Path>, line: &str) {
    let stamped = format!("{} {line}", chrono_like_stamp());
    eprintln!("{stamped}");
    if let Some(path) = path {
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
            use std::io::Write;
            let _ = writeln!(f, "{stamped}");
        }
    }
}

fn chrono_like_stamp() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}", now.as_secs())
}

fn run_validate_keep_off(args: &[String]) -> i32 {
    let seconds: u64 = arg_value(args, "--seconds")
        .and_then(|s| s.parse().ok())
        .unwrap_or(15);
    let wait_hotkey = args.iter().any(|a| a == "--wait-hotkey");
    let log_path = arg_value(args, "--log").map(std::path::PathBuf::from);
    let log = log_path.as_deref();
    if let Some(path) = log {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
    }
    validate_log(
        log,
        &format!("validate-keep-off-internal seconds={seconds} wait_hotkey={wait_hotkey}"),
    );
    let Some(_mutex) = try_acquire_mutex() else {
        validate_log(log, "单实例互斥失败，已有 Veil 在跑。");
        return 2;
    };
    let mut hooks = RecoveryCoordinatorHooks::production();
    hooks.confirm_enable_vdd = Some(Box::new(|| true));
    hooks.run_driver_helper = Box::new(|verb| run_helper_elevated(verb));
    hooks.is_alive = Box::new(|pid| Win32ParentWatcher.is_alive(pid).unwrap_or(false));
    let mut coordinator = RecoveryCoordinator::new(Box::new(Win32CcdApi), hooks);
    let snapshot = match CcdApi::query_snapshot(&Win32CcdApi, CcdConstants::QUERY_FLAGS) {
        Ok(s) => s,
        Err(e) => {
            validate_log(log, &format!("枚举失败：{e}"));
            return 2;
        }
    };
    let Some(internal) = snapshot
        .paths
        .iter()
        .find(|p| p.active && p.role == PathRole::Internal)
    else {
        validate_log(log, "没有活动内屏，拒绝关屏。");
        return 2;
    };
    let identity = internal.identity();
    validate_log(
        log,
        &format!(
            "target internal {} {} {}",
            identity.adapter_luid, identity.target_id, identity.monitor_path
        ),
    );
    if let Some(err) = coordinator.keep_off(identity) {
        validate_log(log, &format!("keep-off 未 APPLY：{err}"));
        return 2;
    }
    if let Some(dir) = coordinator.session_directory() {
        validate_log(log, &format!("session {}", dir.display()));
    }
    let deadline = Instant::now() + Duration::from_secs(seconds);
    while Instant::now() < deadline {
        coordinator.poll();
        if !coordinator.has_session() {
            let msg = coordinator.status_text.clone().unwrap_or_default();
            validate_log(log, &format!("会话结束：{msg}"));
            return if wait_hotkey { 0 } else { 1 };
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    if wait_hotkey {
        validate_log(log, "热键窗口结束，改写 release 兜底。");
    }
    match coordinator.restore_all_and_wait(Duration::from_secs(20)) {
        Ok(msg) => {
            validate_log(log, &format!("restore {msg}"));
            0
        }
        Err(msg) => {
            validate_log(log, &format!("restore-failed {msg}"));
            1
        }
    }
}
