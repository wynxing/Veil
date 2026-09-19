use crate::capability::Gate;
use crate::native::{CcdApi, CcdConstants, ParentWatcher, Win32ParentWatcher};
use crate::planner::DisplayPlanner;
use crate::process::ProcessLaunch;
use crate::session::{
    ArmFile, HeartbeatFile, IntentFile, JsonUtil, ReadyFile, ReleaseFile, ResultFile, ScreenIdentityDto,
    SessionLog, SessionPaths, unix_seconds,
};
use crate::topology::TopologyBlob;
use crate::{KeepOffAction, ScreenIdentity};
use std::path::PathBuf;
use std::time::{Duration, Instant};

pub const ENABLE_VDD_CANCELLED: &str = "已取消启用隐藏辅助输出，物理屏未改动。";
pub const DISABLE_VDD_FAILED: &str = "自带 VDD 未能禁用。";
pub const RECOVERY_EXIT_REASON: &str = "recovery-exit";
pub const RECOVERY_EXITED: &str = "恢复进程已退出。";
pub const RECOVERY_EXITED_LAST_PATH: &str = "恢复进程已退出，未禁用自带 VDD（避免关掉最后活动路径）。";

pub struct RecoveryCoordinatorHooks {
    pub start_recovery: Box<dyn FnMut(&str, Option<&str>) -> i32>,
    pub run_driver_helper: Box<dyn FnMut(&str) -> i32>,
    pub confirm_enable_vdd: Option<Box<dyn FnMut() -> bool>>,
    pub bundled_vdd_installed: Box<dyn Fn() -> bool>,
    pub is_alive: Box<dyn Fn(i32) -> bool>,
    pub virtual_path_wait: Duration,
}

impl RecoveryCoordinatorHooks {
    pub fn production() -> Self {
        Self {
            start_recovery: Box::new(|dir, parent| {
                start_recovery_process(dir, parent).unwrap_or(0)
            }),
            run_driver_helper: Box::new(|verb| run_driver_helper_process(verb).unwrap_or(1)),
            confirm_enable_vdd: None,
            bundled_vdd_installed: Box::new(|| DriverStatus::installed()),
            is_alive: Box::new(|pid| Win32ParentWatcher.is_alive(pid).unwrap_or(false)),
            virtual_path_wait: Duration::from_secs(15),
        }
    }
}

pub struct RecoveryCoordinator {
    ccd: Box<dyn CcdApi>,
    hooks: RecoveryCoordinatorHooks,
    directory: Option<PathBuf>,
    recovery_pid: i32,
    vdd_request_served: Option<PathBuf>,
    intent: IntentFile,
    pub is_ready: bool,
    pub hotkey_registered: bool,
    pub heartbeat: Option<HeartbeatFile>,
    pub status_text: Option<String>,
}

impl RecoveryCoordinator {
    pub fn new(ccd: Box<dyn CcdApi>, hooks: RecoveryCoordinatorHooks) -> Self {
        Self {
            ccd,
            hooks,
            directory: None,
            recovery_pid: 0,
            vdd_request_served: None,
            intent: IntentFile::default(),
            is_ready: false,
            hotkey_registered: false,
            heartbeat: None,
            status_text: None,
        }
    }

    pub fn has_session(&self) -> bool {
        match &self.directory {
            Some(dir) => !SessionPaths::result(dir).exists(),
            None => false,
        }
    }

    pub fn session_directory(&self) -> Option<&std::path::Path> {
        self.directory.as_deref()
    }

    pub fn wanted(&self) -> Vec<ScreenIdentity> {
        self.intent.keep_off.iter().map(|x| x.to_identity()).collect()
    }

