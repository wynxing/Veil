use crate::fakes::{SharedClock, SharedParent};
use crate::session::{HeartbeatFile, JsonUtil, SessionPaths};
use crate::session::{RecoveryState, RestoreState};
use crate::*;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use super::support::*;

#[test]
fn owned_vdd_is_preserved_without_verified_physical_output_for_every_reason() {
    for reason in ["release", "hotkey", "parent-exit", "error", "recovery-exit"] {
        let (mut coordinator, ccd, dir, calls) = owned_coordinator(0);
        ccd.deactivate_path(0);
        let mut result = complete_result();
        result.reason = reason.into();
        JsonUtil::write_atomic(SessionPaths::result(dir.borrow().as_str()), &result).unwrap();
        coordinator.poll();
        assert!(coordinator.has_session());
        assert!(!calls.borrow().iter().any(|v| v == "disable"));
        assert!(coordinator.restore_all_and_wait(Duration::ZERO).is_err());
        std::fs::remove_dir_all(dir.borrow().as_str()).unwrap();
    }
}
#[test]
fn owned_vdd_cleanup_enumeration_failure_is_fail_closed() {
    let (mut coordinator, ccd, dir, calls) = owned_coordinator(0);
    JsonUtil::write_atomic(
        SessionPaths::result(dir.borrow().as_str()),
        &complete_result(),
    )
    .unwrap();
    ccd.set_capture_error("unavailable");
    coordinator.poll();
    assert!(coordinator.has_session());
    assert!(!calls.borrow().iter().any(|v| v == "disable"));
    std::fs::remove_dir_all(dir.borrow().as_str()).unwrap();
}
#[test]
fn failed_cleanup_does_not_loop_uac_and_blocks_repeated_exit() {
    let (mut coordinator, _, dir, calls) = owned_coordinator(1);
    JsonUtil::write_atomic(
        SessionPaths::result(dir.borrow().as_str()),
        &complete_result(),
    )
    .unwrap();
    for _ in 0..10 {
        coordinator.poll();
    }
    assert_eq!(calls.borrow().iter().filter(|v| *v == "disable").count(), 1);
    assert!(coordinator.has_session());
    assert!(coordinator.restore_all_and_wait(Duration::ZERO).is_err());
    assert!(coordinator.restore_all_and_wait(Duration::ZERO).is_err());
    std::fs::remove_dir_all(dir.borrow().as_str()).unwrap();
}
#[test]
fn interrupt_cleanup_failure_notifies_once_and_preserves_error_across_polls() {
    for rc in [1, 1223, 1460] {
        let (mut coordinator, _, dir, calls) = owned_coordinator(rc);
        let mut result = complete_result();
        result.reason = "suspend-resume".into();
        JsonUtil::write_atomic(SessionPaths::result(dir.borrow().as_str()), &result).unwrap();
        coordinator.poll();
        assert!(coordinator.take_should_show_panel());
        let error = coordinator.status_text.clone();
        assert!(error.as_ref().unwrap().contains("辅助虚拟输出"));
        for _ in 0..100 {
            coordinator.poll();
            assert!(!coordinator.take_should_show_panel());
            assert_eq!(coordinator.status_text, error);
        }
        assert_eq!(calls.borrow().iter().filter(|v| *v == "disable").count(), 1);
        assert!(coordinator.has_session());
        std::fs::remove_dir_all(dir.borrow().as_str()).unwrap();
    }
}
#[test]
fn interrupt_cleanup_enumeration_failure_notifies_once() {
    let (mut coordinator, ccd, dir, calls) = owned_coordinator(0);
    let mut result = complete_result();
    result.reason = "suspend-resume".into();
    JsonUtil::write_atomic(SessionPaths::result(dir.borrow().as_str()), &result).unwrap();
    ccd.set_capture_error("unavailable");
    coordinator.poll();
    assert!(coordinator.take_should_show_panel());
    let error = coordinator.status_text.clone();
    for _ in 0..100 {
        coordinator.poll();
        assert!(!coordinator.take_should_show_panel());
        assert_eq!(coordinator.status_text, error);
    }
    assert!(!calls.borrow().iter().any(|v| v == "disable"));
    std::fs::remove_dir_all(dir.borrow().as_str()).unwrap();
}
#[test]
fn restore_failed_heartbeat_notifies_once_without_final_result() {
    let (mut coordinator, _, dir, _) = owned_coordinator(0);
    JsonUtil::write_atomic(
        SessionPaths::heartbeat(dir.borrow().as_str()),
        &HeartbeatFile {
            state: RecoveryState::RestoreFailed,
            armed: true,
            hotkey_registered: true,
            detail: Some("恢复失败，请恢复全部".into()),
            ..Default::default()
        },
    )
    .unwrap();
    coordinator.poll();
    assert!(coordinator.take_should_show_panel());
    for _ in 0..100 {
        coordinator.poll();
        assert!(!coordinator.take_should_show_panel());
        assert!(coordinator.hotkey_registered);
    }
    assert!(!SessionPaths::result(dir.borrow().as_str()).exists());
    std::fs::remove_dir_all(dir.borrow().as_str()).unwrap();
}
#[test]
fn cleanup_only_retry_does_not_publish_release_or_replay_old_result() {
    let (mut coordinator, ccd, dir, calls) = owned_coordinator(1223);
    let mut result = complete_result();
    result.reason = "suspend-resume".into();
    JsonUtil::write_atomic(SessionPaths::result(dir.borrow().as_str()), &result).unwrap();
    coordinator.poll();
    assert!(coordinator.take_should_show_panel());
    assert!(coordinator.keep_off(ccd.rows()[0].identity()).is_some());
    assert!(coordinator.restore_all().is_some());
    assert!(!SessionPaths::release(dir.borrow().as_str()).exists());
    assert!(SessionPaths::result(dir.borrow().as_str()).exists());
    assert_eq!(calls.borrow().iter().filter(|v| *v == "disable").count(), 2);
    for _ in 0..100 {
        coordinator.poll();
        assert!(!coordinator.take_should_show_panel());
    }
    std::fs::remove_dir_all(dir.borrow().as_str()).unwrap();
}
#[test]
fn successful_cleanup_retry_allows_a_fresh_session_and_notification() {
    let (mut coordinator, ccd, dir, calls) = owned_coordinator_sequence(vec![1223, 0]);
    let old = dir.borrow().clone();
    let mut result = complete_result();
    result.reason = "suspend-resume".into();
    JsonUtil::write_atomic(SessionPaths::result(&old), &result).unwrap();
    coordinator.poll();
    assert!(coordinator.take_should_show_panel());
    assert!(coordinator.recovery_block_reason().is_some());
    assert!(coordinator.restore_all().is_none());
    assert!(!coordinator.has_session());
    assert!(coordinator.recovery_block_reason().is_none());
    assert!(!SessionPaths::release(&old).exists());
    assert_eq!(calls.borrow().iter().filter(|v| *v == "disable").count(), 2);
    assert!(coordinator.keep_off(ccd.rows()[0].identity()).is_none());
    assert_ne!(&old, &*dir.borrow());
    JsonUtil::write_atomic(SessionPaths::result(dir.borrow().as_str()), &result).unwrap();
    coordinator.poll();
    assert!(coordinator.take_should_show_panel());
    coordinator.poll();
    assert!(!coordinator.take_should_show_panel());
    std::fs::remove_dir_all(old).unwrap();
    std::fs::remove_dir_all(dir.borrow().as_str()).unwrap();
}
#[test]
fn lost_physical_output_requires_recovery_before_cleanup_retry() {
    let (mut coordinator, ccd, dir, calls) = owned_coordinator(1223);
    let mut result = complete_result();
    result.reason = "suspend-resume".into();
    JsonUtil::write_atomic(SessionPaths::result(dir.borrow().as_str()), &result).unwrap();
    coordinator.poll();
    assert!(coordinator.take_should_show_panel());
    ccd.deactivate_path(0);
    assert!(coordinator.restore_all().is_none());
    assert!(SessionPaths::release(dir.borrow().as_str()).exists());
    assert!(!SessionPaths::result(dir.borrow().as_str()).exists());
    assert_eq!(calls.borrow().iter().filter(|v| *v == "disable").count(), 1);
    JsonUtil::write_atomic(
        SessionPaths::heartbeat(dir.borrow().as_str()),
        &HeartbeatFile {
            state: RecoveryState::RestoreFailed,
            processed_request_id: u64::MAX,
            armed: true,
            hotkey_registered: true,
            detail: Some("恢复仍未完成".into()),
            ..Default::default()
        },
    )
    .unwrap();
    coordinator.poll();
    assert!(coordinator.take_should_show_panel());
    for _ in 0..100 {
        coordinator.poll();
        assert!(!coordinator.take_should_show_panel());
    }
    std::fs::remove_dir_all(dir.borrow().as_str()).unwrap();
}
#[test]
fn coordinator_retains_pre_enable_baseline() {
    let (_, _, dir, _) = owned_coordinator(0);
    let (baseline, _) = TopologyBlob::load(SessionPaths::baseline(dir.borrow().as_str())).unwrap();
    let (handshake, _) = TopologyBlob::load(SessionPaths::topology(dir.borrow().as_str())).unwrap();
    assert_eq!(PathOps::active_targets(&baseline).len(), 1);
    assert_eq!(PathOps::active_targets(&handshake).len(), 2);
    std::fs::remove_dir_all(dir.borrow().as_str()).unwrap();
}
#[test]
fn failed_intent_write_does_not_commit_local_wanted() {
    let started = Rc::new(RefCell::new(String::new()));
    let ccd = dual_physical();
    let mut hooks = coord_hooks(
        started.clone(),
        Rc::new(RefCell::new(vec![])),
        false,
        true,
        None,
        Duration::ZERO,
    );
    let original = std::mem::replace(&mut hooks.start_recovery, Box::new(|_, _| 0));
    let mut original = original;
    hooks.start_recovery = Box::new(move |dir, parent| {
        let pid = original(dir, parent);
        std::fs::create_dir(SessionPaths::intent(dir)).unwrap();
        pid
    });
    let mut coordinator = RecoveryCoordinator::new(Box::new(ccd.clone()), hooks);
    assert!(coordinator.keep_off(ccd.rows()[0].identity()).is_some());
    assert!(coordinator.wanted().is_empty());
    assert!(SessionPaths::release(started.borrow().as_str()).exists());
    std::fs::remove_dir_all(started.borrow().as_str()).unwrap();
}
#[test]
fn failed_release_write_is_reported_instead_of_success() {
    let started = Rc::new(RefCell::new(String::new()));
    let ccd = dual_physical();
    let mut coordinator = RecoveryCoordinator::new(
        Box::new(ccd.clone()),
        coord_hooks(
            started.clone(),
            Rc::new(RefCell::new(vec![])),
            false,
            true,
            None,
            Duration::ZERO,
        ),
    );
    assert!(coordinator.keep_off(ccd.rows()[0].identity()).is_none());
    std::fs::create_dir(SessionPaths::release(started.borrow().as_str())).unwrap();
    assert!(coordinator.restore_all().is_some());
    assert!(coordinator.restore_all_and_wait(Duration::ZERO).is_err());
    assert!(coordinator.has_session());
    std::fs::remove_dir_all(started.borrow().as_str()).unwrap();
}
#[test]
fn partial_apply_failure_rolls_back_instead_of_assuming_no_effect() {
    let dir = TempSession::new();
    let ccd = dual_physical();
    save_topology(&dir.0, &ccd);
    ccd.mutate_on_failure(true);
    ccd.set_next_apply_rc(31);
    let session = arm_with_intent(
        &dir.0,
        ccd.clone(),
        Rc::new(SharedParent::new()),
        Rc::new(SharedClock::new(0.0)),
        false,
    );
    assert!(session.exited);
    assert_eq!(session.result.restore_state, RestoreState::Complete);
    assert!(ccd.rows().iter().all(|p| p.active));
}
