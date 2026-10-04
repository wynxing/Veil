use std::sync::mpsc::Sender;
use std::time::Instant;

use veil_engine::{AuxiliaryInstallItem, ScreenItem};

use crate::operation_worker::{Action, Event, View};

pub(crate) const DEFAULT_DETAIL: &str =
    "托盘常驻。关面板退回托盘，不退出。黑色画面不是关屏成功。";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TrayCommand {
    Open,
    RestoreAll,
    Exit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PanelEffect {
    Stay,
    Present,
    Dismiss,
    Exit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SessionStop {
    Dismiss,
    Exit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum IdleAction {
    Wait,
    Present,
    Exit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LoopNext {
    Idle,
    ExitProcess,
}

pub(crate) fn should_release_single_instance(stop: SessionStop) -> bool {
    matches!(stop, SessionStop::Exit)
}

pub(crate) fn after_ui_session(stop: SessionStop) -> LoopNext {
    if should_release_single_instance(stop) {
        LoopNext::ExitProcess
    } else {
        LoopNext::Idle
    }
}

pub(crate) fn merge_effect(left: PanelEffect, right: PanelEffect) -> PanelEffect {
    use PanelEffect::*;
    match (left, right) {
        (Exit, _) | (_, Exit) => Exit,
        (Present, _) | (_, Present) => Present,
        (Dismiss, _) | (_, Dismiss) => Dismiss,
        (Stay, Stay) => Stay,
    }
}

pub(crate) fn idle_response(effect: PanelEffect) -> IdleAction {
    match effect {
        PanelEffect::Present => IdleAction::Present,
        PanelEffect::Exit => IdleAction::Exit,
        PanelEffect::Stay | PanelEffect::Dismiss => IdleAction::Wait,
    }
}

/// Tray commands observed while no window exists. Open only asks for a window.
/// Restore and exit are replayed after the window is created so the existing
/// submit path still runs.
pub(crate) fn classify_tray_commands(commands: &[TrayCommand]) -> (bool, Vec<TrayCommand>) {
    let mut present = false;
    let mut deferred = Vec::new();
    for command in commands {
        match command {
            TrayCommand::Open => present = true,
            TrayCommand::RestoreAll | TrayCommand::Exit => {
                present = true;
                if !deferred.contains(command) {
                    deferred.push(*command);
                }
            }
        }
    }
    (present, deferred)
}

pub(crate) struct SessionState {
    pub pending: Vec<(u64, Action)>,
    pub operation_started: Instant,
    pub stage_started: Instant,
    pub stage: String,
    pub auto_stage: bool,
    pub cancel_requested: bool,
    pub last_operation_error: Option<String>,
    pub worker_failed: bool,
    pub confirmation: Option<(String, Sender<bool>)>,
    pub has_session: bool,
    pub screens: Vec<ScreenItem>,
    pub auxiliary: AuxiliaryInstallItem,
    pub detail: String,
    pub hotkey_status: String,
    pub hide_before_apply: bool,
    pub holding: bool,
    pub topology_fingerprint: String,
    pub exiting: bool,
    pub dismiss: bool,
    pub announce_panel_shown: bool,
}

impl SessionState {
    pub(crate) fn new(detail: String, hotkey_status: String) -> Self {
        let now = Instant::now();
        Self {
            pending: Vec::new(),
            operation_started: now,
            stage_started: now,
            stage: String::new(),
            auto_stage: false,
            cancel_requested: false,
            last_operation_error: None,
            worker_failed: false,
            confirmation: None,
            has_session: false,
            screens: Vec::new(),
            auxiliary: AuxiliaryInstallItem::from_availability(false),
            detail,
            hotkey_status,
            hide_before_apply: false,
            holding: false,
            topology_fingerprint: String::new(),
            exiting: false,
            dismiss: false,
            announce_panel_shown: false,
        }
    }

    pub(crate) fn note_worker_disconnected(&mut self) -> PanelEffect {
        if self.worker_failed {
            return PanelEffect::Stay;
        }
        self.worker_failed = true;
        self.pending.clear();
        self.stage.clear();
        self.detail = "显示操作线程已退出，当前恢复状态未知。请使用仍可用的紧急热键，并保留日志。"
            .into();
        PanelEffect::Present
    }

    pub(crate) fn apply_event(&mut self, event: Event) -> PanelEffect {
        match event {
            Event::Fault(error) => {
                self.detail = error;
                self.screens.clear();
                self.auto_stage = false;
                PanelEffect::Present
            }
            Event::View(view) => self.apply_view(view),
            Event::Phase(phase) => {
                if self.pending.is_empty() {
                    self.operation_started = Instant::now();
                }
                self.stage = phase;
                self.stage_started = Instant::now();
                self.auto_stage = self.pending.is_empty();
                PanelEffect::Stay
            }
            Event::Confirm { reason, reply } => {
                self.stage = "等待你确认是否启用显示驱动".into();
                self.stage_started = Instant::now();
                self.confirmation = Some((reason, reply));
                PanelEffect::Present
            }
            Event::Done { id, action, error } => self.apply_done(id, action, error),
            Event::ShowPanel => {
                self.announce_panel_shown = true;
                PanelEffect::Present
            }
            Event::ExitReady => {
                self.exiting = true;
                PanelEffect::Exit
            }
        }
    }

    fn apply_view(&mut self, view: View) -> PanelEffect {
        let fingerprint = view.topology_fingerprint.clone();
        self.screens = view.screens;
        self.auxiliary = view.auxiliary;
        self.hotkey_status = view.hotkey_status;
        self.holding = view.holding;
        self.has_session = view.has_session;
        if self.pending.is_empty() {
            if let Some(detail) = view.detail.filter(|text| !text.is_empty()) {
                self.detail = match &self.last_operation_error {
                    Some(error) if !detail.contains(error.as_str()) => {
                        format!("{error} 当前状态：{detail}")
                    }
                    _ => detail,
                };
            }
        }
        let effect = self.note_topology(&fingerprint);
        self.auto_stage = false;
        effect
    }

    fn note_topology(&mut self, fingerprint: &str) -> PanelEffect {
        if self.topology_fingerprint.is_empty() {
            self.topology_fingerprint = fingerprint.to_owned();
            return PanelEffect::Stay;
        }
        if fingerprint == self.topology_fingerprint {
            return PanelEffect::Stay;
        }
        self.topology_fingerprint = fingerprint.to_owned();
        if self.hide_before_apply || self.has_session {
            return PanelEffect::Stay;
        }
        if self.holding {
            self.detail = "显示拓扑已变化，保持关闭已结束。".into();
            self.holding = false;
            return PanelEffect::Present;
        }
        PanelEffect::Stay
    }

    fn apply_done(&mut self, id: u64, action: Action, error: Option<String>) -> PanelEffect {
        self.pending.retain(|(pending_id, _)| *pending_id != id);
        let effect = if let Some(error) = error {
            self.last_operation_error = Some(error.clone());
            self.detail = error;
            PanelEffect::Present
        } else {
            if matches!(action, Action::Cancel | Action::RestoreAll | Action::Exit) {
                self.last_operation_error = None;
            }
            match action {
                Action::KeepOff if !self.cancel_requested && self.pending.is_empty() => {
                    self.hide_before_apply = true;
                    PanelEffect::Dismiss
                }
                Action::RestoreAll => {
                    self.detail = "恢复全部已确认完成。".into();
                    PanelEffect::Stay
                }
                Action::Cancel => {
                    self.detail = "取消已处理；恢复结果已确认。".into();
                    PanelEffect::Stay
                }
                Action::Install => {
                    self.detail = "辅助虚拟输出已安装。".into();
                    PanelEffect::Stay
                }
                _ => PanelEffect::Stay,
            }
        };
        if self.pending.is_empty() {
            self.stage.clear();
            self.cancel_requested = false;
        }
        effect
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use veil_engine::ScreenIdentity;

    fn fresh() -> SessionState {
        SessionState::new(DEFAULT_DETAIL.into(), "热键：未知".into())
    }

    fn view(fingerprint: &str, holding: bool, has_session: bool) -> View {
        View {
            screens: vec![ScreenItem {
                identity: ScreenIdentity::new("luid", 1, r"\\.\DISPLAY1"),
                name: "内置屏".into(),
                kind: "internal".into(),
                wanted: if holding { "保持关闭" } else { "开启" }.into(),
                confirmed: "已显示".into(),
                can_keep_off: !holding,
                can_restore: holding,
                status_text: String::new(),
                block_reason: String::new(),
            }],
            auxiliary: AuxiliaryInstallItem::from_availability(false),
            hotkey_status: "热键：空闲".into(),
            detail: None,
            holding,
            topology_fingerprint: fingerprint.into(),
            has_session,
        }
    }

    #[test]
    fn dismiss_keeps_the_single_instance_mutex() {
        assert!(!should_release_single_instance(SessionStop::Dismiss));
        assert!(should_release_single_instance(SessionStop::Exit));
        assert_eq!(after_ui_session(SessionStop::Dismiss), LoopNext::Idle);
        assert_eq!(
            after_ui_session(SessionStop::Exit),
            LoopNext::ExitProcess
        );
    }

    #[test]
    fn successful_keep_off_dismisses_without_exiting() {
        let mut state = fresh();
        state.pending.push((7, Action::KeepOff));
        let effect = state.apply_event(Event::Done {
            id: 7,
            action: Action::KeepOff,
            error: None,
        });
        assert_eq!(effect, PanelEffect::Dismiss);
        assert!(state.hide_before_apply);
        assert!(!state.exiting);
        assert!(state.pending.is_empty());
    }

    #[test]
    fn cancelled_keep_off_does_not_dismiss() {
        let mut state = fresh();
        state.cancel_requested = true;
        state.pending.push((7, Action::KeepOff));
        let effect = state.apply_event(Event::Done {
            id: 7,
            action: Action::KeepOff,
            error: None,
        });
        assert_eq!(effect, PanelEffect::Stay);
        assert!(!state.hide_before_apply);
    }

    #[test]
    fn confirm_failure_and_show_panel_present_the_window() {
        let mut state = fresh();
        let (reply, _received) = mpsc::channel();
        assert_eq!(
            state.apply_event(Event::Confirm {
                reason: "需要辅助输出".into(),
                reply,
            }),
            PanelEffect::Present
        );
        assert!(state.confirmation.is_some());

        let mut state = fresh();
        assert_eq!(
            state.apply_event(Event::Done {
                id: 1,
                action: Action::RestoreOne,
                error: Some("恢复失败".into()),
            }),
            PanelEffect::Present
        );
        assert_eq!(
            state.apply_event(Event::Fault("无法枚举显示器".into())),
            PanelEffect::Present
        );
        assert_eq!(state.apply_event(Event::ShowPanel), PanelEffect::Present);
        assert!(state.announce_panel_shown);
    }

    #[test]
    fn topology_change_while_holding_presents_unless_suppressed() {
        let mut state = fresh();
        assert_eq!(
            state.apply_event(Event::View(view("a", true, false))),
            PanelEffect::Stay
        );
        assert_eq!(
            state.apply_event(Event::View(view("b", true, false))),
            PanelEffect::Present
        );
        assert!(!state.holding);

        let mut suppressed = fresh();
        suppressed.hide_before_apply = true;
        assert_eq!(
            suppressed.apply_event(Event::View(view("a", true, false))),
            PanelEffect::Stay
        );
        assert_eq!(
            suppressed.apply_event(Event::View(view("b", true, false))),
            PanelEffect::Stay
        );

        let mut session = fresh();
        assert_eq!(
            session.apply_event(Event::View(view("a", true, true))),
            PanelEffect::Stay
        );
        assert_eq!(
            session.apply_event(Event::View(view("b", true, true))),
            PanelEffect::Stay
        );
    }

    #[test]
    fn plain_view_and_phase_do_not_reopen() {
        let mut state = fresh();
        assert_eq!(
            state.apply_event(Event::View(view("a", false, false))),
            PanelEffect::Stay
        );
        assert_eq!(
            state.apply_event(Event::View(view("a", false, false))),
            PanelEffect::Stay
        );
        assert_eq!(
            state.apply_event(Event::Phase("正在枚举".into())),
            PanelEffect::Stay
        );
        assert_eq!(idle_response(PanelEffect::Stay), IdleAction::Wait);
        assert_eq!(idle_response(PanelEffect::Dismiss), IdleAction::Wait);
        assert_eq!(idle_response(PanelEffect::Present), IdleAction::Present);
    }

    #[test]
    fn worker_disconnect_presents_once() {
        let mut state = fresh();
        assert_eq!(state.note_worker_disconnected(), PanelEffect::Present);
        assert!(state.worker_failed);
        assert_eq!(state.note_worker_disconnected(), PanelEffect::Stay);
    }

    #[test]
    fn tray_restore_and_exit_are_replayed_after_the_window_returns() {
        let (present, deferred) = classify_tray_commands(&[
            TrayCommand::Open,
            TrayCommand::RestoreAll,
            TrayCommand::Exit,
            TrayCommand::RestoreAll,
        ]);
        assert!(present);
        assert_eq!(
            deferred,
            vec![TrayCommand::RestoreAll, TrayCommand::Exit]
        );
        let (present, deferred) = classify_tray_commands(&[TrayCommand::Open]);
        assert!(present);
        assert!(deferred.is_empty());
    }
}
