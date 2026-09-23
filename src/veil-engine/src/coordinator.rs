use crate::capability::Gate;
use crate::native::{CcdApi, CcdConstants, ParentWatcher, Win32ParentWatcher};
use crate::planner::DisplayPlanner;
use crate::process::ProcessLaunch;
use crate::session::{
    unix_seconds, ArmFile, HeartbeatFile, IntentFile, JsonUtil, ReadyFile, RecoveryState,
    ReleaseFile, RestoreError, RestoreOutcome, ResultFile, ScreenIdentityDto, SessionLog,
    SessionMetadata, SessionPaths, PROTOCOL_VERSION,
};
use crate::topology::TopologyBlob;
use crate::{KeepOffAction, ScreenIdentity};
use std::path::PathBuf;
use std::time::{Duration, Instant};

pub const ENABLE_VDD_CANCELLED: &str = "已取消启用辅助虚拟输出，物理屏未改动。";
pub const INSTALL_VDD_CANCELLED: &str = "已取消安装辅助虚拟输出，物理屏未改动。";
pub const DISABLE_VDD_FAILED: &str = "辅助虚拟输出未能禁用。";
pub const RECOVERY_EXIT_REASON: &str = "recovery-exit";
pub const RECOVERY_EXITED: &str = "恢复进程已退出。";
pub const RECOVERY_EXITED_LAST_PATH: &str =
    "恢复进程已退出，未禁用辅助虚拟输出（避免关掉最后活动路径）。";
pub const INSTALL_VDD_FAILED: &str = "辅助虚拟输出未能安装，物理屏未改动。";
pub const ENABLE_VDD_FAILED: &str = "辅助虚拟输出未能启用，物理屏未改动。";
pub const VDD_PATH_MISSING: &str = "辅助虚拟输出未能出现活动虚拟路径，物理屏未改动。";
pub const AUXILIARY_ALREADY_INSTALLED: &str = "辅助虚拟输出已安装。";
pub const AUXILIARY_PAYLOAD_MISSING: &str = "缺少已校验的辅助虚拟输出驱动包。";