    pub fn poll(&mut self) {
        let Some(dir) = self.directory.clone() else {
            return;
        };
        if !SessionPaths::result(&dir).exists() && self.recovery_pid > 0 && !(self.hooks.is_alive)(self.recovery_pid) {
            let _ = JsonUtil::write_atomic(
                SessionPaths::result(&dir),
                &ResultFile {
                    reason: RECOVERY_EXIT_REASON.into(),
                    ok: false,
                    error: Some(RECOVERY_EXITED.into()),
                    ..Default::default()
                },
            );
            SessionLog::append(&dir, "finish", Some(RECOVERY_EXITED), Some(RECOVERY_EXIT_REASON), None, None);
        }
        self.serve_vdd_request();
        self.heartbeat = JsonUtil::try_read(SessionPaths::heartbeat(&dir));
        if let Some(hb) = &self.heartbeat {
            self.hotkey_registered = hb.hotkey_registered;
            self.is_ready = hb.armed || SessionPaths::ready(&dir).exists();
            if let Some(detail) = &hb.detail {
                if !detail.is_empty() {
                    self.status_text = Some(detail.clone());
                }
            }
        }
        if SessionPaths::result(&dir).exists() {
            let used_bundled_vdd = self.intent.vdd_assist;
            let result = JsonUtil::try_read::<ResultFile>(SessionPaths::result(&dir));
            let session_name = dir.file_name().and_then(|s| s.to_str()).unwrap_or("").to_string();
            self.heartbeat = None;
            self.status_text = Some(Self::format_result(result.as_ref()));
            if !session_name.is_empty() {
                if let Some(text) = &mut self.status_text {
                    text.push_str(" 记录：");
                    text.push_str(&session_name);
                }
            }
            self.is_ready = false;
            self.hotkey_registered = false;
            self.directory = None;
            self.recovery_pid = 0;
            self.vdd_request_served = None;
            self.intent = IntentFile::default();
            if used_bundled_vdd {
                self.disable_bundled_vdd_after_session(result.as_ref());
            }
        }
    }

    pub fn format_result(result: Option<&ResultFile>) -> String {
        let Some(result) = result else {
            return "恢复已结束，状态未知。".into();
        };
        let mut text = match result.reason.as_str() {
            "release" => "已恢复全部。".into(),
            "hotkey" => "已由 Ctrl+Alt+Shift+F10 恢复。".into(),
            "parent-exit" => "界面退出后已恢复。".into(),
            "execution-gap" => "会话中断，保持关闭已结束。".into(),
            "unexpected-topology" => "显示拓扑已变化，保持关闭已结束。".into(),
            RECOVERY_EXIT_REASON => RECOVERY_EXITED.into(),
            _ => {
                if result.error.as_deref().unwrap_or("").is_empty() {
                    "恢复已结束。".into()
                } else {
                    result.error.clone().unwrap()
                }
            }
        };
        if result.ok {
            return text;
        }
        if let Some(err) = &result.error {
            if !err.is_empty() && result.reason != "execution-gap" && result.reason != "unexpected-topology" {
                return err.clone();
            }
        }
        if result.reapply_attempted {
            text.push_str(" 已尝试再关一次。");
        }
        if restore_failed(result) {
            text.push_str(" 恢复未完全成功。");
        }
        text
    }

    fn serve_vdd_request(&mut self) {
        let Some(dir) = &self.directory else {
            return;
        };
        if self.vdd_request_served.as_ref() == Some(dir)
            || !SessionPaths::vdd_request(dir).exists()
            || SessionPaths::result(dir).exists()
        {
            return;
        }
        self.vdd_request_served = Some(dir.clone());
        SessionLog::append(dir, "vdd-enable", Some("界面按再关请求启用自带 VDD。"), None, None, None);
        let _ = (self.hooks.run_driver_helper)("enable");
    }

    pub fn keep_off(&mut self, identity: ScreenIdentity) -> Option<String> {
        let mut selected = self.wanted();
        if !selected.iter().any(|id| {
            id.adapter_luid == identity.adapter_luid
                && id.target_id == identity.target_id
                && id.monitor_path == identity.monitor_path
        }) {
            selected.push(identity);
        }
        self.apply_intent(selected)
    }

    pub fn restore_one(&mut self, identity: &ScreenIdentity) -> Option<String> {
        let selected: Vec<_> = self.wanted().into_iter().filter(|id| !id.matches(identity)).collect();
        self.apply_intent(selected)
    }

    pub fn restore_all(&mut self) -> Option<String> {
        let Some(dir) = &self.directory else {
            return None;
        };
        let _ = JsonUtil::write_atomic(SessionPaths::release(dir), &ReleaseFile { at: unix_seconds() });
        None
    }

