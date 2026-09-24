use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use veil_engine::{
    AuxiliaryInstallItem, BundledVddAvailability, CcdApi, CcdConstants, DriverStatus,
    RecoveryCoordinator, RecoveryCoordinatorHooks, ScreenIdentity, ScreenItem, ScreenListBuilder,
    TopologyBlob, Win32CcdApi, Win32ParentWatcher,
};

use crate::{app_log, helper_operation_unfinished, run_helper_elevated};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Action {
    KeepOff,
    RestoreOne,
    RestoreAll,
    Install,
    Exit,
    Cancel,
    PanelShown,
}

struct Command {
    id: u64,
    action: Action,
    target: Option<ScreenIdentity>,
}

pub(crate) struct View {
    pub screens: Vec<ScreenItem>,
    pub auxiliary: AuxiliaryInstallItem,
    pub hotkey_status: String,
    pub detail: Option<String>,
    pub holding: bool,
    pub topology_fingerprint: String,
    pub has_session: bool,
}

pub(crate) enum Event {
    View(View),
    Fault(String),
    Phase(String),
    Confirm {
        reason: String,
        reply: Sender<bool>,
    },
    Done {
        id: u64,
        action: Action,
        error: Option<String>,
    },
    ShowPanel,
    ExitReady,
}

pub(crate) struct OperationWorker {
    commands: Sender<Command>,
    pub events: Receiver<Event>,
    cancel_requested: Arc<AtomicBool>,
    next_id: u64,
}

impl OperationWorker {
    pub fn start(ctx: egui::Context) -> Self {
        let (commands, incoming) = mpsc::channel::<Command>();
        let (outgoing, events) = mpsc::channel();
        let cancel_requested = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&cancel_requested);
        std::thread::Builder::new()
            .name("veil-display-operations".into())
            .spawn(move || run(incoming, outgoing, flag, ctx))
            .expect("无法启动显示操作线程");
        Self {
            commands,
            events,
            cancel_requested,
            next_id: 1,
        }
    }

    pub fn send(&mut self, action: Action, target: Option<ScreenIdentity>) -> Result<u64, String> {
        if action == Action::KeepOff {
            self.cancel_requested.store(false, Ordering::SeqCst);
        }
        if matches!(action, Action::Cancel | Action::RestoreAll | Action::Exit) {
            self.cancel_requested.store(true, Ordering::SeqCst);
        }
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        self.commands
            .send(Command { id, action, target })
            .map_err(|_| "显示操作线程已退出，无法确认显示状态。".to_string())?;
        Ok(id)
    }
}

fn emit(out: &Sender<Event>, ctx: &egui::Context, event: Event) {
    let _ = out.send(event);
    ctx.request_repaint();
}

