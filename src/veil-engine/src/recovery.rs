use crate::capability::{resolved_screen_name, DisplaySnapshot, Gate, KeepOffAction};
use crate::native::{
    CcdAbi, CcdApi, CcdConstants, CcdFrame, DisplayConfigModeInfo, DisplayConfigPathInfo, Hotkey,
    MonotonicClock, ParentWatcher, PowerEvent, PowerObserver,
};
use crate::session::{
    ArmFile, HeartbeatFile, HeartbeatScreen, IntentFile, JsonUtil, ReadyFile, RecoveryState,
    ReleaseFile, RestoreState, ResultFile, SessionLog, SessionMetadata, SessionPaths,
    VddRequestFile, PROTOCOL_VERSION,
};
use crate::topology::{PathOps, TopologyBlob};
use crate::ScreenIdentity;
use std::path::PathBuf;
use std::time::Duration;

pub struct RecoveryOptions {
    pub restore_only: bool,
    pub directory: PathBuf,
    pub self_pid: i32,
    pub parent_pid: i32,
    pub ccd: Box<dyn CcdApi>,
    pub hotkey: Box<dyn Hotkey>,
    pub clock: Box<dyn MonotonicClock>,
    pub parent: Box<dyn ParentWatcher>,
    pub power: Box<dyn PowerObserver>,
    pub arm_timeout_seconds: f64,
    pub gap_seconds: f64,
    pub reapply_settle_attempts: i32,
    pub reapply_settle_pause: Duration,
    pub pause: Box<dyn Fn(Duration)>,
    pub vdd_wait_seconds: f64,
}

impl RecoveryOptions {
    pub fn defaults(directory: PathBuf, ccd: Box<dyn CcdApi>, hotkey: Box<dyn Hotkey>) -> Self {
        Self {
            restore_only: false,
            directory,
            self_pid: std::process::id() as i32,
            parent_pid: 0,
            ccd,
            hotkey,
            clock: Box::new(crate::native::TickClock),
            parent: Box::new(crate::native::Win32ParentWatcher),
            power: Box::new(crate::native::Win32PowerObserver::new()),
            arm_timeout_seconds: 10.0,
            gap_seconds: 3.0,
            reapply_settle_attempts: 12,
            reapply_settle_pause: Duration::from_millis(150),
            pause: Box::new(std::thread::sleep),
            vdd_wait_seconds: 20.0,
        }
    }
}

pub struct RecoverySession {
    state: RecoveryState,
    processed_request: u64,
    processed_release: u64,
    last_release_text: Option<String>,
    physical_targets: Vec<ScreenIdentity>,
    baseline_changed: bool,
    opt: RecoveryOptions,
    saved_paths: Vec<DisplayConfigPathInfo>,
    saved_modes: Vec<DisplayConfigModeInfo>,
    saved_identities: Vec<ScreenIdentity>,
    saved_fingerprint: String,
    armed: bool,
    holding: bool,
    reapply_attempted: bool,
    waiting_vdd: bool,
    vdd_wait_start: f64,
    vdd_activate_attempted: bool,
    vdd_activate_logged: bool,
    vdd_instance_seen: bool,
    last_virtual_adapters: Option<String>,
    hotkey_registered: bool,
    started: f64,
    previous: f64,
    suspended: bool,
    last_intent_text: Option<String>,
    expected_targets: Option<Vec<(String, u32)>>,
    intent: IntentFile,
    pub result: ResultFile,
    pub exited: bool,
}

impl RecoverySession {
    pub fn new(opt: RecoveryOptions) -> Self {
        Self {
            state: RecoveryState::Waiting,
            processed_request: 0,
            processed_release: 0,
            last_release_text: None,
            physical_targets: vec![],
            baseline_changed: false,
            opt,
            saved_paths: vec![],
            saved_modes: vec![],
            saved_identities: vec![],
            saved_fingerprint: String::new(),
            armed: false,
            holding: false,
            reapply_attempted: false,
            waiting_vdd: false,
            vdd_wait_start: 0.0,
            vdd_activate_attempted: false,
            vdd_activate_logged: false,
            vdd_instance_seen: false,
            last_virtual_adapters: None,
            hotkey_registered: false,
            started: 0.0,
            previous: 0.0,
            suspended: false,
            last_intent_text: None,
            expected_targets: None,
            intent: IntentFile::default(),
            result: ResultFile {
                protocol_version: PROTOCOL_VERSION,
                reason: "not-armed".into(),
                ok: false,
                ..Default::default()
            },
            exited: false,
        }
    }

    pub fn start(&mut self) {
        if let Err(ex) = self.start_inner() {
            if self.saved_paths.is_empty() {
                self.result.error = Some(ex);
                self.result.reason = "error".into();
                self.result.restore_state = RestoreState::Unknown;
                self.write_result();
            } else {
                self.fail("error", Some(&ex), true);
            }
        }
    }

    fn start_inner(&mut self) -> Result<(), String> {
        CcdAbi::ensure_expected_layout()?;
        let meta: SessionMetadata = JsonUtil::read(SessionPaths::metadata(&self.opt.directory))?;
        if meta.protocol_version != PROTOCOL_VERSION {
            return Err("未知会话协议，拒绝关屏。".into());
        }
        self.baseline_changed = meta.vdd_owned;
        self.physical_targets = meta
            .physical_targets
            .iter()
            .map(|id| id.to_identity())
            .collect();
        let (paths, modes) = TopologyBlob::load(SessionPaths::baseline(&self.opt.directory))?;
        self.saved_fingerprint = TopologyBlob::fingerprint(&paths, &modes);
        self.saved_identities = paths
            .iter()
            .map(|p| {
                self.physical_targets
                    .iter()
                    .find(|id| {
                        id.adapter_luid == p.target_info.adapter_id.to_hex()
                            && id.target_id == p.target_info.id
                    })
                    .cloned()
                    .unwrap_or_else(|| {
                        ScreenIdentity::new(p.target_info.adapter_id.to_hex(), p.target_info.id, "")
                    })
            })
            .collect();
        self.saved_paths = paths;
        self.saved_modes = modes;
        if self.opt.restore_only {
            if let Some(r) =
                JsonUtil::try_read::<ReleaseFile>(SessionPaths::release(&self.opt.directory))
            {
                self.processed_release = r.request_id;
            }
            self.armed = true;
            self.hotkey_registered = self.opt.hotkey.try_register();
            self.finish("release", true);
            return Ok(());
        }
        let (handshake_paths, handshake_modes) =
            TopologyBlob::load(SessionPaths::topology(&self.opt.directory))?;
        let current = self.opt.ccd.capture(CcdConstants::QUERY_FLAGS)?;
        if TopologyBlob::fingerprint(&current.paths, &current.modes)
            != TopologyBlob::fingerprint(&handshake_paths, &handshake_modes)
        {
            return Err("topology changed since save".into());
        }
        if !self.opt.hotkey.try_register() {
            self.fail("error", Some("RegisterHotKey failed"), false);
            return Ok(());
        }
        self.hotkey_registered = true;
        JsonUtil::write_atomic(
            SessionPaths::ready(&self.opt.directory),
            &ReadyFile {
                pid: self.opt.self_pid,
                hotkey_registered: true,
                hotkey: CcdConstants::HOTKEY_TEXT.into(),
            },
        )?;
        SessionLog::append(
            &self.opt.directory,
            "ready",
            Some("热键已注册。"),
            None,
            None,
            None,
        );
        self.write_heartbeat("等待 arm。", None, false);
        self.started = self.opt.clock.seconds();
        self.previous = self.started;
        Ok(())
    }

