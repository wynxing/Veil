use crate::fakes::{self, FakeCcd, FakeHotkey, FakePower, RcHotkey, SharedClock, SharedParent};
use crate::native::{
    CcdAbi, CcdApi, CcdConstants, DisplayConfigModeInfo, DisplayConfigPathInfo,
    DisplayConfigPathSourceInfo, DisplayConfigPathTargetInfo, DisplayConfigSourceMode,
    DisplayConfigVideoSignalInfo, Hotkey, Luid, PointL, PowerEvent,
};
use crate::session::{
    ArmFile, HeartbeatFile, HeartbeatScreen, IntentFile, JsonUtil, OpenSessionRelease, ReadyFile,
    ReleaseFile, ResultFile, ScreenIdentityDto, SessionPaths, VddRequestFile,
};
use crate::session::{RecoveryState, RestoreState, SessionMetadata, PROTOCOL_VERSION};
use crate::*;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use super::support::*;

#[test]
fn format_result_maps_release() {
    assert_eq!(
        RecoveryCoordinator::format_result(Some(&ResultFile {
            protocol_version: PROTOCOL_VERSION,
            restore_state: RestoreState::Complete,
            ok: true,
            reason: "release".into(),
            ..Default::default()
        })),
        "已恢复全部。"
    );
}
#[test]
fn format_result_maps_recovery_exit() {
    assert_eq!(
        RecoveryCoordinator::format_result(Some(&ResultFile {
            protocol_version: PROTOCOL_VERSION,
            ok: false,
            reason: RecoveryCoordinator::RECOVERY_EXIT_REASON.into(),
            error: Some(RecoveryCoordinator::RECOVERY_EXITED.into()),
            ..Default::default()
        })),
        RecoveryCoordinator::RECOVERY_EXITED
    );
}
#[test]
fn format_result_maps_hotkey() {
    assert_eq!(
        RecoveryCoordinator::format_result(Some(&ResultFile {
            protocol_version: PROTOCOL_VERSION,
            restore_state: RestoreState::Complete,
            ok: true,
            reason: "hotkey".into(),
            ..Default::default()
        })),
        "已由 Ctrl+Alt+Shift+F10 恢复。"
    );
}
#[test]
fn format_result_maps_unexpected_topology_after_reapply() {
    assert_eq!(
        RecoveryCoordinator::format_result(Some(&ResultFile {
            protocol_version: PROTOCOL_VERSION,
            restore_state: RestoreState::Complete,
            ok: false,
            reason: "unexpected-topology".into(),
            apply_rc: Some(0),
            restore_rc: Some(0),
            restored_topology: true,
            restored_targets: true,
            reapply_attempted: true,
            ..Default::default()
        })),
        "显示拓扑已变化，保持关闭已结束。 已尝试再关一次。"
    );
}
#[test]
fn format_result_maps_suspend_resume() {
    assert_eq!(
        RecoveryCoordinator::format_result(Some(&ResultFile {
            protocol_version: PROTOCOL_VERSION,
            restore_state: RestoreState::Complete,
            ok: false,
            reason: "suspend-resume".into(),
            restore_rc: Some(0),
            restored_topology: true,
            restored_targets: true,
            ..Default::default()
        })),
        "系统休眠或待机后已恢复显示，保持关闭已结束。"
    );
}
#[test]
fn poll_interrupt_result_asks_to_show_panel() {
    let started = Rc::new(RefCell::new(String::new()));
    let helper = Rc::new(RefCell::new(Vec::new()));
    let ccd = dual_physical();
    let mut coordinator = RecoveryCoordinator::new(
        Box::new(ccd.clone()),
        coord_hooks(
            started.clone(),
            helper,
            false,
            false,
            None,
            Duration::from_secs(15),
        ),
    );
    let identity = ccd
        .query_snapshot(CcdConstants::QUERY_FLAGS)
        .unwrap()
        .physical_screens()
        .into_iter()
        .next()
        .unwrap()
        .identity();
    assert!(coordinator.keep_off(identity).is_none());
    JsonUtil::write_atomic(
        SessionPaths::result(started.borrow().as_str()),
        &ResultFile {
            protocol_version: PROTOCOL_VERSION,
            restore_state: RestoreState::Complete,
            ok: false,
            reason: "suspend-resume".into(),
            restore_rc: Some(0),
            restored_topology: true,
            restored_targets: true,
            ..Default::default()
        },
    )
    .unwrap();
    coordinator.poll();
    assert!(coordinator.take_should_show_panel());
    assert!(!coordinator.take_should_show_panel());
    assert!(coordinator
        .status_text
        .as_deref()
        .unwrap()
        .starts_with("系统休眠或待机后已恢复显示，保持关闭已结束。"));
    let _ = std::fs::remove_dir_all(started.borrow().as_str());
}
#[test]
fn poll_enables_bundled_vdd_once_when_reapply_requests_it() {
    let started = Rc::new(RefCell::new(String::new()));
    let helper = Rc::new(RefCell::new(Vec::new()));
    let ccd = internal_plus_vdd();
    let mut coordinator = RecoveryCoordinator::new(
        Box::new(ccd.clone()),
        coord_hooks(
            started.clone(),
            helper.clone(),
            true,
            true,
            None,
            Duration::from_secs(15),
        ),
    );
    let identity = ccd
        .query_snapshot(CcdConstants::QUERY_FLAGS)
        .unwrap()
        .physical_screens()
        .find(|r| r.role == PathRole::Internal)
        .unwrap()
        .identity();
    assert!(coordinator.keep_off(identity).is_none());
    helper.borrow_mut().clear();
    JsonUtil::write_atomic(
        SessionPaths::vdd_request(started.borrow().as_str()),
        &VddRequestFile {
            at: 1.0,
            reason: "reapply".into(),
        },
    )
    .unwrap();
    coordinator.poll();
    coordinator.poll();
    assert_eq!(*helper.borrow(), vec!["enable".to_string()]);
    let _ = std::fs::remove_dir_all(started.borrow().as_str());
}
#[test]
fn session_result_clears_keep_off_heartbeat() {
    let started = Rc::new(RefCell::new(String::new()));
    let helper = Rc::new(RefCell::new(Vec::new()));
    let ccd = dual_physical();
    let mut coordinator = RecoveryCoordinator::new(
        Box::new(ccd.clone()),
        coord_hooks(
            started.clone(),
            helper.clone(),
            false,
            true,
            None,
            Duration::from_secs(15),
        ),
    );
    let identity = ccd
        .query_snapshot(CcdConstants::QUERY_FLAGS)
        .unwrap()
        .physical_screens()
        .find(|r| r.role == PathRole::Internal)
        .unwrap()
        .identity();
    assert!(coordinator.keep_off(identity.clone()).is_none());
    assert!(coordinator.has_session());
    assert!(!coordinator.restore_request_pending());
    JsonUtil::write_atomic(
        SessionPaths::heartbeat(started.borrow().as_str()),
        &HeartbeatFile {
            state: Default::default(),
            processed_request_id: 0,
            hotkey_registered: true,
            armed: true,
            detail: Some("已保持关闭。".into()),
            screens: vec![HeartbeatScreen {
                adapter_luid: identity.adapter_luid.clone(),
                target_id: identity.target_id,
                monitor_path: identity.monitor_path.clone(),
                name: "Panel".into(),
                wanted: "保持关闭".into(),
                confirmed: "已关闭".into(),
                detail: "已保持关闭。".into(),
                ..Default::default()
            }],
        },
    )
    .unwrap();
    JsonUtil::write_atomic(
        SessionPaths::result(started.borrow().as_str()),
        &ResultFile {
            protocol_version: PROTOCOL_VERSION,
            restore_state: RestoreState::Complete,
            ok: true,
            reason: "release".into(),
            restore_rc: Some(0),
            restored_topology: true,
            ..Default::default()
        },
    )
    .unwrap();
    coordinator.poll();
    assert!(!coordinator.has_session());
    assert!(coordinator.heartbeat.is_none());
    assert!(coordinator.wanted().is_empty());
    assert!(coordinator
        .status_text
        .as_deref()
        .unwrap()
        .starts_with("已恢复全部。"));
    assert!(coordinator
        .status_text
        .as_deref()
        .unwrap()
        .contains("记录："));
    assert!(!coordinator.hotkey_registered);
    assert!(!helper.borrow().iter().any(|v| v == "disable"));
    let _ = std::fs::remove_dir_all(started.borrow().as_str());
}
#[test]
fn missing_metadata_with_live_recovery_waits_for_explicit_result() {
    let started = Rc::new(RefCell::new(String::new()));
    let launches = Rc::new(RefCell::new(0u32));
    let started_hook = started.clone();
    let launches_hook = launches.clone();
    let hooks = RecoveryCoordinatorHooks {
        start_recovery: Box::new(move |dir, _| {
            *launches_hook.borrow_mut() += 1;
            *started_hook.borrow_mut() = dir.to_string();
            JsonUtil::write_atomic(
                SessionPaths::ready(dir),
                &ReadyFile {
                    pid: 4242,
                    hotkey_registered: true,
                    hotkey: CcdConstants::HOTKEY_TEXT.into(),
                },
            )
            .unwrap();
            4242
        }),
        run_driver_helper: Box::new(|_| 0),
        confirm_enable_vdd: None,
        bundled_vdd_installed: Box::new(|| false),
        bundled_vdd_payload: Box::new(|| false),
        is_alive: Box::new(|_| true),
        virtual_path_wait: Duration::from_secs(15),
        on_progress: Box::new(|_| {}),
        cancel_requested: Box::new(|| false),
    };
    let ccd = dual_physical();
    let mut coordinator = RecoveryCoordinator::new(Box::new(ccd.clone()), hooks);
    let identity = ccd
        .query_snapshot(CcdConstants::QUERY_FLAGS)
        .unwrap()
        .physical_screens()
        .find(|row| row.role == PathRole::Internal)
        .unwrap()
        .identity();
    assert!(coordinator.keep_off(identity.clone()).is_none());
    let dir = started.borrow().clone();
    let original = ccd.capture(CcdConstants::QUERY_FLAGS).unwrap();
    ccd.deactivate_path(0);
    let before = *launches.borrow();
    assert!(before >= 1);
    std::fs::remove_file(SessionPaths::metadata(&dir)).unwrap();
    assert!(coordinator.restore_all_and_wait(Duration::ZERO).is_err());
    assert!(coordinator.keep_off(identity).is_some());
    assert_eq!(*launches.borrow(), before);
    assert!(coordinator.has_session());
    assert!(SessionPaths::release(&dir).exists());
    assert!(SessionPaths::baseline(&dir).exists());
    assert!(!ccd.rows()[0].active);
    ccd.set(&original.paths, &original.modes, CcdConstants::APPLY_FLAGS)
        .unwrap();
    JsonUtil::write_atomic(SessionPaths::result(&dir), &complete_result()).unwrap();
    coordinator.poll();
    assert!(!coordinator.has_session());
    assert!(coordinator.restore_all_and_wait(Duration::ZERO).is_ok());
    let _ = std::fs::remove_dir_all(dir);
}
#[test]
fn missing_metadata_with_failed_restore_keeps_owned_vdd() {
    let (mut coordinator, ccd, dir, calls) = owned_coordinator(0);
    let session_dir = dir.borrow().clone();
    ccd.deactivate_path(0);
    std::fs::remove_file(SessionPaths::metadata(&session_dir)).unwrap();
    assert!(coordinator.restore_all().is_none());
    assert!(SessionPaths::release(&session_dir).exists());
    JsonUtil::write_atomic(
        SessionPaths::result(&session_dir),
        &ResultFile {
            protocol_version: PROTOCOL_VERSION,
            restore_state: RestoreState::Partial,
            reason: "release".into(),
            error: Some("测试恢复失败".into()),
            ..Default::default()
        },
    )
    .unwrap();
    coordinator.poll();
    assert!(coordinator.recovery_block_reason().is_some());
    assert!(coordinator.has_session());
    assert!(!calls.borrow().iter().any(|verb| verb == "disable"));
    assert!(!ccd.rows()[0].active);
    assert!(coordinator.keep_off(ccd.rows()[0].identity()).is_some());
    std::fs::remove_dir_all(session_dir).unwrap();
}
#[test]
fn missing_metadata_release_write_failure_blocks_new_close() {
    use std::os::windows::fs::OpenOptionsExt;

    let (mut coordinator, ccd, dir, calls) = owned_coordinator(0);
    let session_dir = dir.borrow().clone();
    ccd.deactivate_path(0);
    std::fs::remove_file(SessionPaths::metadata(&session_dir)).unwrap();
    let release = SessionPaths::release(&session_dir);
    std::fs::write(&release, b"pending").unwrap();
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(&release)
        .unwrap();
    assert!(coordinator
        .restore_all()
        .unwrap()
        .contains("恢复请求写入失败"));
    assert!(coordinator.recovery_block_reason().is_some());
    assert!(coordinator.has_session());
    assert!(!calls.borrow().iter().any(|verb| verb == "disable"));
    assert!(!ccd.rows()[0].active);
    assert!(coordinator.keep_off(ccd.rows()[0].identity()).is_some());
    drop(lock);
    std::fs::remove_dir_all(session_dir).unwrap();
}
#[test]
fn missing_metadata_with_dead_recovery_rebuilds_from_saved_baseline() {
    let started = Rc::new(RefCell::new(String::new()));
    let launches = Rc::new(RefCell::new(0u32));
    let ccd = dual_physical();
    let ccd_for_recovery = ccd.clone();
    let started_for_hook = started.clone();
    let launches_for_hook = launches.clone();
    let hooks = RecoveryCoordinatorHooks {
        start_recovery: Box::new(move |dir, parent| {
            *launches_for_hook.borrow_mut() += 1;
            *started_for_hook.borrow_mut() = dir.to_string();
            if parent.is_none() {
                let meta: SessionMetadata = JsonUtil::read(SessionPaths::metadata(dir)).unwrap();
                assert_eq!(meta.protocol_version, PROTOCOL_VERSION);
                let (paths, modes) = TopologyBlob::load(SessionPaths::baseline(dir)).unwrap();
                ccd_for_recovery
                    .set(&paths, &modes, CcdConstants::APPLY_FLAGS)
                    .unwrap();
                JsonUtil::write_atomic(SessionPaths::result(dir), &complete_result()).unwrap();
            } else {
                JsonUtil::write_atomic(
                    SessionPaths::ready(dir),
                    &ReadyFile {
                        pid: 4242,
                        hotkey_registered: true,
                        hotkey: CcdConstants::HOTKEY_TEXT.into(),
                    },
                )
                .unwrap();
            }
            4242
        }),
        run_driver_helper: Box::new(|_| 0),
        confirm_enable_vdd: None,
        bundled_vdd_installed: Box::new(|| false),
        bundled_vdd_payload: Box::new(|| false),
        is_alive: Box::new(|_| false),
        virtual_path_wait: Duration::from_secs(15),
        on_progress: Box::new(|_| {}),
        cancel_requested: Box::new(|| false),
    };
    let mut coordinator = RecoveryCoordinator::new(Box::new(ccd.clone()), hooks);
    let identity = ccd.rows()[0].identity();
    assert!(coordinator.keep_off(identity).is_none());
    let dir = started.borrow().clone();
    ccd.deactivate_path(0);
    std::fs::remove_file(SessionPaths::metadata(&dir)).unwrap();
    assert!(coordinator
        .restore_all_and_wait(Duration::from_millis(100))
        .is_ok());
    assert_eq!(*launches.borrow(), 2);
    assert!(ccd.rows().iter().all(|row| row.active));
    assert!(!coordinator.has_session());
    let _ = std::fs::remove_dir_all(dir);
}
#[test]
fn missing_metadata_rebuild_failure_keeps_recovery_and_owned_vdd() {
    use std::os::windows::fs::OpenOptionsExt;

    let started = Rc::new(RefCell::new(String::new()));
    let helper = Rc::new(RefCell::new(Vec::new()));
    let ccd = dual_physical();
    let mut coordinator = RecoveryCoordinator::new(
        Box::new(ccd.clone()),
        coord_hooks(
            started.clone(),
            helper.clone(),
            false,
            false,
            None,
            Duration::from_secs(15),
        ),
    );
    let identity = ccd.rows()[0].identity();
    assert!(coordinator.keep_off(identity.clone()).is_none());
    let dir = started.borrow().clone();
    ccd.deactivate_path(0);
    coordinator.mark_vdd_owned_for_test();
    std::fs::remove_file(SessionPaths::metadata(&dir)).unwrap();
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(SessionPaths::baseline(&dir))
        .unwrap();
    let error = coordinator.restore_all().unwrap();
    assert!(error.contains("元数据"));
    assert!(coordinator.has_session());
    assert!(coordinator.vdd_owned_for_test());
    assert!(!ccd.rows()[0].active);
    assert!(!helper.borrow().iter().any(|verb| verb == "disable"));
    assert!(coordinator.keep_off(identity).is_some());
    drop(lock);
    let _ = std::fs::remove_dir_all(dir);
}
#[test]
fn missing_metadata_without_trusted_baseline_fails_closed() {
    let started = Rc::new(RefCell::new(String::new()));
    let helper = Rc::new(RefCell::new(Vec::new()));
    let ccd = dual_physical();
    let mut coordinator = RecoveryCoordinator::new(
        Box::new(ccd.clone()),
        coord_hooks(
            started.clone(),
            helper.clone(),
            false,
            false,
            None,
            Duration::from_secs(15),
        ),
    );
    assert!(coordinator.keep_off(ccd.rows()[0].identity()).is_none());
    let dir = started.borrow().clone();
    ccd.deactivate_path(0);
    coordinator.forget_baseline_for_test();
    coordinator.mark_vdd_owned_for_test();
    std::fs::remove_file(SessionPaths::metadata(&dir)).unwrap();
    assert!(coordinator.restore_all_and_wait(Duration::ZERO).is_err());
    assert!(coordinator.has_session());
    assert!(coordinator.vdd_owned_for_test());
    assert!(!helper.borrow().iter().any(|verb| verb == "disable"));
    assert!(!ccd.rows()[0].active);
    let _ = std::fs::remove_dir_all(dir);
}
#[test]
fn session_result_does_not_disable_preexisting_vdd() {
    let started = Rc::new(RefCell::new(String::new()));
    let helper = Rc::new(RefCell::new(Vec::new()));
    let helper2 = helper.clone();
    let ccd = internal_plus_vdd();
    let mut hooks = coord_hooks(
        started.clone(),
        helper.clone(),
        true,
        true,
        None,
        Duration::from_secs(15),
    );
    hooks.run_driver_helper = Box::new(move |verb| {
        helper2.borrow_mut().push(verb.into());
        if verb == "disable" {
            3
        } else {
            0
        }
    });
    let mut coordinator = RecoveryCoordinator::new(Box::new(ccd.clone()), hooks);
    let identity = ccd
        .query_snapshot(CcdConstants::QUERY_FLAGS)
        .unwrap()
        .physical_screens()
        .find(|r| r.role == PathRole::Internal)
        .unwrap()
        .identity();
    assert!(coordinator.keep_off(identity.clone()).is_none());
    JsonUtil::write_atomic(
        SessionPaths::heartbeat(started.borrow().as_str()),
        &HeartbeatFile {
            state: Default::default(),
            processed_request_id: 0,
            hotkey_registered: true,
            armed: true,
            detail: Some("已保持关闭。".into()),
            screens: vec![HeartbeatScreen {
                adapter_luid: identity.adapter_luid,
                target_id: identity.target_id,
                monitor_path: identity.monitor_path,
                name: "Panel".into(),
                wanted: "保持关闭".into(),
                confirmed: "已关闭".into(),
                detail: "已保持关闭。".into(),
                ..Default::default()
            }],
        },
    )
    .unwrap();
    JsonUtil::write_atomic(
        SessionPaths::result(started.borrow().as_str()),
        &ResultFile {
            protocol_version: PROTOCOL_VERSION,
            restore_state: RestoreState::Complete,
            ok: true,
            reason: "hotkey".into(),
            restore_rc: Some(0),
            restored_topology: true,
            ..Default::default()
        },
    )
    .unwrap();
    coordinator.poll();
    assert!(!coordinator.has_session());
    assert!(!helper.borrow().iter().any(|v| v == "disable"));
    let _ = std::fs::remove_dir_all(started.borrow().as_str());
}