fn run(
    incoming: Receiver<Command>,
    outgoing: Sender<Event>,
    cancel: Arc<AtomicBool>,
    ctx: egui::Context,
) {
    let current_id = Arc::new(AtomicU64::new(0));
    let stage_clock: Arc<Mutex<Option<(String, Instant)>>> = Arc::new(Mutex::new(None));
    let confirm_out = outgoing.clone();
    let confirm_ctx = ctx.clone();
    let progress_out = outgoing.clone();
    let progress_ctx = ctx.clone();
    let progress_id = Arc::clone(&current_id);
    let helper_id = Arc::clone(&current_id);
    let progress_clock = Arc::clone(&stage_clock);
    let cancel_for_hook = Arc::clone(&cancel);
    let mut hooks = RecoveryCoordinatorHooks::production();
    hooks.confirm_enable_vdd = Some(Box::new(move |reason| {
        let (reply, received) = mpsc::channel();
        emit(
            &confirm_out,
            &confirm_ctx,
            Event::Confirm {
                reason: reason.to_string(),
                reply,
            },
        );
        received.recv().unwrap_or(false)
    }));
    hooks.run_driver_helper = Box::new(move |verb| {
        let rc = run_helper_elevated(verb);
        app_log(&format!(
            "helper-result id={} verb={verb} rc={rc}",
            helper_id.load(Ordering::SeqCst)
        ));
        rc
    });
    hooks.is_alive = Box::new(|pid| {
        veil_engine::ParentWatcher::is_alive(&Win32ParentWatcher, pid).unwrap_or(false)
    });
    hooks.on_progress = Box::new(move |phase| {
        let id = progress_id.load(Ordering::SeqCst);
        let mut clock = progress_clock.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((previous, started)) = clock.replace((phase.to_string(), Instant::now())) {
            app_log(&format!(
                "operation-stage-end id={id} stage={previous} elapsed_ms={}",
                started.elapsed().as_millis()
            ));
        }
        app_log(&format!("operation-stage id={id} {phase}"));
        emit(
            &progress_out,
            &progress_ctx,
            Event::Phase(phase.to_string()),
        );
    });
    hooks.cancel_requested = Box::new(move || cancel_for_hook.load(Ordering::SeqCst));
    let mut coordinator = RecoveryCoordinator::new(Box::new(Win32CcdApi), hooks);
    send_view(&mut coordinator, &outgoing, &ctx);
    let mut cancel_outcome: Option<Option<String>> = None;
    loop {
        match incoming.recv_timeout(Duration::from_millis(400)) {
            Ok(command) => {
                if command.action == Action::PanelShown {
                    coordinator.panel_show_attempt_finished();
                    continue;
                }
                let action = command.action;
                let operation_started = Instant::now();
                current_id.store(command.id, Ordering::SeqCst);
                app_log(&format!(
                    "operation-start id={} action={action:?}",
                    command.id
                ));
                let error = if matches!(action, Action::KeepOff | Action::Install | Action::Exit)
                    && helper_operation_unfinished()
                {
                    Some("上次辅助程序仍在运行，设备状态未确认；请等待其结束后重试。".into())
                } else {
                    match action {
                        Action::KeepOff => {
                            let mut result = command
                                .target
                                .map(|id| coordinator.keep_off(id))
                                .unwrap_or_else(|| Some("缺少目标物理屏。".into()));
                            let restore_requested =
                                result.is_some() && coordinator.restore_request_pending();
                            let recovery_error = if restore_requested {
                                coordinator
                                    .wait_for_restore_completion(Duration::from_secs(20))
                                    .err()
                                    .map(|e| e.to_string())
                            } else {
                                None
                            };
                            if let Some(error) = &mut result {
                                if let Some(restore) = &recovery_error {
                                    *error =
                                        format!("{error} 关屏请求未提交；恢复未确认：{restore}");
                                } else if restore_requested {
                                    error.push_str(" 关屏请求未提交；恢复和清理已确认。");
                                } else {
                                    error.push_str(" 关屏请求未提交。");
                                }
                            }
                            cancel_outcome = if cancel.load(Ordering::SeqCst) && result.is_some() {
                                Some(recovery_error)
                            } else {
                                None
                            };
                            result
                        }
                        Action::RestoreOne => command
                            .target
                            .as_ref()
                            .map(|id| {
                                coordinator.restore_one(id).or_else(|| {
                                    coordinator
                                        .wait_for_single_restore_confirmation(
                                            id,
                                            Duration::from_secs(20),
                                        )
                                        .err()
                                        .map(|e| e.to_string())
                                })
                            })
                            .unwrap_or_else(|| Some("缺少目标物理屏。".into())),
                        Action::RestoreAll => {
                            let result = coordinator.restore_all_and_wait(Duration::from_secs(20));
                            cancel_outcome = None;
                            cancel.store(false, Ordering::SeqCst);
                            result.err().map(|e| e.to_string())
                        }
                        Action::Cancel => {
                            let result = cancel_outcome.take().unwrap_or_else(|| {
                                coordinator
                                    .restore_all_and_wait(Duration::from_secs(20))
                                    .err()
                                    .map(|e| e.to_string())
                            });
                            cancel.store(false, Ordering::SeqCst);
                            result
                        }
                        Action::Install => coordinator.install_auxiliary_output(),
                        Action::Exit => {
                            let result = coordinator.restore_all_and_wait(Duration::from_secs(20));
                            cancel_outcome = None;
                            cancel.store(false, Ordering::SeqCst);
                            result.err().map(|e| e.to_string())
                        }
                        Action::PanelShown => unreachable!(),
                    }
                };
                finish_stage(&stage_clock, command.id);
                app_log(&format!(
                    "operation-end id={} action={action:?} elapsed_ms={} error={error:?}",
                    command.id,
                    operation_started.elapsed().as_millis()
                ));
                current_id.store(0, Ordering::SeqCst);
                emit(
                    &outgoing,
                    &ctx,
                    Event::Done {
                        id: command.id,
                        action,
                        error: error.clone(),
                    },
                );
                if action == Action::Exit && error.is_none() {
                    emit(&outgoing, &ctx, Event::ExitReady);
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
        send_view(&mut coordinator, &outgoing, &ctx);
        if current_id.load(Ordering::SeqCst) == 0 {
            finish_stage(&stage_clock, 0);
        }
    }
}

fn finish_stage(clock: &Mutex<Option<(String, Instant)>>, id: u64) {
    let previous = clock.lock().unwrap_or_else(|e| e.into_inner()).take();
    if let Some((stage, started)) = previous {
        app_log(&format!(
            "operation-stage-end id={id} stage={stage} elapsed_ms={}",
            started.elapsed().as_millis()
        ));
    }
}

fn send_view(coordinator: &mut RecoveryCoordinator, outgoing: &Sender<Event>, ctx: &egui::Context) {
    coordinator.poll();
    if coordinator.take_should_show_panel() {
        emit(outgoing, ctx, Event::ShowPanel);
    }
    let frame = match Win32CcdApi.capture(CcdConstants::QUERY_FLAGS) {
        Ok(frame) => frame,
        Err(error) => {
            app_log(&format!("display-refresh-error {error}"));
            emit(
                outgoing,
                ctx,
                Event::Fault(format!("无法枚举显示器：{error}")),
            );
            return;
        }
    };
    let bundled = BundledVddAvailability::from_flags(
        DriverStatus::installed(),
        DriverStatus::payload_present(),
    );
    let mut screens = ScreenListBuilder::build(
        &frame.snapshot,
        coordinator.heartbeat.as_ref(),
        &coordinator.wanted(),
        bundled,
        coordinator.is_ready || !coordinator.has_session(),
        coordinator.hotkey_registered || !coordinator.has_session(),
    );
    if let Some(reason) = coordinator.recovery_block_reason() {
        for screen in &mut screens {
            screen.can_keep_off = false;
            screen.block_reason = reason.into();
        }
    }
    let holding = screens
        .iter()
        .any(|s| s.wanted == "保持关闭" || s.confirmed == "处理中");
    let hotkey_status = if coordinator.hotkey_registered {
        format!("{}：可用", CcdConstants::HOTKEY_TEXT)
    } else if !coordinator.has_session() {
        format!("{}：空闲，下次关屏前启用", CcdConstants::HOTKEY_TEXT)
    } else {
        format!("{}：不可用", CcdConstants::HOTKEY_TEXT)
    };
    emit(
        outgoing,
        ctx,
        Event::View(View {
            screens,
            auxiliary: AuxiliaryInstallItem::from_availability(bundled),
            hotkey_status,
            detail: coordinator.status_text.clone(),
            holding,
            topology_fingerprint: TopologyBlob::fingerprint(&frame.paths, &frame.modes),
            has_session: coordinator.has_session(),
        }),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn cancel_signal_and_command_do_not_wait_for_a_stalled_operation() {
        let (commands, incoming) = mpsc::channel::<Command>();
        let (_events_out, events) = mpsc::channel();
        let (entered_out, entered) = mpsc::channel();
        let (release_out, release) = mpsc::channel();
        let backend = std::thread::spawn(move || {
            let first = incoming.recv().unwrap();
            entered_out.send(()).unwrap();
            release.recv().unwrap();
            let second = incoming.recv().unwrap();
            (first.action, second.action)
        });
        let flag = Arc::new(AtomicBool::new(false));
        let mut client = OperationWorker {
            commands,
            events,
            cancel_requested: flag.clone(),
            next_id: 1,
        };
        client.send(Action::KeepOff, None).unwrap();
        entered.recv_timeout(Duration::from_secs(1)).unwrap();
        let started = Instant::now();
        client.send(Action::Cancel, None).unwrap();
        assert!(started.elapsed() < Duration::from_millis(100));
        assert!(flag.load(Ordering::SeqCst));
        release_out.send(()).unwrap();
        assert_eq!(backend.join().unwrap(), (Action::KeepOff, Action::Cancel));
    }
}