    pub fn tick(&mut self) {
        if self.exited {
            return;
        }
        if let Err(ex) = self.tick_core() {
            if self.state == RecoveryState::RestoreFailed {
                self.result.error = Some(ex.clone());
                self.write_heartbeat(&ex, None, true);
            } else {
                self.fail("error", Some(&ex), self.holding || self.armed);
            }
        }
    }

    fn tick_core(&mut self) -> Result<(), String> {
        let now = self.opt.clock.seconds();
        let hotkey = self.opt.hotkey.was_pressed();
        if !self.armed {
            if hotkey {
                self.fail("cancelled-before-arm", None, false);
                return Ok(());
            }
            if SessionPaths::release(&self.opt.directory).exists() {
                self.fail("release", Some("已取消，未改物理屏。"), false);
                return Ok(());
            }
            if let Some(arm) = JsonUtil::try_read::<ArmFile>(SessionPaths::arm(&self.opt.directory))
            {
                if arm.pid != self.opt.self_pid {
                    self.fail("error", Some("arm PID mismatch"), false);
                    return Ok(());
                }
                self.armed = true;
                self.previous = now;
                SessionLog::append(&self.opt.directory, "armed", None, None, None, None);
                self.write_heartbeat("已 arm。", None, false);
                return Ok(());
            }
            if now - self.started >= self.opt.arm_timeout_seconds {
                self.fail(
                    "not-armed",
                    Some("recovery worker not ready within timeout"),
                    false,
                );
            }
            return Ok(());
        }
        if hotkey {
            self.finish("hotkey", true);
            return Ok(());
        }
        if SessionPaths::release(&self.opt.directory).exists() {
            let text = std::fs::read_to_string(SessionPaths::release(&self.opt.directory))
                .map_err(|e| e.to_string());
            match text {
                Ok(text) if self.last_release_text.as_deref() != Some(&text) => {
                    self.last_release_text = Some(text.clone());
                    let release: ReleaseFile =
                        serde_json::from_str(&text).map_err(|e| e.to_string())?;
                    if release.request_id == 0 {
                        return Err("恢复请求缺少编号。".into());
                    }
                    if release.request_id > self.processed_release {
                        self.processed_release = release.request_id;
                        self.finish("release", true);
                        return Ok(());
                    }
                }
                Err(e) if self.state != RecoveryState::RestoreFailed => return Err(e),
                _ => {}
            }
        }
        if self.opt.parent_pid > 0 && !self.opt.parent.is_alive(self.opt.parent_pid)? {
            self.finish("parent-exit", true);
            return Ok(());
        }
        if self.state == RecoveryState::RestoreFailed {
            return Ok(());
        }
        match self.opt.power.poll() {
            PowerEvent::Suspending => {
                self.suspended = true;
                self.previous = now;
                SessionLog::append(
                    &self.opt.directory,
                    "power-suspend",
                    Some("系统正在进入睡眠或待机，暂停保持关闭监视。"),
                    None,
                    None,
                    None,
                );
                self.write_heartbeat("系统正在休眠或待机。", None, false);
                return Ok(());
            }
            PowerEvent::Resumed => {
                self.suspended = false;
                self.handle_power_resume(now);
                return Ok(());
            }
            PowerEvent::None => {}
        }
        if self.suspended {
            self.previous = now;
            return Ok(());
        }
        if self.waiting_vdd {
            self.tick_waiting_vdd(now);
            return Ok(());
        }
        if now - self.previous > self.opt.gap_seconds {
            self.handle_interrupt("execution-gap");
            return Ok(());
        }
        self.previous = now;
        if crate::maintenance::is_active()? {
            self.finish("maintenance", true);
            return Ok(());
        }
        self.apply_intent_if_needed();
        if self.exited || self.waiting_vdd {
            return Ok(());
        }
        if self.holding {
            if self.newer_intent_pending() {
                return Ok(());
            }
            if let Some(expected) = self.expected_targets.clone() {
                let frame = self.opt.ccd.capture(CcdConstants::QUERY_FLAGS)?;
                let current = PathOps::active_targets(&frame.paths);
                if current != expected {
                    let selected: Vec<_> = self
                        .intent
                        .keep_off
                        .iter()
                        .map(|x| x.to_identity())
                        .collect();
                    if keep_off_still_holds(&frame.snapshot, &selected) {
                        self.expected_targets = Some(current);
                        SessionLog::append(
                            &self.opt.directory,
                            "topology-settle",
                            Some("活动目标变了，所选物理屏仍关。"),
                            None,
                            None,
                            None,
                        );
                        self.write_heartbeat("拓扑微调，仍保持关闭。", Some(&selected), false);
                        return Ok(());
                    }
                    self.handle_interrupt("unexpected-topology");
                }
            }
        }
        Ok(())
    }

    pub fn run_until_exit(&mut self, slice: Option<Duration>) {
        let pause = slice.unwrap_or(Duration::from_millis(50));
        self.start();
        while !self.exited {
            self.tick();
            if !self.exited {
                std::thread::sleep(pause);
            }
        }
    }

