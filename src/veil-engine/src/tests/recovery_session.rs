use crate::fakes::{self, FakeHotkey, FakePower, SharedClock, SharedParent};
use crate::native::{CcdApi, CcdConstants, PowerEvent};
use crate::session::{
    ArmFile, HeartbeatFile, IntentFile, JsonUtil, OpenSessionRelease, ReleaseFile, ResultFile,
    SessionPaths,
};
use crate::session::{RestoreState, SessionMetadata, PROTOCOL_VERSION};
use crate::*;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use super::support::*;

#[test]
fn hotkey_failure_never_writes_ready_or_applies() {
    let dir = TempSession::new();
    let ccd = dual_physical();
    save_topology(&dir.0, &ccd);
    let mut hotkey = FakeHotkey::new();
    hotkey.register_success = false;
    let mut session = RecoverySession::new(options(
        dir.0.clone(),
        ccd.clone(),
        hotkey,
        11,
        Rc::new(SharedParent::new()),
        Rc::new(SharedClock::new(0.0)),
    ));
    session.start();
    assert!(session.exited);
    assert!(!SessionPaths::ready(&dir.0).exists());
    assert!(!ccd.applied());
    assert_eq!(session.result.reason, "error");
}
#[test]
fn release_before_arm_exits_without_apply() {
    let dir = TempSession::new();
    let ccd = dual_physical();
    save_topology(&dir.0, &ccd);
    let mut session = RecoverySession::new(options(
        dir.0.clone(),
        ccd.clone(),
        FakeHotkey::new(),
        11,
        Rc::new(SharedParent::new()),
        Rc::new(SharedClock::new(0.0)),
    ));
    session.start();
    JsonUtil::write_atomic(
        SessionPaths::release(&dir.0),
        &ReleaseFile {
            at: 1.0,
            request_id: crate::session::request_id(),
        },
    )
    .unwrap();
    session.tick();
    assert!(session.exited);
    assert!(!ccd.applied());
    assert_eq!(session.result.reason, "release");
    assert!(!session.result.ok);
    assert!(session
        .result
        .error
        .as_deref()
        .unwrap_or("")
        .contains("未改物理屏"));
    assert!(SessionPaths::result(&dir.0).exists());
}
#[test]
fn start_exception_writes_result() {
    let dir = TempSession::new();
    let ccd = dual_physical();
    save_topology(&dir.0, &ccd);
    ccd.set_capture_error("ccd-start");
    let mut session = RecoverySession::new(options(
        dir.0.clone(),
        ccd.clone(),
        FakeHotkey::new(),
        11,
        Rc::new(SharedParent::new()),
        Rc::new(SharedClock::new(0.0)),
    ));
    session.start();
    assert!(session.exited);
    assert!(!ccd.applied());
    assert_eq!(session.result.reason, "error");
    assert_eq!(session.result.error.as_deref(), Some("ccd-start"));
}
#[test]
fn tick_exception_writes_result() {
    let dir = TempSession::new();
    let ccd = dual_physical();
    save_topology(&dir.0, &ccd);
    let parent = Rc::new(SharedParent::new());
    let mut session = RecoverySession::new(options(
        dir.0.clone(),
        ccd,
        FakeHotkey::new(),
        11,
        parent.clone(),
        Rc::new(SharedClock::new(0.0)),
    ));
    session.start();
    JsonUtil::write_atomic(SessionPaths::arm(&dir.0), &ArmFile { pid: 11 }).unwrap();
    session.tick();
    *parent.error.borrow_mut() = Some("parent-fault".into());
    session.tick();
    assert!(session.exited);
    assert_eq!(session.result.reason, "error");
    assert_eq!(session.result.error.as_deref(), Some("parent-fault"));
}
#[test]
fn arm_pid_mismatch_never_applies() {
    let dir = TempSession::new();
    let ccd = dual_physical();
    save_topology(&dir.0, &ccd);
    let mut session = RecoverySession::new(options(
        dir.0.clone(),
        ccd.clone(),
        FakeHotkey::new(),
        11,
        Rc::new(SharedParent::new()),
        Rc::new(SharedClock::new(0.0)),
    ));
    session.start();
    JsonUtil::write_atomic(SessionPaths::arm(&dir.0), &ArmFile { pid: 99 }).unwrap();
    session.tick();
    assert!(session.exited);
    assert!(!ccd.applied());
    assert!(session
        .result
        .error
        .as_deref()
        .unwrap_or("")
        .contains("arm PID"));
}
#[test]
fn apply_happens_only_after_matching_arm_and_intent() {
    let dir = TempSession::new();
    let ccd = dual_physical();
    save_topology(&dir.0, &ccd);
    let mut session = RecoverySession::new(options(
        dir.0.clone(),
        ccd.clone(),
        FakeHotkey::new(),
        11,
        Rc::new(SharedParent::new()),
        Rc::new(SharedClock::new(0.0)),
    ));
    session.start();
    assert!(SessionPaths::ready(&dir.0).exists());
    assert!(!ccd.applied());
    JsonUtil::write_atomic(SessionPaths::arm(&dir.0), &ArmFile { pid: 11 }).unwrap();
    session.tick();
    assert!(!ccd.applied());
    write_keep_internal_off(&dir.0, false);
    session.tick();
    assert!(ccd.validated());
    assert!(ccd.applied());
    assert!(ccd.flags().contains(&CcdConstants::APPLY_FLAGS));
    assert!(!ccd
        .flags()
        .iter()
        .any(|f| f & CcdConstants::SDC_SAVE_TO_DATABASE != 0));
    let heartbeat: HeartbeatFile = JsonUtil::read(SessionPaths::heartbeat(&dir.0)).unwrap();
    assert!(heartbeat
        .screens
        .iter()
        .any(|s| s.confirmed == ConfirmedState::Off));
    assert!(!heartbeat.screens.iter().any(|s| {
        s.wanted == WantedState::KeepOff
            && s.confirmed == ConfirmedState::Off
            && s.detail.contains("失败")
    }));
}
#[test]
fn failed_validate_does_not_retry_and_restores_all() {
    let dir = TempSession::new();
    let ccd = dual_physical();
    ccd.set_validate_rc(87);
    save_topology(&dir.0, &ccd);
    let mut session = arm_with_intent(
        &dir.0,
        ccd.clone(),
        Rc::new(SharedParent::new()),
        Rc::new(SharedClock::new(0.0)),
        false,
    );
    assert!(session.exited);
    assert_eq!(session.result.restore_state, RestoreState::Complete);
    assert!(ccd.rows().iter().all(|p| p.active));
    let flags = ccd.flags();
    for _ in 0..20 {
        session.tick();
    }
    assert_eq!(flags, ccd.flags());
    assert_eq!(
        flags
            .iter()
            .filter(|f| **f == CcdConstants::APPLY_FLAGS)
            .count(),
        1,
        "only restoration may APPLY"
    );
}
#[test]
fn apply_failure_restores() {
    let dir = TempSession::new();
    let ccd = dual_physical();
    ccd.set_next_apply_rc(31);
    save_topology(&dir.0, &ccd);
    let session = arm_with_intent(
        &dir.0,
        ccd.clone(),
        Rc::new(SharedParent::new()),
        Rc::new(SharedClock::new(0.0)),
        false,
    );
    assert!(ccd.applied());
    assert_eq!(session.result.restore_rc, Some(0));
    assert!(session.exited);
    assert_eq!(session.result.restore_state, RestoreState::Complete);
}
#[test]
fn parent_exit_restores_and_writes_result() {
    let dir = TempSession::new();
    let ccd = dual_physical();
    save_topology(&dir.0, &ccd);
    let parent = Rc::new(SharedParent::new());
    let mut session = arm_with_intent(
        &dir.0,
        ccd,
        parent.clone(),
        Rc::new(SharedClock::new(0.0)),
        false,
    );
    *parent.alive.borrow_mut() = false;
    session.tick();
    assert!(session.exited);
    assert_eq!(session.result.reason, "parent-exit");
    assert_eq!(session.result.restore_rc, Some(0));
}
#[test]
fn topology_churn_while_physical_still_off_does_not_burn_reapply() {
    let dir = TempSession::new();
    let ccd = dual_physical();
    save_topology(&dir.0, &ccd);
    let mut session = arm_with_intent(
        &dir.0,
        ccd.clone(),
        Rc::new(SharedParent::new()),
        Rc::new(SharedClock::new(0.0)),
        false,
    );
    assert!(!session.exited);
    ccd.update_path_target(1, 99);
    session.tick();
    assert!(!session.exited);
    assert!(!session.result.reapply_attempted);
    let events = std::fs::read_to_string(SessionPaths::events(&dir.0)).unwrap();
    assert!(events.contains("topology-settle"));
    assert!(!events.contains("\"type\":\"interrupt\""));
}
#[test]
fn execution_gap_restores_without_reapply() {
    let dir = TempSession::new();
    let ccd = dual_physical();
    save_topology(&dir.0, &ccd);
    let clock = Rc::new(SharedClock::new(0.0));
    let mut session = arm_with_intent(
        &dir.0,
        ccd.clone(),
        Rc::new(SharedParent::new()),
        clock.clone(),
        false,
    );
    let apply_count = ccd
        .flags()
        .iter()
        .filter(|f| **f == CcdConstants::APPLY_FLAGS)
        .count();
    clock.set(10.0);
    session.tick();
    assert!(session.exited);
    assert!(!session.result.reapply_attempted);
    assert_eq!(session.result.reason, "execution-gap");
    assert!(ccd.rows().iter().all(|p| p.active));
    let later = ccd
        .flags()
        .iter()
        .filter(|f| **f == CcdConstants::APPLY_FLAGS)
        .count();
    assert_eq!(later, apply_count + 1, "中断只应回放一次基线");
    let events = std::fs::read_to_string(SessionPaths::events(&dir.0)).unwrap();
    assert!(!events.contains("reapply-attempt"));
}
#[test]
fn slow_apply_is_not_an_execution_gap() {
    let dir = TempSession::new();
    let ccd = dual_physical();
    save_topology(&dir.0, &ccd);
    let clock = Rc::new(SharedClock::new(0.0));
    let jump = clock.clone();
    ccd.set_after_apply(move |_| {
        let now = *jump.seconds.borrow();
        jump.set(now + 5.0);
    });
    let mut session = RecoverySession::new(options(
        dir.0.clone(),
        ccd.clone(),
        FakeHotkey::new(),
        11,
        Rc::new(SharedParent::new()),
        clock,
    ));
    session.start();
    JsonUtil::write_atomic(SessionPaths::arm(&dir.0), &ArmFile { pid: 11 }).unwrap();
    session.tick();
    write_keep_internal_off(&dir.0, false);
    session.tick();
    assert!(!session.exited, "关屏耗时超过间隔阈值时不应回放基线");
    assert_ne!(session.result.reason, "execution-gap");
    session.tick();
    assert!(!session.exited);
    assert_ne!(session.result.reason, "execution-gap");
}
#[test]
fn suspend_ignores_topology_churn_until_resume() {
    let dir = TempSession::new();
    let ccd = dual_physical();
    save_topology(&dir.0, &ccd);
    let power = Rc::new(FakePower::new());
    let mut opt = options(
        dir.0.clone(),
        ccd.clone(),
        FakeHotkey::new(),
        11,
        Rc::new(SharedParent::new()),
        Rc::new(SharedClock::new(0.0)),
    );
    opt.power = Box::new(power.clone());
    let mut session = RecoverySession::new(opt);
    session.start();
    JsonUtil::write_atomic(SessionPaths::arm(&dir.0), &ArmFile { pid: 11 }).unwrap();
    session.tick();
    write_keep_internal_off(&dir.0, false);
    session.tick();
    assert!(!session.exited);
    power.push(PowerEvent::Suspending);
    session.tick();
    ccd.activate_path(0);
    session.tick();
    assert!(!session.exited);
    assert!(!session.result.reapply_attempted);
    let events = std::fs::read_to_string(SessionPaths::events(&dir.0)).unwrap();
    assert!(events.contains("power-suspend"));
    assert!(!events.contains("\"type\":\"interrupt\""));
    power.push(PowerEvent::Resumed);
    session.tick();
    assert!(session.exited);
    assert_eq!(session.result.reason, "suspend-resume");
    assert!(!session.result.reapply_attempted);
    assert!(ccd.rows().iter().all(|p| p.active));
}
#[test]
fn power_resume_restores_without_reapply() {
    let dir = TempSession::new();
    let ccd = dual_physical();
    save_topology(&dir.0, &ccd);
    let power = Rc::new(FakePower::new());
    let mut opt = options(
        dir.0.clone(),
        ccd.clone(),
        FakeHotkey::new(),
        11,
        Rc::new(SharedParent::new()),
        Rc::new(SharedClock::new(0.0)),
    );
    opt.power = Box::new(power.clone());
    let mut session = RecoverySession::new(opt);
    session.start();
    JsonUtil::write_atomic(SessionPaths::arm(&dir.0), &ArmFile { pid: 11 }).unwrap();
    session.tick();
    write_keep_internal_off(&dir.0, false);
    session.tick();
    assert!(!ccd.rows()[0].active);
    let before = ccd.flags();
    for state in [0, 2, 1] {
        power.push(PowerEvent::DisplayState(state));
        session.tick();
        assert!(!session.exited);
        assert_eq!(ccd.flags(), before);
    }
    power.push(PowerEvent::Resumed);
    session.tick();
    assert!(session.exited);
    assert_eq!(session.result.reason, "suspend-resume");
    assert!(!session.result.reapply_attempted);
    assert_eq!(session.result.restore_state, RestoreState::Complete);
    assert!(ccd.rows().iter().all(|p| p.active));
    let restored_flags = ccd.flags();
    for _ in 0..10 {
        power.push(PowerEvent::Resumed);
        session.tick();
    }
    assert_eq!(ccd.flags(), restored_flags);
    let events = std::fs::read_to_string(SessionPaths::events(&dir.0)).unwrap();
    assert!(events.contains("power-resume"));
    assert!(!events.contains("reapply-attempt"));
}
#[test]
fn failed_resume_restore_does_not_loop_apply() {
    let dir = TempSession::new();
    let ccd = dual_physical();
    save_topology(&dir.0, &ccd);
    let power = Rc::new(FakePower::new());
    let mut opt = options(
        dir.0.clone(),
        ccd.clone(),
        FakeHotkey::new(),
        11,
        Rc::new(SharedParent::new()),
        Rc::new(SharedClock::new(0.0)),
    );
    opt.power = Box::new(power.clone());
    let mut session = RecoverySession::new(opt);
    session.start();
    JsonUtil::write_atomic(SessionPaths::arm(&dir.0), &ArmFile { pid: 11 }).unwrap();
    session.tick();
    write_keep_internal_off(&dir.0, false);
    session.tick();
    ccd.set_capture_error("unavailable");
    power.push(PowerEvent::Resumed);
    session.tick();
    let flags = ccd.flags();
    for _ in 0..10 {
        session.tick();
    }
    assert_eq!(flags, ccd.flags());
    assert!(!session.result.reapply_attempted);
}
#[test]
fn stale_capture_after_restore_still_reapplies_on_topology() {
    let dir = TempSession::new();
    let ccd = dual_physical();
    save_topology(&dir.0, &ccd);
    let mut session = arm_with_intent(
        &dir.0,
        ccd.clone(),
        Rc::new(SharedParent::new()),
        Rc::new(SharedClock::new(0.0)),
        false,
    );
    let apply_count = ccd
        .flags()
        .iter()
        .filter(|f| **f == CcdConstants::APPLY_FLAGS)
        .count();
    ccd.activate_path(0);
    session.tick();
    assert!(session.result.reapply_attempted);
    assert!(
        ccd.flags()
            .iter()
            .filter(|f| **f == CcdConstants::APPLY_FLAGS)
            .count()
            > apply_count
    );
    let events = std::fs::read_to_string(SessionPaths::events(&dir.0)).unwrap();
    assert!(events.contains("reapply-settle"));
    assert!(events.contains("reapplied") || events.contains("reapply-attempt"));
}
#[test]
fn reapply_without_second_target_requests_bundled_vdd() {
    let dir = TempSession::new();
    let ccd = internal_plus_vdd();
    save_topology(&dir.0, &ccd);
    let mut session = arm_with_intent(
        &dir.0,
        ccd.clone(),
        Rc::new(SharedParent::new()),
        Rc::new(SharedClock::new(0.0)),
        true,
    );
    ccd.set_after_apply(|inner| {
        inner.paths.truncate(1);
        inner.rows.truncate(1);
        inner.rows[0].active = true;
    });
    ccd.activate_path(0);
    ccd.update_path_target(1, 99);
    session.tick();
    assert!(session.result.reapply_attempted);
    assert!(!session.exited);
    assert!(SessionPaths::vdd_request(&dir.0).exists());
    let events = std::fs::read_to_string(SessionPaths::events(&dir.0)).unwrap();
    assert!(events.contains("vdd-request"));
    ccd.clear_after_apply();
    let fresh = internal_plus_vdd();
    ccd.set_paths_rows(fresh.paths(), fresh.rows());
    session.tick();
    assert!(!session.exited);
    assert!(std::fs::read_to_string(SessionPaths::events(&dir.0))
        .unwrap()
        .contains("reapplied"));
}
#[test]
fn shrinking_intent_reactivates_restored_path_and_keeps_session() {
    let dir = TempSession::new();
    let ccd = three_physical();
    save_topology(&dir.0, &ccd);
    let mut session = RecoverySession::new(options(
        dir.0.clone(),
        ccd.clone(),
        FakeHotkey::new(),
        11,
        Rc::new(SharedParent::new()),
        Rc::new(SharedClock::new(0.0)),
    ));
    session.start();
    JsonUtil::write_atomic(SessionPaths::arm(&dir.0), &ArmFile { pid: 11 }).unwrap();
    session.tick();
    write_keep_off(
        &dir.0,
        &[(1, r"\\?\DISPLAY#CMN#1"), (2, r"\\?\DISPLAY#PDA#1")],
        false,
    );
    session.tick();
    assert!(!session.exited);
    assert!(!ccd.rows()[0].active);
    assert!(!ccd.rows()[1].active);
    assert!(ccd.rows()[2].active);
    write_keep_off(&dir.0, &[(2, r"\\?\DISPLAY#PDA#1")], false);
    session.tick();
    assert!(!session.exited);
    assert!(ccd.rows()[0].active);
    assert!(!ccd.rows()[1].active);
    assert!(ccd.rows()[2].active);
    let events = std::fs::read_to_string(SessionPaths::events(&dir.0)).unwrap();
    assert!(events.contains("partial-restored"));
    let heartbeat: HeartbeatFile = JsonUtil::read(SessionPaths::heartbeat(&dir.0)).unwrap();
    assert!(heartbeat.screens.iter().any(|s| {
        s.target_id == 1 && s.wanted == WantedState::On && s.confirmed == ConfirmedState::On
    }));
    assert!(heartbeat.screens.iter().any(|s| {
        s.target_id == 2 && s.wanted == WantedState::KeepOff && s.confirmed == ConfirmedState::Off
    }));
}
#[test]
fn hotkey_press_finishes_and_restores_saved() {
    let dir = TempSession::new();
    let ccd = dual_physical();
    save_topology(&dir.0, &ccd);
    let hotkey = Rc::new(RefCell::new(FakeHotkey::new()));
    let mut session = RecoverySession::new(options_ex(
        dir.0.clone(),
        ccd.clone(),
        Box::new(fakes::RcHotkey(hotkey.clone())),
        11,
        Rc::new(SharedParent::new()),
        Rc::new(SharedClock::new(0.0)),
        20.0,
    ));
    session.start();
    JsonUtil::write_atomic(SessionPaths::arm(&dir.0), &ArmFile { pid: 11 }).unwrap();
    session.tick();
    write_keep_internal_off(&dir.0, false);
    session.tick();
    assert!(!ccd.rows()[0].active);
    hotkey.borrow_mut().pressed = true;
    session.tick();
    assert!(session.exited);
    assert_eq!(session.result.reason, "hotkey");
    assert!(ccd.rows()[0].active);
    assert!(ccd.rows()[1].active);
}
#[test]
fn unexpected_topology_with_selected_lit_reapplies_once_then_finishes() {
    let dir = TempSession::new();
    let ccd = dual_physical();
    save_topology(&dir.0, &ccd);
    let mut session = arm_with_intent(
        &dir.0,
        ccd.clone(),
        Rc::new(SharedParent::new()),
        Rc::new(SharedClock::new(0.0)),
        false,
    );
    assert!(!session.exited);
    ccd.activate_path(0);
    ccd.update_path_target(1, 99);
    ccd.set_validate_rc(87);
    session.tick();
    assert!(session.exited);
    assert_eq!(session.result.reason, "unexpected-topology");
    assert!(session.result.reapply_attempted);
    let events = std::fs::read_to_string(SessionPaths::events(&dir.0)).unwrap();
    assert!(events.contains("\"type\":\"interrupt\""));
    assert!(events.contains("reapply-attempt"));
}
#[test]
fn vdd_wait_timeout_finishes_without_looping_apply() {
    let dir = TempSession::new();
    let ccd = internal_plus_vdd();
    save_topology(&dir.0, &ccd);
    let clock = Rc::new(SharedClock::new(0.0));
    let mut session = RecoverySession::new(options_ex(
        dir.0.clone(),
        ccd.clone(),
        Box::new(FakeHotkey::new()),
        11,
        Rc::new(SharedParent::new()),
        clock.clone(),
        1.0,
    ));
    session.start();
    JsonUtil::write_atomic(SessionPaths::arm(&dir.0), &ArmFile { pid: 11 }).unwrap();
    session.tick();
    write_keep_internal_off(&dir.0, true);
    session.tick();
    ccd.set_after_apply(|inner| {
        inner.paths.truncate(1);
        inner.rows.truncate(1);
        inner.rows[0].active = true;
    });
    ccd.activate_path(0);
    ccd.update_path_target(1, 99);
    session.tick();
    assert!(session.result.reapply_attempted);
    assert!(!session.exited);
    assert!(SessionPaths::vdd_request(&dir.0).exists());
    let apply_count = ccd
        .flags()
        .iter()
        .filter(|f| **f == CcdConstants::APPLY_FLAGS)
        .count();
    clock.set(12.0);
    session.tick();
    assert!(session.exited);
    assert_eq!(session.result.reason, "unexpected-topology");
    let later = ccd
        .flags()
        .iter()
        .filter(|f| **f == CcdConstants::APPLY_FLAGS)
        .count();
    assert!(
        later <= apply_count + 1,
        "timeout may restore once, must not loop APPLY"
    );
    let events = std::fs::read_to_string(SessionPaths::events(&dir.0)).unwrap();
    assert!(!events.contains("reapplied"));
    assert!(events.contains("等待辅助虚拟输出超时"));
}
#[test]
fn request_all_and_wait_returns_when_result_appears() {
    let root = TempSession::new();
    let session_dir = root.0.join("session-wait");
    std::fs::create_dir_all(&session_dir).unwrap();
    JsonUtil::write_atomic(
        SessionPaths::metadata(&session_dir),
        &SessionMetadata {
            protocol_version: PROTOCOL_VERSION,
            ..Default::default()
        },
    )
    .unwrap();
    write_live_ready(&session_dir);
    let result = SessionPaths::result(&session_dir);
    let writer = session_dir.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(40));
        JsonUtil::write_atomic(
            SessionPaths::result(&writer),
            &ResultFile {
                protocol_version: PROTOCOL_VERSION,
                restore_state: RestoreState::Complete,
                ok: true,
                reason: "release".into(),
                ..Default::default()
            },
        )
        .unwrap();
    });
    OpenSessionRelease::wait_after_release(&root.0, Duration::from_secs(2)).unwrap();
    assert!(SessionPaths::release(&session_dir).exists());
    assert!(result.exists());
}
#[test]
fn restore_one_writes_shrunk_intent_when_targets_already_off() {
    let started = Rc::new(RefCell::new(String::new()));
    let helper = Rc::new(RefCell::new(Vec::new()));
    let ccd = three_physical();
    let mut coordinator = RecoveryCoordinator::new(
        Box::new(ccd.clone()),
        coord_hooks(
            started.clone(),
            helper,
            false,
            true,
            None,
            Duration::from_secs(15),
        ),
    );
    let snap = ccd.query_snapshot(CcdConstants::QUERY_FLAGS).unwrap();
    let ids: Vec<_> = snap.physical_screens().map(|r| r.identity()).collect();
    assert!(coordinator.keep_off(ids[0].clone()).is_none());
    assert!(coordinator.keep_off(ids[1].clone()).is_none());
    ccd.deactivate_path(0);
    ccd.deactivate_path(1);
    assert!(coordinator.restore_one(&ids[0]).is_none());
    let wanted = coordinator.wanted();
    assert_eq!(wanted.len(), 1);
    assert!(wanted[0].matches(&ids[1]));
    let intent: IntentFile =
        JsonUtil::read(SessionPaths::intent(started.borrow().as_str())).unwrap();
    assert_eq!(intent.keep_off.len(), 1);
    assert_eq!(intent.keep_off[0].target_id, ids[1].target_id);
    JsonUtil::write_atomic(
        SessionPaths::heartbeat(started.borrow().as_str()),
        &HeartbeatFile {
            processed_request_id: intent.request_id,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(coordinator
        .wait_for_single_restore_confirmation(&ids[0], Duration::from_millis(1))
        .is_err());
    let _ = std::fs::remove_dir_all(started.borrow().as_str());
}
#[test]
fn single_restore_wait_accepts_acknowledged_active_target() {
    let started = Rc::new(RefCell::new(String::new()));
    let ccd = three_physical();
    let mut coordinator = RecoveryCoordinator::new(
        Box::new(ccd.clone()),
        coord_hooks(
            started.clone(),
            Rc::new(RefCell::new(Vec::new())),
            false,
            true,
            None,
            Duration::from_secs(15),
        ),
    );
    let snap = ccd.query_snapshot(CcdConstants::QUERY_FLAGS).unwrap();
    let ids: Vec<_> = snap.physical_screens().map(|r| r.identity()).collect();
    assert!(coordinator.keep_off(ids[0].clone()).is_none());
    assert!(coordinator.keep_off(ids[1].clone()).is_none());
    ccd.deactivate_path(0);
    ccd.deactivate_path(1);
    assert!(coordinator.restore_one(&ids[0]).is_none());
    let intent: IntentFile =
        JsonUtil::read(SessionPaths::intent(started.borrow().as_str())).unwrap();
    JsonUtil::write_atomic(
        SessionPaths::heartbeat(started.borrow().as_str()),
        &HeartbeatFile {
            processed_request_id: intent.request_id,
            ..Default::default()
        },
    )
    .unwrap();
    ccd.activate_path(0);
    assert!(coordinator
        .wait_for_single_restore_confirmation(&ids[0], Duration::from_secs(1))
        .is_ok());
    let _ = std::fs::remove_dir_all(started.borrow().as_str());
}
#[test]
fn request_all_and_wait_returns_immediately_without_sessions() {
    let root = TempSession::new();
    OpenSessionRelease::wait_after_release(&root.0, Duration::from_secs(2)).unwrap();
}
#[test]
fn dual_physical_never_uses_clone_flags() {
    let dir = TempSession::new();
    let ccd = dual_physical();
    save_topology(&dir.0, &ccd);
    let _ = arm_with_intent(
        &dir.0,
        ccd.clone(),
        Rc::new(SharedParent::new()),
        Rc::new(SharedClock::new(0.0)),
        false,
    );
    assert!(!ccd
        .flags()
        .iter()
        .any(|f| f & CcdConstants::SDC_TOPOLOGY_CLONE != 0));
}
