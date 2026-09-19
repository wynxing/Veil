use crate::capability::{DisplaySnapshot, Gate, KeepOffAction};
use crate::native::{
    CcdAbi, CcdApi, CcdConstants, DisplayConfigModeInfo, DisplayConfigPathInfo, Hotkey, MonotonicClock,
    ParentWatcher,
};
use crate::session::{
    ArmFile, HeartbeatFile, HeartbeatScreen, IntentFile, JsonUtil, ReadyFile, ResultFile, SessionLog,
    SessionPaths, VddRequestFile,
};
use crate::topology::{PathOps, TopologyBlob};
use crate::ScreenIdentity;
use std::path::PathBuf;
use std::time::Duration;

pub struct RecoveryOptions {
    pub directory: PathBuf,
    pub self_pid: i32,
    pub parent_pid: i32,
    pub ccd: Box<dyn CcdApi>,
    pub hotkey: Box<dyn Hotkey>,
    pub clock: Box<dyn MonotonicClock>,
    pub parent: Box<dyn ParentWatcher>,
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
            directory,
            self_pid: std::process::id() as i32,
            parent_pid: 0,
            ccd,
            hotkey,
            clock: Box::new(crate::native::TickClock),
            parent: Box::new(crate::native::Win32ParentWatcher),
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
    opt: RecoveryOptions,
    saved_paths: Vec<DisplayConfigPathInfo>,
    saved_modes: Vec<DisplayConfigModeInfo>,
    saved_fingerprint: String,
    armed: bool,
    holding: bool,
    reapply_attempted: bool,
    waiting_vdd: bool,
    vdd_wait_start: f64,
    hotkey_registered: bool,
    started: f64,
    previous: f64,
    last_intent_text: Option<String>,
    expected_targets: Option<Vec<(String, u32)>>,
    intent: IntentFile,
    pub result: ResultFile,
    pub exited: bool,
}

impl RecoverySession {
    pub fn new(opt: RecoveryOptions) -> Self {
        Self {
            opt,
            saved_paths: vec![],
            saved_modes: vec![],
            saved_fingerprint: String::new(),
            armed: false,
            holding: false,
            reapply_attempted: false,
            waiting_vdd: false,
            vdd_wait_start: 0.0,
            hotkey_registered: false,
            started: 0.0,
            previous: 0.0,
            last_intent_text: None,
            expected_targets: None,
            intent: IntentFile::default(),
            result: ResultFile {
                reason: "not-armed".into(),
                ok: false,
                ..Default::default()
            },
            exited: false,
        }
    }

    pub fn start(&mut self) {
        if let Err(ex) = self.start_inner() {
            self.fail("error", Some(&ex), false);
        }
    }