    fn apply_intent_if_needed(&mut self) {
        let path = SessionPaths::intent(&self.opt.directory);
        if !path.exists() {
            return;
        }
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(_) => return,
        };
        if self.last_intent_text.as_deref() == Some(&text) {
            return;
        }
        let intent = match serde_json::from_str::<IntentFile>(&text) {
            Ok(v) => v,
            Err(ex) => {
                self.fail("error", Some(&format!("intent 无效：{ex}")), true);
                return;
            }
        };
        let previous: Vec<ScreenIdentity> = self
            .intent
            .keep_off
            .iter()
            .map(|x| x.to_identity())
            .collect();
        if intent.request_id == 0 {
            self.fail("error", Some("关屏请求缺少编号。"), true);
            return;
        }
        if intent.request_id <= self.processed_request {
            return;
        }
        self.processed_request = intent.request_id;
        self.last_intent_text = Some(text);
        self.intent = intent;
        let selected: Vec<_> = self
            .intent
            .keep_off
            .iter()
            .map(|x| x.to_identity())
            .collect();
        if selected.is_empty() {
            self.finish("release", true);
            return;
        }
        let shrink = previous
            .iter()
            .any(|old| !selected.iter().any(|id| id.matches(old)));
        let ok = if shrink {
            self.try_apply_from_saved(&selected)
        } else {
            self.try_apply(&selected, false)
        };
        if self.waiting_vdd {
            return;
        }
        if !ok && !self.exited && self.state != RecoveryState::RestoreFailed {
            self.fail("error", Some("操作失败，已结束本轮关闭要求。"), true);
        }
    }

    fn newer_intent_pending(&self) -> bool {
        let path = SessionPaths::intent(&self.opt.directory);
        let Ok(text) = std::fs::read_to_string(&path) else {
            return false;
        };
        if self.last_intent_text.as_deref() == Some(text.as_str()) {
            return false;
        }
        let Ok(intent) = serde_json::from_str::<IntentFile>(&text) else {
            return false;
        };
        intent.request_id > self.processed_request && !intent.keep_off.is_empty()
    }

    fn try_apply_from_saved(&mut self, selected: &[ScreenIdentity]) -> bool {
        let _guard = match crate::maintenance::OperationLock::for_keep_off() {
            Ok(g) => g,
            Err(e) => {
                self.result.error = Some(e);
                return false;
            }
        };
        if self.saved_paths.is_empty() || self.saved_identities.len() != self.saved_paths.len() {
            self.write_heartbeat("保存拓扑无法用于单屏恢复。", Some(selected), true);
            return false;
        }
        if selected
            .iter()
            .any(|id| !self.saved_identities.iter().any(|saved| saved.matches(id)))
        {
            self.write_heartbeat(
                "保存拓扑与当前设备对不上，未恢复该屏。",
                Some(selected),
                true,
            );
            return false;
        }
        let prepared = match PathOps::deactivate(
            &self.saved_paths,
            &self.saved_modes,
            &self.saved_identities,
            selected,
            true,
        ) {
            Ok(p) => p,
            Err(e) => {
                self.write_heartbeat(&e, Some(selected), true);
                return false;
            }
        };
        if !prepared.can_apply() {
            self.write_heartbeat("无法从保存拓扑恢复该屏，未 APPLY。", Some(selected), true);
            return false;
        }
        let rc = match self.opt.ccd.set(
            &prepared.paths,
            &prepared.modes,
            CcdConstants::VALIDATE_FLAGS,
        ) {
            Ok(v) => v,
            Err(e) => {
                self.write_heartbeat(&e, Some(selected), true);
                return false;
            }
        };
        if rc != 0 {
            SessionLog::append(
                &self.opt.directory,
                "apply-blocked",
                Some(&format!("partial-restore VALIDATE {rc}")),
                None,
                Some(false),
                Some(rc),
            );
            self.write_heartbeat(
                &format!("无法从保存拓扑恢复该屏：校验 {rc}。"),
                Some(selected),
                true,
            );
            return false;
        }
        let apply_rc =
            match self
                .opt
                .ccd
                .set(&prepared.paths, &prepared.modes, CcdConstants::APPLY_FLAGS)
            {
                Ok(v) => v,
                Err(e) => {
                    self.write_heartbeat(&e, Some(selected), true);
                    return false;
                }
            };
        self.result.apply_rc = Some(apply_rc);
        self.result.adjusted_origin = Some(prepared.adjusted_origin);
        if apply_rc != 0 {
            SessionLog::append(
                &self.opt.directory,
                "apply-failed",
                Some(&format!("partial-restore APPLY {apply_rc}")),
                None,
                Some(false),
                Some(apply_rc),
            );
            self.write_heartbeat(&format!("APPLY 失败：{apply_rc}。"), Some(selected), true);
            return false;
        }
        self.state = RecoveryState::Holding;
        self.holding = true;
        self.expected_targets = Some(PathOps::active_targets(&prepared.paths));
        SessionLog::append(
            &self.opt.directory,
            "partial-restored",
            Some("已恢复该屏，其余仍保持关闭。"),
            None,
            Some(false),
            Some(apply_rc),
        );
        self.write_heartbeat("已恢复该屏，其余仍保持关闭。", Some(selected), false);
        true
    }

    fn try_apply(&mut self, selected: &[ScreenIdentity], is_reapply: bool) -> bool {
        let _guard = match crate::maintenance::OperationLock::for_keep_off() {
            Ok(g) => g,
            Err(e) => {
                self.result.error = Some(e);
                return false;
            }
        };
        let frame = match self.opt.ccd.capture(CcdConstants::QUERY_FLAGS) {
            Ok(f) => f,
            Err(e) => {
                self.write_heartbeat(&e, Some(selected), true);
                return false;
            }
        };
        let plan = Gate::plan_keep_off(
            &frame.snapshot,
            selected,
            self.intent.vdd_assist || frame.snapshot.has_active_bundled_vdd(),
        );
        if plan.action == KeepOffAction::EnableBundledVdd
            || plan.action == KeepOffAction::InstallBundledVdd
        {
            self.request_bundled_vdd();
            return false;
        }
        if plan.action == KeepOffAction::Blocked {
            let blocked = plan
                .block_reason
                .clone()
                .unwrap_or_else(|| Gate::LAST_PATH_REASON.into());
            SessionLog::append(
                &self.opt.directory,
                "apply-blocked",
                Some(&blocked),
                Some(&format!("{:?}", plan.action)),
                Some(is_reapply),
                None,
            );
            self.write_heartbeat(&blocked, Some(selected), true);
            return false;
        }
        let identities: Vec<_> = frame.snapshot.paths.iter().map(|p| p.identity()).collect();
        let still_active: Vec<_> = selected
            .iter()
            .filter(|id| {
                frame
                    .snapshot
                    .paths
                    .iter()
                    .any(|p| p.active && p.is_physical() && id.matches(&p.identity()))
            })
            .cloned()
            .collect();
        if still_active.is_empty() {
            if frame.snapshot.active_paths().next().is_some() {
                self.state = RecoveryState::Holding;
                self.holding = true;
                self.expected_targets = Some(PathOps::active_targets(&frame.paths));
                SessionLog::append(
                    &self.opt.directory,
                    "already-off",
                    Some("所选物理屏已关，未再 APPLY。"),
                    None,
                    Some(is_reapply),
                    None,
                );
                self.write_heartbeat(
                    if is_reapply {
                        "醒后所选屏仍关着。"
                    } else {
                        "已保持关闭。"
                    },
                    Some(selected),
                    false,
                );
                return true;
            }
            SessionLog::append(
                &self.opt.directory,
                "apply-blocked",
                Some("没有剩余活动路径。"),
                None,
                Some(is_reapply),
                None,
            );
            self.write_heartbeat(
                "VALIDATE 后没有剩余活动路径，未 APPLY。",
                Some(selected),
                true,
            );
            return false;
        }
        let prepared = match PathOps::deactivate(
            &frame.paths,
            &frame.modes,
            &identities,
            &still_active,
            plan.adjust_origin,
        ) {
            Ok(p) => p,
            Err(e) => {
                self.write_heartbeat(&e, Some(selected), true);
                return false;
            }
        };
        if !prepared.can_apply() {
            self.write_heartbeat(
                "VALIDATE 后没有剩余活动路径，未 APPLY。",
                Some(selected),
                true,
            );
            return false;
        }
        let mut paths = prepared.paths.clone();
        let modes = prepared.modes.clone();
        let adjusted_clone = false;
        let mut rc = match self
            .opt
            .ccd
            .set(&paths, &modes, CcdConstants::VALIDATE_FLAGS)
        {
            Ok(v) => v,
            Err(e) => {
                self.write_heartbeat(&e, Some(selected), true);
                return false;
            }
        };
        if rc == 87 && plan.may_adjust_clone {
            if !keeps_active_bundled_vdd(&frame.snapshot, &paths) {
                self.write_heartbeat(
                    "停用物理路径后没有活动辅助输出，未改拓扑。",
                    Some(selected),
                    true,
                );
                return false;
            }
            invalidate_inactive_source_indices(&mut paths);
            rc = match self
                .opt
                .ccd
                .set(&paths, &modes, CcdConstants::VALIDATE_FLAGS)
            {
                Ok(v) => v,
                Err(e) => {
                    self.write_heartbeat(&e, Some(selected), true);
                    return false;
                }
            };
            if rc != 0 {
                self.write_heartbeat(&format!("无法保持关闭：校验 {rc}。"), Some(selected), true);
                self.result.apply_rc = Some(rc);
                return false;
            }
        }
        if rc != 0 {
            SessionLog::append(
                &self.opt.directory,
                "apply-blocked",
                Some(&format!("VALIDATE {rc}")),
                None,
                Some(is_reapply),
                Some(rc),
            );
            self.write_heartbeat(&format!("无法保持关闭：校验 {rc}。"), Some(selected), true);
            self.result.apply_rc = Some(rc);
            return false;
        }
        let apply_rc = match self.opt.ccd.set(&paths, &modes, CcdConstants::APPLY_FLAGS) {
            Ok(v) => v,
            Err(e) => {
                self.write_heartbeat(&e, Some(selected), true);
                return false;
            }
        };
        self.result.apply_rc = Some(apply_rc);
        self.result.adjusted_origin = Some(prepared.adjusted_origin);
        self.result.adjusted_clone = Some(adjusted_clone);
        if apply_rc != 0 {
            SessionLog::append(
                &self.opt.directory,
                "apply-failed",
                Some(&format!("APPLY {apply_rc}")),
                None,
                Some(is_reapply),
                Some(apply_rc),
            );
            self.write_heartbeat(&format!("APPLY 失败：{apply_rc}。"), Some(selected), true);

            return false;
        }
        self.state = RecoveryState::Holding;
        self.holding = true;
        self.expected_targets = Some(PathOps::active_targets(&paths));
        SessionLog::append(
            &self.opt.directory,
            if is_reapply { "reapplied" } else { "applied" },
            Some(if is_reapply {
                "已再次保持关闭。"
            } else {
                "已保持关闭。"
            }),
            None,
            Some(is_reapply),
            Some(apply_rc),
        );
        self.write_heartbeat(
            if is_reapply {
                "已再次保持关闭。"
            } else {
                "已保持关闭。"
            },
            Some(selected),
            false,
        );
        true
    }

    fn handle_interrupt(&mut self, reason: &str) {
        self.result.reason = reason.into();
        SessionLog::append(
            &self.opt.directory,
            "interrupt",
            None,
            Some(reason),
            Some(self.reapply_attempted),
            None,
        );
        self.restore_saved();
        if self.result.restore_state != RestoreState::Complete {
            self.fail(reason, Some("中断后恢复未完成。"), false);
            return;
        }
        let selected: Vec<_> = self
            .intent
            .keep_off
            .iter()
            .map(|x| x.to_identity())
            .collect();
        let allow_reapply = reason == "unexpected-topology";
        if allow_reapply && !self.reapply_attempted && !selected.is_empty() {
            self.reapply_attempted = true;
            self.result.reapply_attempted = true;
            SessionLog::append(
                &self.opt.directory,
                "reapply-attempt",
                None,
                Some(reason),
                None,
                None,
            );
            self.wait_for_selected_physical(&selected);
            if self.try_apply(&selected, true) && self.holding {
                self.previous = self.opt.clock.seconds();
                return;
            }
            if self.waiting_vdd {
                self.previous = self.opt.clock.seconds();
                return;
            }
        }
        if !self.exited && self.state != RecoveryState::RestoreFailed {
            self.finish(reason, true);
        }
    }

    fn finish(&mut self, reason: &str, restore: bool) {
        if self.exited {
            return;
        }
        self.result.reason = reason.into();
        self.intent.keep_off.clear();
        self.waiting_vdd = false;
        self.armed = true;
        if restore {
            self.restore_saved();
        }
        self.result.ok = matches!(reason, "hotkey" | "release" | "parent-exit")
            && self.result.restore_state == RestoreState::Complete;
        let complete = matches!(
            self.result.restore_state,
            RestoreState::Complete | RestoreState::NotNeeded
        );
        self.state = if complete {
            RecoveryState::Finished
        } else {
            RecoveryState::RestoreFailed
        };
        self.holding = false;
        self.expected_targets = None;
        let detail = if complete {
            finish_heartbeat(reason)
        } else {
            "恢复未完成，已停止自动操作；请按热键或点恢复全部重试。".into()
        };
        self.write_heartbeat(&detail, None, !complete);
        SessionLog::append(
            &self.opt.directory,
            "finish",
            Some(&detail),
            Some(reason),
            None,
            self.result.apply_rc,
        );
        // Keep the worker and hotkey available after a failed restoration.
        if complete || reason == "parent-exit" || self.opt.parent_pid == 0 {
            self.write_result();
        }
    }

    fn fail(&mut self, reason: &str, error: Option<&str>, restore: bool) {
        let restore = restore || (!self.armed && self.baseline_changed);
        self.result.error = error.map(str::to_owned);
        if !self.armed && !restore && !self.baseline_changed {
            self.result.reason = reason.into();
            self.result.restore_state = RestoreState::NotNeeded;
            self.write_result();
            return;
        }
        if !self.armed && restore && !self.baseline_changed {
            self.result.reason = reason.into();
            self.result.restore_state = RestoreState::Unknown;
            self.write_result();
            return;
        }
        self.armed = true;
        self.finish(reason, restore);
    }

    fn restore_saved(&mut self) {
        self.state = RecoveryState::Restoring;
        self.write_heartbeat("正在恢复物理输出。", None, false);
        self.result.restore_state = RestoreState::Unknown;
        self.result.restored_targets = false;
        self.result.restored_topology = false;
        self.result.fallback_rc = None;
        if self.saved_paths.is_empty() {
            return;
        }
        self.result.restore_rc = self
            .opt
            .ccd
            .set(
                &self.saved_paths,
                &self.saved_modes,
                CcdConstants::APPLY_FLAGS,
            )
            .ok();
        self.observe_restoration(10);
        if self.result.restore_state != RestoreState::Complete {
            self.result.fallback_rc = self
                .opt
                .ccd
                .set_topology(CcdConstants::SDC_APPLY | CcdConstants::SDC_TOPOLOGY_INTERNAL)
                .ok();
            self.observe_restoration(11);
        }
        self.holding = false;
        self.expected_targets = None;
    }

    fn observe_restoration(&mut self, attempts: usize) {
        for attempt in 0..attempts {
            let active = self.opt.ccd.capture(CcdConstants::QUERY_FLAGS);
            let connected = self.opt.ccd.connected_physical();
            self.result.restore_state = match (active, connected) {
                (Ok(frame), Ok(connected)) => {
                    self.result.restored_topology =
                        TopologyBlob::fingerprint(&frame.paths, &frame.modes)
                            == self.saved_fingerprint;
                    let physical: Vec<_> = frame
                        .snapshot
                        .active_physical()
                        .map(|p| p.identity())
                        .collect();
                    let complete = !physical.is_empty()
                        && self.physical_targets.iter().all(|id| {
                            !connected.iter().any(|c| c.matches(id))
                                || physical.iter().any(|p| p.matches(id))
                        });
                    self.result.restored_targets = complete;
                    if complete {
                        RestoreState::Complete
                    } else {
                        RestoreState::Partial
                    }
                }
                _ => RestoreState::Unknown,
            };
            if self.result.restore_state == RestoreState::Complete {
                return;
            }
            if attempt + 1 < attempts {
                (self.opt.pause)(Duration::from_millis(150));
            }
        }
    }

    fn write_heartbeat(&mut self, detail: &str, selected: Option<&[ScreenIdentity]>, failed: bool) {
        let owned: Vec<ScreenIdentity> = self
            .intent
            .keep_off
            .iter()
            .map(|x| x.to_identity())
            .collect();
        let selected = selected.unwrap_or(&owned);
        let snapshot = self
            .opt
            .ccd
            .capture(CcdConstants::QUERY_FLAGS)
            .map(|f| f.snapshot)
            .unwrap_or_else(|_| DisplaySnapshot::new(vec![], 0));
        let previous =
            JsonUtil::try_read::<HeartbeatFile>(SessionPaths::heartbeat(&self.opt.directory));
        let previous_row = |id: &ScreenIdentity| {
            previous.as_ref().and_then(|hb| {
                hb.screens.iter().find(|s| {
                    id.matches(&ScreenIdentity::new(
                        &s.adapter_luid,
                        s.target_id,
                        &s.monitor_path,
                    ))
                })
            })
        };
        let mut screens = Vec::new();
        for row in snapshot.physical_screens() {
            let wanted = if selected.iter().any(|id| id.matches(&row.identity())) {
                "保持关闭"
            } else {
                "开启"
            };
            let confirmed = if wanted == "保持关闭" {
                if failed {
                    "失败"
                } else if !row.active && self.holding {
                    "已关闭"
                } else if self.armed && !self.holding {
                    "处理中"
                } else if row.active {
                    "处理中"
                } else {
                    "已关闭"
                }
            } else if row.active {
                "已显示"
            } else {
                "未知"
            };
            let prev = previous_row(&row.identity());
            screens.push(HeartbeatScreen {
                adapter_luid: row.adapter_luid.clone(),
                target_id: row.target_id,
                monitor_path: row.monitor_path.clone(),
                name: resolved_screen_name(
                    Some(&row.monitor_name),
                    Some(&row.source_name),
                    prev.map(|s| s.name.as_str()),
                    &row.monitor_path,
                    false,
                ),
                kind: row.kind_label().into(),
                wanted: wanted.into(),
                confirmed: confirmed.into(),
                detail: if wanted == "保持关闭" {
                    detail.into()
                } else {
                    String::new()
                },
            });
        }
        for id in selected {
            if screens.iter().any(|s| {
                id.matches(&ScreenIdentity::new(
                    &s.adapter_luid,
                    s.target_id,
                    &s.monitor_path,
                ))
            }) {
                continue;
            }
            let prev = previous_row(id);
            screens.push(HeartbeatScreen {
                adapter_luid: id.adapter_luid.clone(),
                target_id: id.target_id,
                monitor_path: id.monitor_path.clone(),
                name: resolved_screen_name(
                    None,
                    None,
                    prev.map(|s| s.name.as_str()),
                    &id.monitor_path,
                    true,
                ),
                kind: prev
                    .map(|s| s.kind.clone())
                    .filter(|k| !k.trim().is_empty())
                    .unwrap_or_else(|| "物理".into()),
                wanted: "保持关闭".into(),
                confirmed: if failed {
                    "失败".into()
                } else if self.holding {
                    "已关闭".into()
                } else {
                    "处理中".into()
                },
                detail: detail.into(),
            });
        }
        let _ = JsonUtil::write_atomic(
            SessionPaths::heartbeat(&self.opt.directory),
            &HeartbeatFile {
                state: self.state,
                processed_request_id: self.processed_request.max(self.processed_release),
                hotkey_registered: self.hotkey_registered,
                armed: self.armed,
                screens,
                detail: Some(detail.into()),
            },
        );
    }

    fn write_result(&mut self) {
        if let Err(error) =
            JsonUtil::write_atomic(SessionPaths::result(&self.opt.directory), &self.result)
        {
            self.state = RecoveryState::RestoreFailed;
            self.write_heartbeat(&format!("无法写入恢复结果：{error}"), None, true);
            if self.result.reason == "parent-exit" || self.opt.parent_pid == 0 {
                if self.hotkey_registered {
                    self.opt.hotkey.unregister();
                    self.hotkey_registered = false;
                }
                self.exited = true;
            }
            return;
        }
        if self.hotkey_registered {
            self.opt.hotkey.unregister();
            self.hotkey_registered = false;
        }
        self.exited = true;
    }

    fn handle_power_resume(&mut self, now: f64) {
        SessionLog::append(
            &self.opt.directory,
            "power-resume",
            Some("系统已唤醒，回放关屏前显示。"),
            None,
            None,
            None,
        );
        if !self.holding && !self.waiting_vdd && self.intent.keep_off.is_empty() {
            self.previous = now;
            self.write_heartbeat("系统已唤醒。", None, false);
            return;
        }
        self.wait_for_physical_enumerate();
        self.finish("suspend-resume", true);
    }

    fn wait_for_physical_enumerate(&mut self) {
        let attempts = self.opt.reapply_settle_attempts.max(1);
        for i in 0..attempts {
            if let Ok(snap) = self
                .opt
                .ccd
                .capture(CcdConstants::QUERY_FLAGS)
                .map(|f| f.snapshot)
            {
                if snap.active_physical().next().is_some() {
                    SessionLog::append(
                        &self.opt.directory,
                        "resume-settle",
                        Some(&format!("physical-active attempt {}", i + 1)),
                        None,
                        None,
                        None,
                    );
                    return;
                }
            }
            if i + 1 < attempts {
                (self.opt.pause)(self.opt.reapply_settle_pause);
            }
        }
        SessionLog::append(
            &self.opt.directory,
            "resume-settle",
            Some("timeout, no physical"),
            None,
            None,
            None,
        );
    }

    fn wait_for_selected_physical(&mut self, selected: &[ScreenIdentity]) {
        let attempts = self.opt.reapply_settle_attempts.max(1);
        for i in 0..attempts {
            if let Ok(snap) = self
                .opt
                .ccd
                .capture(CcdConstants::QUERY_FLAGS)
                .map(|f| f.snapshot)
            {
                if selected.iter().any(|id| {
                    snap.paths
                        .iter()
                        .any(|p| p.active && p.is_physical() && id.matches(&p.identity()))
                }) {
                    SessionLog::append(
                        &self.opt.directory,
                        "reapply-settle",
                        Some(&format!("selected-active attempt {}", i + 1)),
                        None,
                        None,
                        None,
                    );
                    return;
                }
            }
            if i + 1 < attempts {
                (self.opt.pause)(self.opt.reapply_settle_pause);
            }
        }
        SessionLog::append(
            &self.opt.directory,
            "reapply-settle",
            Some("timeout, selected still off"),
            None,
            None,
            None,
        );
    }

    fn request_bundled_vdd(&mut self) {
        self.waiting_vdd = true;
        self.vdd_activate_attempted = false;
        self.vdd_activate_logged = false;
        self.vdd_instance_seen = false;
        self.last_virtual_adapters = None;
        self.vdd_wait_start = self.opt.clock.seconds();
        let write = JsonUtil::write_atomic(
            SessionPaths::vdd_request(&self.opt.directory),
            &VddRequestFile {
                at: self.vdd_wait_start,
                reason: "reapply".into(),
            },
        );
        if let Err(e) = write {
            self.waiting_vdd = false;
            self.fail("error", Some(&e), true);
            return;
        }
        SessionLog::append(
            &self.opt.directory,
            "vdd-request",
            Some("再关需要再次启用辅助虚拟输出。"),
            None,
            None,
            None,
        );
        self.write_heartbeat("等待再次启用隐藏辅助输出。", None, false);
    }

    fn tick_waiting_vdd(&mut self, now: f64) {
        if now - self.vdd_wait_start > self.opt.vdd_wait_seconds {
            SessionLog::append(
                &self.opt.directory,
                "apply-blocked",
                Some("等待辅助虚拟输出超时。"),
                None,
                None,
                None,
            );
            if !self.vdd_instance_seen {
                SessionLog::append(
                    &self.opt.directory,
                    "apply-blocked",
                    Some("全部路径里没有辅助虚拟输出设备。"),
                    None,
                    None,
                    None,
                );
            }
            let reason = if self.result.reason.is_empty() || self.result.reason == "not-armed" {
                "execution-gap".into()
            } else {
                self.result.reason.clone()
            };
            self.finish(&reason, true);
            return;
        }
        let Ok(active) = self.opt.ccd.capture(CcdConstants::QUERY_FLAGS) else {
            return;
        };
        if active.snapshot.has_active_bundled_vdd() {
            self.vdd_instance_seen = true;
            self.proceed_after_vdd(now);
            return;
        }
        let Ok(all) = self.opt.ccd.capture(CcdConstants::ALL_PATH_FLAGS) else {
            return;
        };
        self.note_virtual_adapters(&all.snapshot);
        if all.snapshot.paths.iter().any(|row| row.is_bundled_vdd()) {
            self.vdd_instance_seen = true;
        }
        let inactive = all
            .snapshot
            .paths
            .iter()
            .any(|row| row.is_bundled_vdd() && !row.active);
        if inactive && !self.vdd_activate_attempted {
            if self.activate_inactive_bundled(&active, &all) {
                let Ok(after) = self.opt.ccd.capture(CcdConstants::QUERY_FLAGS) else {
                    return;
                };
                if after.snapshot.has_active_bundled_vdd() {
                    self.proceed_after_vdd(now);
                }
            }
        }
    }

    fn proceed_after_vdd(&mut self, now: f64) {
        let _ = std::fs::remove_file(SessionPaths::vdd_request(&self.opt.directory));
        SessionLog::append(&self.opt.directory, "vdd-ready", None, None, None, None);
        let selected: Vec<_> = self
            .intent
            .keep_off
            .iter()
            .map(|x| x.to_identity())
            .collect();
        self.waiting_vdd = false;
        if self.try_apply(&selected, true) && self.holding {
            self.previous = now;
            return;
        }
        let reason = self.result.reason.clone();
        self.finish(&reason, true);
    }

    fn note_virtual_adapters(&mut self, snapshot: &DisplaySnapshot) {
        let mut paths: Vec<_> = snapshot
            .paths
            .iter()
            .filter(|row| row.role == crate::PathRole::Virtual)
            .map(|row| row.adapter_path.clone())
            .collect();
        paths.sort();
        paths.dedup();
        let text = if paths.is_empty() {
            "没有虚拟适配器路径".to_string()
        } else {
            paths.join(" | ")
        };
        if self.last_virtual_adapters.as_deref() == Some(text.as_str()) {
            return;
        }
        self.last_virtual_adapters = Some(text.clone());
        SessionLog::append(
            &self.opt.directory,
            "vdd-paths",
            Some(&text),
            None,
            None,
            None,
        );
    }

    fn activate_inactive_bundled(&mut self, active: &CcdFrame, all: &CcdFrame) -> bool {
        if active.paths.len() != active.snapshot.paths.len()
            || all.paths.len() != all.snapshot.paths.len()
        {
            self.write_heartbeat("路径与描述数量不一致，未改拓扑。", None, true);
            return false;
        }
        let selected: Vec<_> = self
            .intent
            .keep_off
            .iter()
            .map(|item| item.to_identity())
            .collect();
        let pick_bundled = |available: bool| {
            all.snapshot
                .paths
                .iter()
                .enumerate()
                .find(|(index, row)| {
                    row.is_bundled_vdd()
                        && !row.active
                        && (all.paths[*index].target_info.target_available != 0) == available
                })
        };
        let Some((bundled_index, _)) = pick_bundled(true).or_else(|| pick_bundled(false)) else {
            return false;
        };
        let mut paths: Vec<_> = active
            .paths
            .iter()
            .zip(active.snapshot.paths.iter())
            .filter(|(_, row)| row.active)
            .map(|(path, _)| *path)
            .collect();
        let mut bundled = all.paths[bundled_index];
        bundled.flags |= CcdConstants::DISPLAYCONFIG_PATH_ACTIVE
            | CcdConstants::DISPLAYCONFIG_PATH_SUPPORT_VIRTUAL_MODE;
        let mut modes = active.modes.clone();
        if let Err(error) = attach_active_modes(&mut bundled, &mut modes) {
            self.write_heartbeat(&error, Some(&selected), true);
            return false;
        }
        paths.push(bundled);
        let mut validate_rc = match self
            .opt
            .ccd
            .set(&paths, &modes, CcdConstants::VALIDATE_FLAGS)
        {
            Ok(rc) => rc,
            Err(error) => {
                self.write_heartbeat(&error, Some(&selected), true);
                return false;
            }
        };
        if validate_rc == 87 {
            if let Some(path) = paths.last_mut() {
                path.target_info.scaling = CcdConstants::DISPLAYCONFIG_SCALING_PREFERRED;
            }
            validate_rc = match self
                .opt
                .ccd
                .set(&paths, &modes, CcdConstants::VALIDATE_FLAGS)
            {
                Ok(rc) => rc,
                Err(error) => {
                    self.write_heartbeat(&error, Some(&selected), true);
                    return false;
                }
            };
        }
        if validate_rc != 0 {
            if !self.vdd_activate_logged {
                self.vdd_activate_logged = true;
                let bundled = paths.last();
                let detail = match bundled {
                    Some(path) => format!(
                        "激活辅助路径校验 {validate_rc}，活动路径 {}，辅助源 {:08x} 目标 {:08x}，未改拓扑。",
                        paths.len(),
                        path.source_info.mode_info_idx,
                        path.target_info.mode_info_idx
                    ),
                    None => format!("激活辅助路径校验 {validate_rc}，未改拓扑。"),
                };
                SessionLog::append(
                    &self.opt.directory,
                    "apply-blocked",
                    Some(&detail),
                    None,
                    None,
                    Some(validate_rc),
                );
            }
            return false;
        }
        self.vdd_activate_attempted = true;
        let apply_rc = match self.opt.ccd.set(&paths, &modes, CcdConstants::APPLY_FLAGS) {
            Ok(rc) => rc,
            Err(error) => {
                self.write_heartbeat(&error, Some(&selected), true);
                return false;
            }
        };
        if apply_rc != 0 {
            SessionLog::append(
                &self.opt.directory,
                "apply-failed",
                Some(&format!("激活辅助路径 APPLY {apply_rc}")),
                None,
                None,
                Some(apply_rc),
            );
            return false;
        }
        SessionLog::append(
            &self.opt.directory,
            "vdd-activate",
            Some("已把未活动的辅助路径标为活动，已关物理屏保持关闭。"),
            None,
            None,
            Some(apply_rc),
        );
        true
    }
}