    pub fn restore_all_and_wait(&mut self, timeout: Duration) -> Result<String, String> {
        if self.directory.is_none() {
            return Ok(String::new());
        }
        self.restore_all();
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            self.poll();
            if self.directory.is_none() {
                return Ok(self.status_text.clone().unwrap_or_default());
            }
            std::thread::sleep(Duration::from_millis(150));
        }
        Err("恢复超时或失败，未退出。请检查画面，必要时 Win+Ctrl+Shift+B。".into())
    }

    fn apply_intent(&mut self, selected: Vec<ScreenIdentity>) -> Option<String> {
        if selected.is_empty() {
            return self.restore_all();
        }
        let previous = self.wanted();
        let shrink = previous.iter().any(|old| !selected.iter().any(|id| id.matches(old)));
        if shrink {
            return self.write_shrunk_intent(selected);
        }
        let mut snapshot = match self.ccd.query_snapshot(CcdConstants::QUERY_FLAGS) {
            Ok(s) => s,
            Err(e) => return Some(e),
        };
        let mut plan = Gate::plan_keep_off(&snapshot, &selected, (self.hooks.bundled_vdd_installed)());
        if plan.action == KeepOffAction::Blocked {
            return plan.block_reason;
        }
        if plan.action == KeepOffAction::EnableBundledVdd {
            if let Some(confirm) = &mut self.hooks.confirm_enable_vdd {
                if !confirm() {
                    return Some(ENABLE_VDD_CANCELLED.into());
                }
            }
            let helper_rc = (self.hooks.run_driver_helper)("enable");
            if helper_rc != 0 {
                return Some("自带 VDD 未能启用，物理屏未改动。".into());
            }
            let wait_until = Instant::now() + self.hooks.virtual_path_wait;
            while Instant::now() < wait_until {
                snapshot = match self.ccd.query_snapshot(CcdConstants::QUERY_FLAGS) {
                    Ok(s) => s,
                    Err(e) => return Some(e),
                };
                if snapshot.has_active_bundled_vdd() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(400));
            }
            if !snapshot.has_active_bundled_vdd() {
                self.append_disable_result();
                return Some("自带 VDD 未能出现活动虚拟路径，物理屏未改动。".into());
            }
            plan = Gate::plan_keep_off(&snapshot, &selected, true);
            if plan.action != KeepOffAction::Deactivate {
                self.append_disable_result();
                return Some(plan.block_reason.unwrap_or_else(|| "启用自带 VDD 后仍无法保持关闭。".into()));
            }
        }
        let planned = match DisplayPlanner::validate_deactivate(self.ccd.as_ref(), &selected, plan.adjust_origin) {
            Ok(p) => p,
            Err(e) => return Some(e),
        };
        let validate_ok = planned.ok() || (plan.may_adjust_clone && planned.rc == 87);
        if !validate_ok {
            if planned.remaining_active == 0 && planned.rc == 0 {
                return Some("没有第二活动目标，未 APPLY。".into());
            }
            return Some(format!("无法保持关闭：校验 {}。", planned.rc));
        }
        if let Some(error) = self.ensure_recovery() {
            return Some(error);
        }
        self.intent = IntentFile {
            keep_off: selected.iter().map(ScreenIdentityDto::from_identity).collect(),
            vdd_assist: plan.may_adjust_clone || snapshot.has_active_bundled_vdd(),
        };
        let dir = self.directory.as_ref().unwrap();
        let _ = JsonUtil::write_atomic(SessionPaths::intent(dir), &self.intent);
        None
    }

    fn write_shrunk_intent(&mut self, selected: Vec<ScreenIdentity>) -> Option<String> {
        if !self.has_session() {
            return None;
        }
        if let Some(error) = self.ensure_recovery() {
            return Some(error);
        }
        let vdd_assist = self.intent.vdd_assist;
        self.intent = IntentFile {
            keep_off: selected.iter().map(ScreenIdentityDto::from_identity).collect(),
            vdd_assist,
        };
        let dir = self.directory.as_ref().unwrap();
        let _ = JsonUtil::write_atomic(SessionPaths::intent(dir), &self.intent);
        None
    }

    fn ensure_recovery(&mut self) -> Option<String> {
        self.poll();
        if self.has_session() && self.is_ready {
            return if self.hotkey_registered {
                None
            } else {
                Some("紧急热键不可用。".into())
            };
        }
        let dir = SessionPaths::new_session_directory();
        let frame = match self.ccd.capture(CcdConstants::QUERY_FLAGS) {
            Ok(f) => f,
            Err(e) => return Some(e),
        };
        if let Err(e) = TopologyBlob::save(SessionPaths::topology(&dir), &frame.paths, &frame.modes) {
            return Some(e);
        }
        let parent = std::process::id().to_string();
        self.recovery_pid = (self.hooks.start_recovery)(&dir.to_string_lossy(), Some(&parent));
        let deadline = Instant::now() + Duration::from_secs(8);
        let mut ready: Option<ReadyFile> = None;
        while Instant::now() < deadline {
            ready = JsonUtil::try_read(SessionPaths::ready(&dir));
            if ready.is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        match &ready {
            Some(r) if r.pid == self.recovery_pid && r.hotkey_registered => {}
            Some(r) if !r.hotkey_registered => {
                self.directory = None;
                return Some("紧急热键不可用。".into());
            }
            _ => {
                self.directory = None;
                return Some("恢复进程未就绪。".into());
            }
        }
        let pid = ready.as_ref().unwrap().pid;
        let _ = JsonUtil::write_atomic(SessionPaths::arm(&dir), &ArmFile { pid });
        self.directory = Some(dir);
        self.is_ready = true;
        self.hotkey_registered = true;
        None
    }

    fn disable_bundled_vdd_after_session(&mut self, result: Option<&ResultFile>) {
        let no_physical = self
            .ccd
            .query_snapshot(CcdConstants::QUERY_FLAGS)
            .map(|s| s.active_physical().next().is_none())
            .unwrap_or(false);
        if result.map(|r| r.reason.as_str()) == Some(RECOVERY_EXIT_REASON) && no_physical {
            self.status_text = Some(match &self.status_text {
                Some(s) if !s.is_empty() => format!("{s} {RECOVERY_EXITED_LAST_PATH}"),
                _ => RECOVERY_EXITED_LAST_PATH.into(),
            });
            return;
        }
        self.append_disable_result();
    }

    fn append_disable_result(&mut self) {
        let rc = (self.hooks.run_driver_helper)("disable");
        if rc == 0 {
            return;
        }
        self.status_text = Some(match &self.status_text {
            Some(s) if !s.is_empty() => format!("{s} {DISABLE_VDD_FAILED}"),
            _ => DISABLE_VDD_FAILED.into(),
        });
    }
}