pub struct RecoveryCoordinatorHooks {
    pub start_recovery: Box<dyn FnMut(&str, Option<&str>) -> i32>,
    pub run_driver_helper: Box<dyn FnMut(&str) -> i32>,
    pub confirm_enable_vdd: Option<Box<dyn FnMut(&str) -> bool>>,
    pub bundled_vdd_installed: Box<dyn Fn() -> bool>,
    pub bundled_vdd_payload: Box<dyn Fn() -> bool>,
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
            bundled_vdd_payload: Box::new(|| DriverStatus::payload_present()),
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
    vdd_owned: bool,
    pending_release: u64,
    cleanup_attempted: bool,
    baseline: Option<crate::native::CcdFrame>,
    last_outcome: Option<Result<RestoreOutcome, RestoreError>>,
    pub is_ready: bool,
    pub hotkey_registered: bool,
    pub heartbeat: Option<HeartbeatFile>,
    pub status_text: Option<String>,
    should_show_panel: bool,
    operation_id: u64,
    result_consumed: bool,
    panel_notified: bool,
    restored_outcome: Option<RestoreOutcome>,
    panel_context: Option<(PathBuf, u64, String)>,
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
            vdd_owned: false,
            pending_release: 0,
            cleanup_attempted: false,
            baseline: None,
            last_outcome: None,
            is_ready: false,
            hotkey_registered: false,
            heartbeat: None,
            status_text: None,
            should_show_panel: false,
            operation_id: 0,
            result_consumed: false,
            panel_notified: false,
            restored_outcome: None,
            panel_context: None,
        }
    }

    #[cfg(test)]
    pub(crate) fn forget_baseline_for_test(&mut self) {
        self.baseline = None;
    }

    #[cfg(test)]
    pub(crate) fn mark_vdd_owned_for_test(&mut self) {
        self.vdd_owned = true;
    }

    #[cfg(test)]
    pub(crate) fn vdd_owned_for_test(&self) -> bool {
        self.vdd_owned
    }

    pub fn has_session(&self) -> bool {
        self.directory.is_some()
    }

    pub fn session_directory(&self) -> Option<&std::path::Path> {
        self.directory.as_deref()
    }

    pub fn wanted(&self) -> Vec<ScreenIdentity> {
        self.intent
            .keep_off
            .iter()
            .map(|x| x.to_identity())
            .collect()
    }

    pub fn take_should_show_panel(&mut self) -> bool {
        let show = self.should_show_panel;
        self.should_show_panel = false;
        if show {
            self.log_panel_dispatch("panel-open-start");
        }
        show
    }

    pub fn panel_show_attempt_finished(&self) {
        self.log_panel_dispatch("panel-open-end");
    }

    fn log_panel_dispatch(&self, event: &str) {
        if let Some((dir, operation, reason)) = &self.panel_context {
            SessionLog::append(
                dir,
                event,
                Some(&format!(
                    "operation={operation}; 窗口请求，非机旁可见性证明"
                )),
                Some(reason),
                None,
                None,
            );
        }
    }

    pub fn recovery_block_reason(&self) -> Option<&str> {
        if self.has_session() && matches!(self.last_outcome, Some(Err(_))) {
            self.status_text.as_deref()
        } else {
            None
        }
    }

    fn begin_operation(&mut self, id: u64) {
        self.operation_id = id;
        self.result_consumed = false;
        self.panel_notified = false;
        self.should_show_panel = false;
        self.cleanup_attempted = false;
        self.restored_outcome = None;
        self.last_outcome = None;
        self.panel_context = None;
    }

    fn notify_panel(&mut self, reason: &str) {
        if self.panel_notified {
            return;
        }
        self.panel_notified = true;
        self.should_show_panel = true;
        self.panel_context = self
            .directory
            .clone()
            .map(|dir| (dir, self.operation_id, reason.into()));
        self.log_operation("panel-request", reason, None);
    }

    fn log_operation(&self, event: &str, reason: &str, rc: Option<i32>) {
        if let Some(dir) = &self.directory {
            SessionLog::append(
                dir,
                event,
                Some(&format!("operation={}", self.operation_id)),
                Some(reason),
                None,
                rc,
            );
        }
    }

    pub fn is_interrupt_reason(reason: &str) -> bool {
        matches!(
            reason,
            "suspend-resume" | "execution-gap" | "unexpected-topology"
        )
    }

    pub fn poll(&mut self) {
        let Some(dir) = self.directory.clone() else {
            return;
        };
        // A retained directory is recovery context, not an unconsumed event.
        // In particular, do not let an old heartbeat overwrite a cleanup error.
        if self.result_consumed {
            return;
        }
        if !SessionPaths::result(&dir).exists()
            && self.recovery_pid > 0
            && !(self.hooks.is_alive)(self.recovery_pid)
        {
            let recorded = JsonUtil::write_atomic(
                SessionPaths::result(&dir),
                &ResultFile {
                    protocol_version: PROTOCOL_VERSION,
                    reason: RECOVERY_EXIT_REASON.into(),
                    ok: false,
                    error: Some(RECOVERY_EXITED.into()),
                    ..Default::default()
                },
            );
            self.intent.keep_off.clear();
            self.is_ready = false;
            self.hotkey_registered = false;
            self.heartbeat = None;
            if let Err(e) = recorded {
                self.last_outcome = Some(Err(RestoreError::Protocol(e.clone())));
                self.status_text = Some(e);
                self.recovery_pid = 0;
                return;
            }
            SessionLog::append(
                &dir,
                "finish",
                Some(RECOVERY_EXITED),
                Some(RECOVERY_EXIT_REASON),
                None,
                None,
            );
        }
        self.serve_vdd_request();
        self.heartbeat = JsonUtil::try_read(SessionPaths::heartbeat(&dir));
        let mut restore_failed = false;
        if let Some(hb) = &self.heartbeat {
            self.hotkey_registered = hb.hotkey_registered;
            if hb.state == RecoveryState::RestoreFailed
                && hb.processed_request_id >= self.pending_release
            {
                self.intent.keep_off.clear();
                self.last_outcome = Some(Err(RestoreError::Failed(
                    hb.detail.clone().unwrap_or_default(),
                )));
                restore_failed = true;
            }
            self.is_ready = hb.armed || SessionPaths::ready(&dir).exists();
            if let Some(detail) = &hb.detail {
                if !detail.is_empty() {
                    self.status_text = Some(detail.clone());
                }
            }
        }
        if restore_failed {
            self.notify_panel("restore-failed");
        }
        if SessionPaths::result(&dir).exists() {
            self.result_consumed = true;
            let result = JsonUtil::read::<ResultFile>(SessionPaths::result(&dir));
            let outcome = result
                .as_ref()
                .map_err(|e| RestoreError::Protocol(e.clone()))
                .and_then(|r| r.restoration_outcome());
            if let Ok(r) = &result {
                if Self::is_interrupt_reason(&r.reason) {
                    self.notify_panel(&r.reason);
                    self.status_text = Some(Self::format_result(Some(r)));
                } else {
                    self.status_text = Some(match &outcome {
                        Ok(o) => o.message.clone(),
                        Err(e) => e.to_string(),
                    });
                    if let Some(error) = &r.error {
                        if !error.is_empty() {
                            self.status_text = Some(format!(
                                "{error} {}",
                                self.status_text.as_deref().unwrap_or_default()
                            ));
                        }
                    }
                }
            } else {
                self.status_text = Some(match &outcome {
                    Ok(o) => o.message.clone(),
                    Err(e) => e.to_string(),
                });
            }
            if let Some(text) = &mut self.status_text {
                text.push_str(&format!(
                    " 记录：{}",
                    dir.file_name().unwrap_or_default().to_string_lossy()
                ));
            }
            self.last_outcome = Some(outcome.clone());
            self.is_ready = false;
            self.hotkey_registered = false;
            self.recovery_pid = 0;
            self.intent = IntentFile::default();
            self.heartbeat = None;
            if outcome.is_ok() {
                self.restored_outcome = outcome.ok();
                if self.vdd_owned && !self.cleanup_attempted {
                    self.cleanup_attempted = true;
                    self.disable_bundled_vdd_after_session(result.as_ref().ok());
                }
                if self.vdd_owned {
                    self.last_outcome = Some(Err(RestoreError::Failed(
                        self.status_text
                            .clone()
                            .unwrap_or_else(|| DISABLE_VDD_FAILED.into()),
                    )));
                }
                // A failed cleanup remains retryable even though physical output is restored.
                self.complete_cleanup();
            }
        }
    }

    pub fn format_result(result: Option<&ResultFile>) -> String {
        let Some(result) = result else {
            return "恢复已结束，状态未知。".into();
        };
        if let Err(error) = result.restoration_outcome() {
            return error.to_string();
        }
        let mut text = match result.reason.as_str() {
            "release" => "已恢复全部。".into(),
            "hotkey" => "已由 Ctrl+Alt+Shift+F10 恢复。".into(),
            "parent-exit" => "界面退出后已恢复。".into(),
            "execution-gap" => "会话中断，保持关闭已结束。".into(),
            "suspend-resume" => "系统休眠或待机后已恢复显示，保持关闭已结束。".into(),
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
            if !err.is_empty()
                && result.reason != "execution-gap"
                && result.reason != "unexpected-topology"
                && result.reason != "suspend-resume"
            {
                return err.clone();
            }
        }
        if result.reapply_attempted {
            text.push_str(" 已尝试再关一次。");
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
        SessionLog::append(
            dir,
            "vdd-enable",
            Some("界面按再关请求启用辅助虚拟输出。"),
            None,
            None,
            None,
        );
        self.vdd_owned = true;
        let saved =
            JsonUtil::read::<SessionMetadata>(SessionPaths::metadata(dir)).and_then(|mut meta| {
                meta.vdd_owned = true;
                JsonUtil::write_atomic(SessionPaths::metadata(dir), &meta)
            });
        if let Err(e) = saved {
            self.status_text = Some(e);
            if let Some(error) = self.restore_all() {
                self.status_text = Some(error);
            }
            return;
        }
        let rc = (self.hooks.run_driver_helper)("enable");
        if rc != 0 {
            self.status_text = Some("再次启用辅助输出失败，结束本轮要求。".into());
            if let Some(e) = self.restore_all() {
                self.status_text = Some(e);
            }
        }
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

    pub fn install_auxiliary_output(&mut self) -> Option<String> {
        if (self.hooks.bundled_vdd_installed)() {
            return Some(AUXILIARY_ALREADY_INSTALLED.into());
        }
        if !(self.hooks.bundled_vdd_payload)() {
            return Some(AUXILIARY_PAYLOAD_MISSING.into());
        }
        if let Some(confirm) = &mut self.hooks.confirm_enable_vdd {
            if !confirm(Gate::INSTALL_VDD_REASON) {
                return Some(INSTALL_VDD_CANCELLED.into());
            }
        }
        let helper_rc = (self.hooks.run_driver_helper)("install-driver");
        if helper_rc != 0 {
            return Some(INSTALL_VDD_FAILED.into());
        }
        None
    }

    pub fn restore_one(&mut self, identity: &ScreenIdentity) -> Option<String> {
        let selected: Vec<_> = self
            .wanted()
            .into_iter()
            .filter(|id| !id.matches(identity))
            .collect();
        self.apply_intent(selected)
    }

    pub fn restore_all(&mut self) -> Option<String> {
        let Some(dir) = self.directory.clone() else {
            return None;
        };
        let request_id = crate::session::request_id();
        // Retry only the failed device cleanup when all still-connected baseline
        // physical targets are active. Never replay a saved topology just for UAC.
        if self.result_consumed
            && self.restored_outcome.is_some()
            && self.vdd_owned
            && self.physical_restoration_confirmed()
        {
            self.operation_id = request_id;
            self.cleanup_attempted = true;
            self.disable_bundled_vdd_after_session(None);
            if self.vdd_owned {
                return self.status_text.clone();
            }
            self.last_outcome = self.restored_outcome.clone().map(Ok);
            self.status_text = Some("显示已恢复，辅助虚拟输出已清理。".into());
            self.complete_cleanup();
            return None;
        }
        let recovery_alive = self.recovery_pid > 0 && (self.hooks.is_alive)(self.recovery_pid);
        let metadata_missing = !SessionPaths::metadata(&dir).exists();
        if metadata_missing && !recovery_alive {
            // The original worker can restore from its in-memory baseline. A
            // replacement worker needs metadata rebuilt from the pre-close
            // baseline; never capture the currently closed layout as baseline.
            let trusted_baseline = self.baseline.as_ref().is_some_and(|frame| {
                !frame.paths.is_empty() && frame.snapshot.active_physical().next().is_some()
            });
            if !trusted_baseline {
                return self.missing_metadata_error("缺少可信的关屏前拓扑", recovery_alive);
            }
            if let Err(error) = self.prepare_directory() {
                return self.missing_metadata_error(&format!("重建失败：{error}"), recovery_alive);
            }
        }
        if let Err(e) = JsonUtil::write_atomic(
            SessionPaths::release(&dir),
            &ReleaseFile {
                at: unix_seconds(),
                request_id,
            },
        ) {
            return if metadata_missing {
                self.missing_metadata_error(&format!("恢复请求写入失败：{e}"), recovery_alive)
            } else {
                Some(e)
            };
        }
        self.pending_release = request_id;
        if !recovery_alive {
            if SessionPaths::result(&dir).exists() {
                if let Err(e) = std::fs::rename(
                    SessionPaths::result(&dir),
                    dir.join(format!("result-{}.json", crate::session::request_id())),
                ) {
                    return if metadata_missing {
                        self.missing_metadata_error(
                            &format!("旧恢复结果归档失败：{e}"),
                            recovery_alive,
                        )
                    } else {
                        Some(e.to_string())
                    };
                }
            }
            self.recovery_pid = (self.hooks.start_recovery)(&dir.to_string_lossy(), None);
            if self.recovery_pid <= 0 {
                let exe = ProcessLaunch::recovery_exe_path();
                let error = format!("无法启动恢复专用进程（{}）。", exe.display());
                return if metadata_missing {
                    self.missing_metadata_error(&error, recovery_alive)
                } else {
                    Some(error)
                };
            }
        }
        self.begin_operation(request_id);
        self.intent.keep_off.clear();
        None
    }

    fn missing_metadata_error(&mut self, detail: &str, recovery_alive: bool) -> Option<String> {
        let error = format!("会话元数据缺失，无法确认恢复：{detail}。请重试恢复全部。");
        self.last_outcome = Some(Err(RestoreError::Protocol(error.clone())));
        self.status_text = Some(error.clone());
        self.intent.keep_off.clear();
        if !recovery_alive {
            self.recovery_pid = 0;
            self.is_ready = false;
            self.hotkey_registered = false;
        }
        Some(error)
    }

    pub fn restore_all_and_wait(
        &mut self,
        timeout: Duration,
    ) -> Result<RestoreOutcome, RestoreError> {
        if self.directory.is_none() {
            return self
                .last_outcome
                .clone()
                .unwrap_or_else(|| Ok(RestoreOutcome::complete("无需恢复。")));
        }
        if let Some(e) = self.restore_all() {
            return Err(RestoreError::Protocol(e));
        }
        if self.directory.is_none() {
            return self
                .last_outcome
                .clone()
                .unwrap_or_else(|| Err(RestoreError::Protocol("恢复缺少明确结果。".into())));
        }
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            self.poll();
            if let Some(outcome) = &self.last_outcome {
                if outcome.is_err() || self.directory.is_none() {
                    return outcome.clone();
                }
            }
            std::thread::sleep(Duration::from_millis(150));
        }
        let error = RestoreError::Timeout("恢复超时或清理失败，未退出。请重试恢复全部。".into());
        self.status_text = Some(error.to_string());
        self.last_outcome = Some(Err(error.clone()));
        Err(error)
    }

    fn apply_intent(&mut self, selected: Vec<ScreenIdentity>) -> Option<String> {
        if selected.is_empty() {
            return self.restore_all();
        }
        if matches!(self.last_outcome, Some(Err(_))) {
            return Some("上次恢复未完成，请先恢复全部。".into());
        }
        let guard = match crate::maintenance::OperationLock::for_keep_off() {
            Ok(g) => g,
            Err(e) => return Some(e),
        };
        drop(guard);
        if self.baseline.is_none() {
            self.baseline = match self.ccd.capture(CcdConstants::QUERY_FLAGS) {
                Ok(f) => Some(f),
                Err(e) => return Some(e),
            };
        }
        let error = self.apply_intent_inner(selected);
        if let Some(e) = &error {
            if self.directory.is_some() || self.vdd_owned {
                if self.directory.is_none() {
                    if let Err(setup) = self.prepare_directory() {
                        self.last_outcome = Some(Err(RestoreError::Protocol(setup.clone())));
                        return Some(format!("{e}；恢复准备失败：{setup}"));
                    }
                }
                if let Some(restore) = self.restore_all() {
                    self.last_outcome = Some(Err(RestoreError::Protocol(restore.clone())));
                    return Some(format!("{e}；恢复请求失败：{restore}"));
                }
            } else {
                self.baseline = None;
            }
        }
        error
    }

    fn apply_intent_inner(&mut self, selected: Vec<ScreenIdentity>) -> Option<String> {
        if selected.is_empty() {
            return self.restore_all();
        }
        let previous = self.wanted();
        let shrink = previous
            .iter()
            .any(|old| !selected.iter().any(|id| id.matches(old)));
        if shrink {
            return self.write_shrunk_intent(selected);
        }
        let mut snapshot = match self.ccd.query_snapshot(CcdConstants::QUERY_FLAGS) {
            Ok(s) => s,
            Err(e) => return Some(e),
        };
        let mut plan = Gate::plan_keep_off(
            &snapshot,
            &selected,
            crate::BundledVddAvailability::from_flags(
                (self.hooks.bundled_vdd_installed)(),
                (self.hooks.bundled_vdd_payload)(),
            ),
        );
        if plan.action == KeepOffAction::Blocked {
            return plan.block_reason;
        }
        if matches!(
            plan.action,
            KeepOffAction::EnableBundledVdd | KeepOffAction::InstallBundledVdd
        ) {
            let reason = plan
                .block_reason
                .clone()
                .unwrap_or_else(|| Gate::ENABLE_VDD_REASON.into());
            if let Some(confirm) = &mut self.hooks.confirm_enable_vdd {
                if !confirm(&reason) {
                    return Some(if plan.action == KeepOffAction::InstallBundledVdd {
                        INSTALL_VDD_CANCELLED.into()
                    } else {
                        ENABLE_VDD_CANCELLED.into()
                    });
                }
            }
            if plan.action == KeepOffAction::InstallBundledVdd {
                let helper_rc = (self.hooks.run_driver_helper)("install-driver");
                if helper_rc != 0 {
                    return Some(INSTALL_VDD_FAILED.into());
                }
            }
            // Mark cleanup responsibility before launching: failure may be partial.
            self.vdd_owned = true;
            if previous.is_empty() {
                if let Err(e) = self.prepare_directory() {
                    return Some(e);
                }
            } else {
                if let Some(error) = self.publish_intent(&selected, true) {
                    return Some(error);
                }
                if let Some(dir) = self.directory.clone() {
                    if let Err(e) = JsonUtil::read::<SessionMetadata>(SessionPaths::metadata(&dir))
                        .and_then(|mut meta| {
                            meta.vdd_owned = true;
                            JsonUtil::write_atomic(SessionPaths::metadata(&dir), &meta)
                        })
                    {
                        return Some(e);
                    }
                }
            }
            let helper_rc = (self.hooks.run_driver_helper)("enable");
            if helper_rc != 0 {
                return Some(ENABLE_VDD_FAILED.into());
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
                return Some(VDD_PATH_MISSING.into());
            }
            plan = Gate::plan_keep_off(&snapshot, &selected, true);
            if plan.action != KeepOffAction::Deactivate {
                return Some(
                    plan.block_reason
                        .unwrap_or_else(|| "启用辅助虚拟输出后仍无法保持关闭。".into()),
                );
            }
        }
        let planned = match DisplayPlanner::validate_deactivate(
            self.ccd.as_ref(),
            &selected,
            plan.adjust_origin,
        ) {
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
        self.publish_intent(
            &selected,
            plan.may_adjust_clone || snapshot.has_active_bundled_vdd(),
        )
    }

    fn publish_intent(&mut self, selected: &[ScreenIdentity], vdd_assist: bool) -> Option<String> {
        if let Some(error) = self.ensure_recovery() {
            return Some(error);
        }
        let intent = IntentFile {
            request_id: crate::session::request_id(),
            keep_off: selected
                .iter()
                .map(ScreenIdentityDto::from_identity)
                .collect(),
            vdd_assist,
        };
        let dir = self.directory.as_ref().unwrap();
        if let Err(e) = JsonUtil::write_atomic(SessionPaths::intent(dir), &intent) {
            return Some(e);
        }
        self.intent = intent;
        self.begin_operation(self.intent.request_id);
        self.pending_release = 0;
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
        let intent = IntentFile {
            request_id: crate::session::request_id(),
            keep_off: selected
                .iter()
                .map(ScreenIdentityDto::from_identity)
                .collect(),
            vdd_assist,
        };
        let dir = self.directory.as_ref().unwrap();
        if let Err(e) = JsonUtil::write_atomic(SessionPaths::intent(dir), &intent) {
            return Some(e);
        }
        self.intent = intent;
        self.begin_operation(self.intent.request_id);
        self.pending_release = 0;
        None
    }

    fn ensure_recovery(&mut self) -> Option<String> {
        self.poll();
        if matches!(self.last_outcome, Some(Err(_))) {
            return Some("恢复未完成，拒绝新的关屏。".into());
        }
        if self.has_session() && self.is_ready {
            return if self.hotkey_registered {
                None
            } else {
                Some("紧急热键不可用。".into())
            };
        }
        if let Err(e) = self.prepare_directory() {
            return Some(e);
        }
        let dir = self.directory.clone().unwrap();
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
                return Some("紧急热键不可用。".into());
            }
            _ => {
                return Some("恢复进程未就绪。".into());
            }
        }
        let pid = ready.as_ref().unwrap().pid;
        if let Err(e) = JsonUtil::write_atomic(SessionPaths::arm(&dir), &ArmFile { pid }) {
            return Some(e);
        }
        self.directory = Some(dir);
        self.is_ready = true;
        self.hotkey_registered = true;
        None
    }

    fn prepare_directory(&mut self) -> Result<(), String> {
        if self.baseline.is_none() {
            self.baseline = Some(self.ccd.capture(CcdConstants::QUERY_FLAGS)?);
        }
        let creating = self.directory.is_none();
        let dir = self
            .directory
            .clone()
            .unwrap_or_else(SessionPaths::new_session_directory);
        let baseline = self.baseline.clone().ok_or("缺少恢复基线。")?;
        let handshake = self
            .ccd
            .capture(CcdConstants::QUERY_FLAGS)
            .unwrap_or_else(|_| baseline.clone());
        let write_session = || -> Result<(), String> {
            TopologyBlob::save(
                SessionPaths::baseline(&dir),
                &baseline.paths,
                &baseline.modes,
            )?;
            TopologyBlob::save(
                SessionPaths::topology(&dir),
                &handshake.paths,
                &handshake.modes,
            )?;
            JsonUtil::write_atomic(
                SessionPaths::metadata(&dir),
                &SessionMetadata {
                    protocol_version: PROTOCOL_VERSION,
                    physical_targets: baseline
                        .snapshot
                        .active_physical()
                        .map(|p| ScreenIdentityDto::from_identity(&p.identity()))
                        .collect(),
                    vdd_owned: self.vdd_owned,
                },
            )?;
            Ok(())
        };
        if let Err(error) = write_session() {
            if creating {
                let _ = std::fs::remove_dir_all(&dir);
            }
            return Err(error);
        }
        self.directory = Some(dir);
        Ok(())
    }

    fn disable_bundled_vdd_after_session(&mut self, _result: Option<&ResultFile>) {
        self.log_operation("cleanup-start", "disable-owned-vdd", None);
        let safe = self
            .ccd
            .query_snapshot(CcdConstants::QUERY_FLAGS)
            .map(|s| s.active_physical().next().is_some())
            .unwrap_or(false);
        if !safe {
            self.status_text =
                Some("未确认活动物理输出，保留辅助虚拟输出；请重试恢复全部。".into());
            self.last_outcome = Some(Err(RestoreError::Failed(self.status_text.clone().unwrap())));
            self.log_operation("cleanup-end", "physical-output-unconfirmed", None);
            return;
        }
        let rc = (self.hooks.run_driver_helper)("disable");
        self.log_operation("cleanup-end", "disable-owned-vdd", Some(rc));
        if rc != 0 {
            let detail = match rc {
                1223 => "辅助虚拟输出清理已取消，请点恢复全部重试。".to_string(),
                1460 => "辅助虚拟输出清理超时，状态未知；请确认后重试恢复全部。".to_string(),
                _ => format!("{DISABLE_VDD_FAILED} 返回码 {rc}；请点恢复全部重试。"),
            };
            self.status_text = Some(detail.clone());
            self.last_outcome = Some(Err(RestoreError::Failed(detail)));
        } else {
            self.vdd_owned = false;
        }
    }

    fn complete_cleanup(&mut self) {
        if !self.vdd_owned {
            self.directory = None;
            self.baseline = None;
            self.vdd_request_served = None;
        }
    }

    fn physical_restoration_confirmed(&self) -> bool {
        let Some(baseline) = &self.baseline else {
            return false;
        };
        let (Ok(active), Ok(connected)) = (
            self.ccd.query_snapshot(CcdConstants::QUERY_FLAGS),
            self.ccd.connected_physical(),
        ) else {
            return false;
        };
        let physical: Vec<_> = active.active_physical().map(|p| p.identity()).collect();
        !physical.is_empty()
            && baseline.snapshot.active_physical().all(|p| {
                let id = p.identity();
                !connected.iter().any(|c| c.matches(&id)) || physical.iter().any(|a| a.matches(&id))
            })
    }
}