fn keeps_active_bundled_vdd(
    snapshot: &crate::DisplaySnapshot,
    paths: &[crate::native::DisplayConfigPathInfo],
) -> bool {
    paths.iter().any(|path| {
        path.flags & CcdConstants::DISPLAYCONFIG_PATH_ACTIVE != 0
            && snapshot.paths.iter().any(|row| {
                row.is_bundled_vdd()
                    && row
                        .adapter_luid
                        .eq_ignore_ascii_case(&path.target_info.adapter_id.to_hex())
                    && row.target_id == path.target_info.id
            })
    })
}

fn attach_active_modes(
    path: &mut crate::native::DisplayConfigPathInfo,
    modes: &mut Vec<DisplayConfigModeInfo>,
) -> Result<(), String> {
    if modes.len() > u16::MAX as usize - 3 {
        return Err("模式表已满，无法激活辅助路径。".into());
    }
    let width = 1920u32;
    let height = 1200u32;
    let total_w = width + 160;
    let total_h = height + 119;
    let origin_x = modes
        .iter()
        .filter(|mode| mode.info_type == CcdConstants::DISPLAYCONFIG_MODE_INFO_TYPE_SOURCE)
        .map(|mode| {
            let source = mode.source_mode();
            source.position.x.saturating_add(source.width as i32)
        })
        .max()
        .unwrap_or(0);
    let source_index = modes.len() as u32;
    let mut source = DisplayConfigModeInfo {
        info_type: CcdConstants::DISPLAYCONFIG_MODE_INFO_TYPE_SOURCE,
        id: path.source_info.id,
        adapter_id: path.source_info.adapter_id,
        ..DisplayConfigModeInfo::default()
    };
    source.set_source_mode(crate::native::DisplayConfigSourceMode {
        width,
        height,
        pixel_format: CcdConstants::DISPLAYCONFIG_PIXELFORMAT_32BPP,
        position: crate::native::PointL { x: origin_x, y: 0 },
    });
    modes.push(source);
    let target_index = modes.len() as u32;
    let mut target = DisplayConfigModeInfo {
        info_type: CcdConstants::DISPLAYCONFIG_MODE_INFO_TYPE_TARGET,
        id: path.target_info.id,
        adapter_id: path.target_info.adapter_id,
        ..DisplayConfigModeInfo::default()
    };
    let mut signal = crate::native::DisplayConfigVideoSignalInfo::default();
    signal.pixel_rate = u64::from(total_w) * u64::from(total_h) * 60;
    signal.h_sync_freq = crate::native::DisplayConfigRational {
        numerator: total_h * 60,
        denominator: 1,
    };
    signal.v_sync_freq = crate::native::DisplayConfigRational {
        numerator: 60,
        denominator: 1,
    };
    signal.active_size = crate::native::DisplayConfig2DRegion {
        cx: width,
        cy: height,
    };
    signal.total_size = crate::native::DisplayConfig2DRegion {
        cx: total_w,
        cy: total_h,
    };
    signal.video_standard = 255;
    signal.scan_line_ordering = CcdConstants::DISPLAYCONFIG_SCANLINE_ORDERING_PROGRESSIVE;
    target.union.target_mode = crate::native::DisplayConfigTargetMode {
        target_video_signal_info: signal,
    };
    modes.push(target);
    let desktop_index = modes.len() as u32;
    let mut desktop = DisplayConfigModeInfo {
        info_type: CcdConstants::DISPLAYCONFIG_MODE_INFO_TYPE_DESKTOP_IMAGE,
        id: path.target_info.id,
        adapter_id: path.target_info.adapter_id,
        ..DisplayConfigModeInfo::default()
    };
    let size = crate::native::PointL {
        x: width as i32,
        y: height as i32,
    };
    let image = crate::native::RectL {
        left: 0,
        top: 0,
        right: size.x,
        bottom: size.y,
    };
    desktop.union.desktop_image_info = crate::native::DisplayConfigDesktopImageInfo {
        path_source_size: size,
        desktop_image_region: image,
        desktop_image_clip: image,
    };
    modes.push(desktop);
    path.source_info.mode_info_idx = (source_index << 16) | 0xFFFF;
    path.target_info.mode_info_idx = (target_index << 16) | (desktop_index & 0xFFFF);
    if path.target_info.rotation == 0 {
        path.target_info.rotation = 1;
    }
    path.target_info.scaling = CcdConstants::DISPLAYCONFIG_SCALING_IDENTITY;
    path.target_info.scan_line_ordering = CcdConstants::DISPLAYCONFIG_SCANLINE_ORDERING_PROGRESSIVE;
    path.target_info.refresh_rate = crate::native::DisplayConfigRational {
        numerator: 60,
        denominator: 1,
    };
    Ok(())
}