    fn start_inner(&mut self) -> Result<(), String> {
        CcdAbi::ensure_expected_layout()?;
        let topology_path = SessionPaths::topology(&self.opt.directory);
        if !topology_path.exists() {
            self.fail("error", Some("missing topology.json"), false);
            return Ok(());
        }
        let (paths, modes) = TopologyBlob::load(&topology_path)?;
        self.saved_fingerprint = TopologyBlob::fingerprint(&paths, &modes);
        self.saved_paths = paths;
        self.saved_modes = modes;
        let current = self.opt.ccd.capture(CcdConstants::QUERY_FLAGS)?;
        if TopologyBlob::fingerprint(&current.paths, &current.modes) != self.saved_fingerprint {
            self.fail("error", Some("topology changed since save"), false);
            return Ok(());
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
        SessionLog::append(&self.opt.directory, "ready", Some("热键已注册。"), None, None, None);
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
            self.fail("error", Some(&ex), self.holding || self.armed);
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
            if let Some(arm) = JsonUtil::try_read::<ArmFile>(SessionPaths::arm(&self.opt.directory)) {
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
                self.fail("not-armed", Some("recovery worker not ready within timeout"), false);
            }
            return Ok(());
        }
        if hotkey {
            self.finish("hotkey", true);
            return Ok(());
        }
        if SessionPaths::release(&self.opt.directory).exists() {
            self.finish("release", true);
            return Ok(());
        }
        if self.opt.parent_pid > 0 && !self.opt.parent.is_alive(self.opt.parent_pid)? {
            self.finish("parent-exit", true);
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
        self.apply_intent_if_needed();
        if self.exited {
            return Ok(());
        }
        if self.holding {
            if let Some(expected) = self.expected_targets.clone() {
                let frame = self.opt.ccd.capture(CcdConstants::QUERY_FLAGS)?;
                let current = PathOps::active_targets(&frame.paths);
                if current != expected {
                    let selected: Vec<_> = self.intent.keep_off.iter().map(|x| x.to_identity()).collect();
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
        let intent = match JsonUtil::read::<IntentFile>(&path) {
            Ok(v) => v,
            Err(ex) => {
                self.write_heartbeat(&format!("intent 无效：{ex}"), None, false);
                return;
            }
        };
        self.last_intent_text = Some(text);
        self.intent = intent;
        let selected: Vec<_> = self.intent.keep_off.iter().map(|x| x.to_identity()).collect();
        if selected.is_empty() {
            self.finish("release", true);
            return;
        }
        if !self.try_apply(&selected, false) {
            self.last_intent_text = None;
        }
    }

    fn try_apply(&mut self, selected: &[ScreenIdentity], is_reapply: bool) -> bool {
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
        if plan.action == KeepOffAction::EnableBundledVdd {
            if is_reapply {
                self.request_bundled_vdd();
                return false;
            }
            let enable = plan.block_reason.clone().unwrap_or_else(|| Gate::ENABLE_VDD_REASON.into());
            SessionLog::append(
                &self.opt.directory,
                "apply-blocked",
                Some(&enable),
                Some(&format!("{:?}", plan.action)),
                Some(false),
                None,
            );
            self.write_heartbeat(&enable, Some(selected), true);
            return false;
        }
        if plan.action == KeepOffAction::Blocked {
            let blocked = plan.block_reason.clone().unwrap_or_else(|| Gate::LAST_PATH_REASON.into());
            SessionLog::append(
                &self.opt.directory,
                "apply-blocked",
                Some(&blocked),
                Some(&format!("{:?}", plan.action)),
                Some(is_reapply),
                None,
            );
            self.write_heartbeat(&blocked, Some(selected), true);
            if is_reapply {
                let reason = self.result.reason.clone();
                self.finish(&reason, true);
            }
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
            self.write_heartbeat("VALIDATE 后没有剩余活动路径，未 APPLY。", Some(selected), true);
            return false;
        }
        let prepared = match PathOps::deactivate(&frame.paths, &frame.modes, &identities, &still_active, plan.adjust_origin)
        {
            Ok(p) => p,
            Err(e) => {
                self.write_heartbeat(&e, Some(selected), true);
                return false;
            }
        };
        if !prepared.can_apply() {
            self.write_heartbeat("VALIDATE 后没有剩余活动路径，未 APPLY。", Some(selected), true);
            return false;
        }
        let mut paths = prepared.paths.clone();
        let mut modes = prepared.modes.clone();
        let mut adjusted_clone = false;
        let mut rc = match self.opt.ccd.set(&paths, &modes, CcdConstants::VALIDATE_FLAGS) {
            Ok(v) => v,
            Err(e) => {
                self.write_heartbeat(&e, Some(selected), true);
                return false;
            }
        };
        if rc == 87 && plan.may_adjust_clone {
            let clone_rc = match self
                .opt
                .ccd
                .set_topology(CcdConstants::SDC_APPLY | CcdConstants::SDC_TOPOLOGY_CLONE)
            {
                Ok(v) => v,
                Err(e) => {
                    self.write_heartbeat(&e, Some(selected), true);
                    return false;
                }
            };
            if clone_rc != 0 {
                self.write_heartbeat(&format!("无法改为共用源拓扑：{clone_rc}。"), Some(selected), true);
                self.restore_saved();
                return false;
            }
            adjusted_clone = true;
            let frame = match self.opt.ccd.capture(CcdConstants::QUERY_FLAGS) {
                Ok(f) => f,
                Err(e) => {
                    self.write_heartbeat(&e, Some(selected), true);
                    self.restore_saved();
                    return false;
                }
            };
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
            let prepared = match PathOps::deactivate(&frame.paths, &frame.modes, &identities, &still_active, plan.adjust_origin)
            {
                Ok(p) => p,
                Err(e) => {
                    self.write_heartbeat(&e, Some(selected), true);
                    self.restore_saved();
                    return false;
                }
            };
            if !prepared.can_apply() {
                self.write_heartbeat("改为共用源后仍无法留下活动路径。", Some(selected), true);
                self.restore_saved();
                return false;
            }
            paths = prepared.paths;
            modes = prepared.modes;
            rc = match self.opt.ccd.set(&paths, &modes, CcdConstants::VALIDATE_FLAGS) {
                Ok(v) => v,
                Err(e) => {
                    self.write_heartbeat(&e, Some(selected), true);
                    self.restore_saved();
                    return false;
                }
            };
            if rc != 0 {
                self.write_heartbeat(&format!("无法保持关闭：校验 {rc}。"), Some(selected), true);
                self.restore_saved();
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
            self.restore_saved();
            if is_reapply {
                self.finish("error", false);
            }
            return false;
        }
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
        let selected: Vec<_> = self.intent.keep_off.iter().map(|x| x.to_identity()).collect();
        if !self.reapply_attempted && !selected.is_empty() {
            self.reapply_attempted = true;
            self.result.reapply_attempted = true;
            SessionLog::append(&self.opt.directory, "reapply-attempt", None, Some(reason), None, None);
            self.write_heartbeat("会话中断，尝试再关一次。", Some(&selected), false);
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
        self.finish(reason, false);
    }

    fn finish(&mut self, reason: &str, restore: bool) {
        if self.exited {
            return;
        }
        self.result.reason = reason.into();
        if restore {
            self.restore_saved();
        }
        self.result.ok = matches!(reason, "hotkey" | "release" | "parent-exit")
            && (self.result.apply_rc.is_none() || self.result.apply_rc == Some(0))
            && self.result.restore_rc == Some(0)
            && self.result.restored_targets
            && self.result.restored_topology;
        let hb = finish_heartbeat(reason);
        self.write_heartbeat(&hb, None, false);
        SessionLog::append(
            &self.opt.directory,
            "finish",
            Some(&hb),
            Some(reason),
            Some(self.result.reapply_attempted),
            self.result.apply_rc,
        );
        self.write_result();
    }

    fn fail(&mut self, reason: &str, error: Option<&str>, restore: bool) {
        self.result.reason = reason.into();
        self.result.error = error.map(|s| s.to_string());
        self.result.ok = false;
        if restore {
            self.restore_saved();
        }
        let detail = error.unwrap_or(reason);
        self.write_heartbeat(detail, None, false);
        SessionLog::append(&self.opt.directory, "finish", Some(detail), Some(reason), None, None);
        self.write_result();
    }

    fn restore_saved(&mut self) {
        if self.saved_paths.is_empty() {
            return;
        }
        self.result.restore_rc = self
            .opt
            .ccd
            .set(&self.saved_paths, &self.saved_modes, CcdConstants::APPLY_FLAGS)
            .ok();
        if let Ok(after) = self.opt.ccd.capture(CcdConstants::QUERY_FLAGS) {
            self.result.restored_targets =
                PathOps::active_targets(&after.paths) == PathOps::active_targets(&self.saved_paths);
            self.result.restored_topology =
                TopologyBlob::fingerprint(&after.paths, &after.modes) == self.saved_fingerprint;
        }
        if self.result.restore_rc != Some(0) || !self.result.restored_targets {
            self.result.fallback_rc = self
                .opt
                .ccd
                .set_topology(CcdConstants::SDC_APPLY | CcdConstants::SDC_TOPOLOGY_INTERNAL)
                .ok();
        }
        self.holding = false;
        self.expected_targets = None;
    }

    fn write_heartbeat(&mut self, detail: &str, selected: Option<&[ScreenIdentity]>, failed: bool) {
        let owned: Vec<ScreenIdentity> = self.intent.keep_off.iter().map(|x| x.to_identity()).collect();
        let selected = selected.unwrap_or(&owned);
        let snapshot = self
            .opt
            .ccd
            .capture(CcdConstants::QUERY_FLAGS)
            .map(|f| f.snapshot)
            .unwrap_or_else(|_| DisplaySnapshot::new(vec![], 0));
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
            screens.push(HeartbeatScreen {
                adapter_luid: row.adapter_luid.clone(),
                target_id: row.target_id,
                monitor_path: row.monitor_path.clone(),
                name: row.display_name(),
                wanted: wanted.into(),
                confirmed: confirmed.into(),
                detail: if wanted == "保持关闭" { detail.into() } else { String::new() },
            });
        }
        for id in selected {
            if screens.iter().any(|s| {
                id.matches(&ScreenIdentity::new(&s.adapter_luid, s.target_id, &s.monitor_path))
            }) {
                continue;
            }
            screens.push(HeartbeatScreen {
                adapter_luid: id.adapter_luid.clone(),
                target_id: id.target_id,
                monitor_path: id.monitor_path.clone(),
                name: id.monitor_path.clone(),
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
                hotkey_registered: self.hotkey_registered,
                armed: self.armed,
                screens,
                detail: Some(detail.into()),
            },
        );
    }

    fn write_result(&mut self) {
        if self.hotkey_registered {
            self.opt.hotkey.unregister();
            self.hotkey_registered = false;
        }
        let _ = JsonUtil::write_atomic(SessionPaths::result(&self.opt.directory), &self.result);
        self.exited = true;
    }

    fn wait_for_selected_physical(&mut self, selected: &[ScreenIdentity]) {
        let attempts = self.opt.reapply_settle_attempts.max(1);
        for i in 0..attempts {
            if let Ok(snap) = self.opt.ccd.capture(CcdConstants::QUERY_FLAGS).map(|f| f.snapshot) {
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
        self.vdd_wait_start = self.opt.clock.seconds();
        let _ = JsonUtil::write_atomic(
            SessionPaths::vdd_request(&self.opt.directory),
            &VddRequestFile {
                at: self.vdd_wait_start,
                reason: "reapply".into(),
            },
        );
        SessionLog::append(
            &self.opt.directory,
            "vdd-request",
            Some("再关需要再次启用自带 VDD。"),
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
                Some("等待自带 VDD 超时。"),
                None,
                None,
                None,
            );
            let reason = if self.result.reason.is_empty() || self.result.reason == "not-armed" {
                "execution-gap".into()
            } else {
                self.result.reason.clone()
            };
            self.finish(&reason, true);
            return;
        }
        let Ok(snap) = self.opt.ccd.capture(CcdConstants::QUERY_FLAGS).map(|f| f.snapshot) else {
            return;
        };
        if !snap.has_active_bundled_vdd() {
            return;
        }
        let _ = std::fs::remove_file(SessionPaths::vdd_request(&self.opt.directory));
        SessionLog::append(&self.opt.directory, "vdd-ready", None, None, None, None);
        let selected: Vec<_> = self.intent.keep_off.iter().map(|x| x.to_identity()).collect();
        self.waiting_vdd = false;
        if self.try_apply(&selected, true) && self.holding {
            self.previous = now;
            return;
        }
        let reason = self.result.reason.clone();
        self.finish(&reason, false);
    }
}

fn keep_off_still_holds(snapshot: &DisplaySnapshot, selected: &[ScreenIdentity]) -> bool {
    if snapshot.active_paths().next().is_none() {
        return false;
    }
    for id in selected {
        if let Some(row) = snapshot.paths.iter().find(|p| p.is_physical() && id.matches(&p.identity())) {
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
        "unexpected-topology" => "显示拓扑已变化，保持关闭已结束。".into(),
        _ => "保持关闭已结束。".into(),
    }
}