fn start_recovery_process(directory: &str, parent_pid: Option<&str>) -> Result<i32, String> {
    let exe = ProcessLaunch::recovery_exe_path();
    let args = format!(
        "--directory \"{}\" --parent-pid {}",
        directory,
        parent_pid.unwrap_or("0")
    );
    let args = if parent_pid.is_none() {
        format!("{args} --restore-only")
    } else {
        args
    };
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
    pub fn payload_present() -> bool {
        crate::payload::payload_present()
    }

    pub fn installed() -> bool {
        !crate::devices::known_bundled_instance_ids().is_empty()
    }
}

impl RecoveryCoordinator {
    pub const ENABLE_VDD_CANCELLED: &'static str = ENABLE_VDD_CANCELLED;
    pub const INSTALL_VDD_CANCELLED: &'static str = INSTALL_VDD_CANCELLED;
    pub const INSTALL_VDD_FAILED: &'static str = INSTALL_VDD_FAILED;
    pub const AUXILIARY_ALREADY_INSTALLED: &'static str = AUXILIARY_ALREADY_INSTALLED;
    pub const AUXILIARY_PAYLOAD_MISSING: &'static str = AUXILIARY_PAYLOAD_MISSING;
    pub const DISABLE_VDD_FAILED: &'static str = DISABLE_VDD_FAILED;
    pub const RECOVERY_EXIT_REASON: &'static str = RECOVERY_EXIT_REASON;
    pub const RECOVERY_EXITED: &'static str = RECOVERY_EXITED;
    pub const RECOVERY_EXITED_LAST_PATH: &'static str = RECOVERY_EXITED_LAST_PATH;
}