fn invalidate_inactive_source_indices(paths: &mut [crate::native::DisplayConfigPathInfo]) {
    for path in paths.iter_mut() {
        if path.flags & CcdConstants::DISPLAYCONFIG_PATH_ACTIVE != 0 {
            continue;
        }
        path.source_info.mode_info_idx = 0xFFFF_FFFF;
    }
}

fn keep_off_still_holds(snapshot: &DisplaySnapshot, selected: &[ScreenIdentity]) -> bool {
    if snapshot.active_paths().next().is_none() {
        return false;
    }
    for id in selected {
        if let Some(row) = snapshot
            .paths
            .iter()
            .find(|p| p.is_physical() && id.matches(&p.identity()))
        {
            if row.active {
                return false;
            }
        }
    }
    true
}

fn finish_heartbeat(reason: &str) -> String {
    match reason {
        "hotkey" => "已由热键恢复。".into(),
        "release" => "已恢复全部。".into(),
        "parent-exit" => "界面退出后已恢复。".into(),
        "execution-gap" => "会话中断，保持关闭已结束。".into(),
        "suspend-resume" => "系统休眠或待机后已恢复显示，保持关闭已结束。".into(),
        "unexpected-topology" => "显示拓扑已变化，保持关闭已结束。".into(),
        _ => "保持关闭已结束。".into(),
    }
}