fn restore_failed(result: &ResultFile) -> bool {
    matches!(result.restore_rc, Some(rc) if rc > 0) || (result.restore_rc == Some(0) && !result.restored_targets)
}

fn start_recovery_process(directory: &str, parent_pid: Option<&str>) -> Result<i32, String> {
    let exe = ProcessLaunch::recovery_exe_path();
    let args = format!(
        "--directory \"{}\" --parent-pid {}",
        directory,
        parent_pid.unwrap_or("0")
    );
    ProcessLaunch::start_detached(&exe.to_string_lossy(), &args)
}

fn run_driver_helper_process(verb: &str) -> Result<i32, String> {
    let exe = ProcessLaunch::driver_helper_exe_path();
    let status = std::process::Command::new(&exe)
        .arg(verb)
        .status()
        .map_err(|e| e.to_string())?;
    Ok(status.code().unwrap_or(1))
}

pub struct DriverStatus;

impl DriverStatus {
    pub fn installed() -> bool {
        let program = std::env::var("ProgramFiles").unwrap_or_else(|_| r"C:\Program Files".into());
        std::path::Path::new(&program)
            .join("Veil")
            .join("vdd")
            .join("MttVDD.inf")
            .exists()
    }
}

impl RecoveryCoordinator {
    pub const ENABLE_VDD_CANCELLED: &'static str = ENABLE_VDD_CANCELLED;
    pub const DISABLE_VDD_FAILED: &'static str = DISABLE_VDD_FAILED;
    pub const RECOVERY_EXIT_REASON: &'static str = RECOVERY_EXIT_REASON;
    pub const RECOVERY_EXITED: &'static str = RECOVERY_EXITED;
    pub const RECOVERY_EXITED_LAST_PATH: &'static str = RECOVERY_EXITED_LAST_PATH;
}
