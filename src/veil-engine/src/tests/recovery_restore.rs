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
fn failed_restore_keeps_hotkey_and_requires_explicit_retry() {
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
    ccd.set_apply_rc(31);
    ccd.set_internal_rc(31);
    publish_release(&dir.0);
    session.tick();
    assert!(!session.exited);
    assert!(hotkey.borrow().registered);
    assert_eq!(session.result.restore_state, RestoreState::Partial);
    let hb: HeartbeatFile = JsonUtil::read(SessionPaths::heartbeat(&dir.0)).unwrap();
    assert_eq!(hb.state, RecoveryState::RestoreFailed);
    assert!(hb.screens.iter().all(|s| s.wanted == "开启"));
    let flags = ccd.flags();
    for _ in 0..30 {
        session.tick();
    }
    assert_eq!(flags, ccd.flags());
    ccd.set_apply_rc(0);
    hotkey.borrow_mut().pressed = true;
    session.tick();
    assert!(session.exited);
    assert_eq!(session.result.restore_state, RestoreState::Complete);
    assert!(!hotkey.borrow().registered);
}
#[test]
fn failed_release_can_be_retried_with_new_request_number() {
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
    ccd.set_apply_rc(31);
    ccd.set_internal_rc(31);
    publish_release(&dir.0);
    session.tick();
    assert!(!session.exited);
    let flags = ccd.flags();
    session.tick();
    assert_eq!(flags, ccd.flags());
    ccd.set_apply_rc(0);
    publish_release(&dir.0);
    session.tick();
    assert!(session.exited);
}
#[test]
fn malformed_release_does_not_loop_restoration() {
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
    ccd.set_apply_rc(31);
    ccd.set_internal_rc(31);
    std::fs::write(SessionPaths::release(&dir.0), "{").unwrap();
    session.tick();
    let flags = ccd.flags();
    for _ in 0..10 {
        session.tick();
    }
    assert_eq!(flags, ccd.flags());
}
#[test]
fn parent_exit_after_restore_failure_finishes_once_with_failure() {
    let dir = TempSession::new();
    let ccd = dual_physical();
    save_topology(&dir.0, &ccd);
    let parent = Rc::new(SharedParent::new());
    let mut session = arm_with_intent(
        &dir.0,
        ccd.clone(),
        parent.clone(),
        Rc::new(SharedClock::new(0.0)),
        false,
    );
    ccd.set_apply_rc(31);
    ccd.set_internal_rc(31);
    publish_release(&dir.0);
    session.tick();
    *parent.alive.borrow_mut() = false;
    session.tick();
    assert!(session.exited);
    assert!(session.result.restoration_outcome().is_err());
    let flags = ccd.flags();
    session.tick();
    assert_eq!(flags, ccd.flags());
}
#[test]
fn single_restore_failure_ends_all_close_requirements() {
    let dir = TempSession::new();
    let ccd = three_physical();
    save_topology(&dir.0, &ccd);
    let mut session = arm_with_intent(
        &dir.0,
        ccd.clone(),
        Rc::new(SharedParent::new()),
        Rc::new(SharedClock::new(0.0)),
        false,
    );
    write_keep_off(
        &dir.0,
        &[(1, r"\\?\DISPLAY#CMN#1"), (2, r"\\?\DISPLAY#PDA#1")],
        false,
    );
    session.tick();
    ccd.set_validate_rc(31);
    write_keep_off(&dir.0, &[(2, r"\\?\DISPLAY#PDA#1")], false);
    session.tick();
    assert!(session.exited);
    assert!(ccd.rows().iter().all(|p| p.active));
    let flags = ccd.flags();
    for _ in 0..10 {
        session.tick();
    }
    assert_eq!(flags, ccd.flags());
}
#[test]
fn failed_additional_close_restores_pre_session_baseline() {
    let dir = TempSession::new();
    let ccd = three_physical();
    save_topology(&dir.0, &ccd);
    let mut session = arm_with_intent(
        &dir.0,
        ccd.clone(),
        Rc::new(SharedParent::new()),
        Rc::new(SharedClock::new(0.0)),
        false,
    );
    ccd.set_next_apply_rc(31);
    write_keep_off(
        &dir.0,
        &[(1, r"\\?\DISPLAY#CMN#1"), (2, r"\\?\DISPLAY#PDA#1")],
        false,
    );
    session.tick();
    assert!(session.exited);
    assert!(ccd.rows().iter().all(|p| p.active));
}
#[test]
fn second_screen_relight_does_not_restore_dual_baseline() {
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
    JsonUtil::write_atomic(SessionPaths::arm(&dir.0), &ArmFile { pid: 11 }).unwrap();
    session.tick();
    write_keep_internal_off(&dir.0, false);
    session.tick();
    assert!(!session.exited);
    assert!(!ccd.rows()[0].active);
    assert!(ccd.rows()[1].active);

    write_keep_off(
        &dir.0,
        &[(1, r"\\?\DISPLAY#CMN#1"), (2, r"\\?\DISPLAY#PDA#1")],
        true,
    );
    session.tick();
    assert!(!session.exited, "等待辅助输出时不能结束本轮并回放双屏");
    assert!(
        !ccd.applied_paths()
            .iter()
            .any(|p| both_physical_targets_active(p)),
        "写出双屏意图后不能 APPLY 两块都亮的基线"
    );

    let mut paths = ccd.paths();
    let mut rows = ccd.rows();
    paths[0].flags |= CcdConstants::DISPLAYCONFIG_PATH_ACTIVE;
    rows[0].active = true;
    paths[1].flags |= CcdConstants::DISPLAYCONFIG_PATH_ACTIVE;
    rows[1].active = true;
    paths.push(fakes::path_default(false, 3));
    rows.push(bundled_vdd_row(3));
    ccd.set_paths_rows(paths, rows);
    session.tick();

    assert!(!session.exited);
    assert!(!ccd.rows()[0].active, "内置屏应保持关闭");
    assert!(!ccd.rows()[1].active, "外接屏应保持关闭");
    assert!(ccd.rows().iter().any(|r| r.is_bundled_vdd() && r.active));
    assert!(
        !ccd.applied_paths()
            .iter()
            .any(|p| both_physical_targets_active(p)),
        "虚拟输出把已关屏点亮后，不能回放原始双屏基线"
    );
    assert!(!ccd
        .flags()
        .iter()
        .any(|f| f & CcdConstants::SDC_TOPOLOGY_CLONE != 0));
}
#[test]
fn validate_87_applies_deactivated_paths_without_clone_topology() {
    let dir = TempSession::new();
    let ccd = internal_plus_vdd();
    save_topology(&dir.0, &ccd);
    ccd.push_validate_rc(87);
    ccd.push_validate_rc(0);
    let session = arm_with_intent(
        &dir.0,
        ccd.clone(),
        Rc::new(SharedParent::new()),
        Rc::new(SharedClock::new(0.0)),
        true,
    );
    assert!(!session.exited);
    assert!(!ccd.rows()[0].active);
    assert!(ccd.rows()[1].active);
    assert!(!ccd
        .flags()
        .iter()
        .any(|f| f & CcdConstants::SDC_TOPOLOGY_CLONE != 0));
    assert!(ccd.applied_paths().iter().any(|paths| {
        paths.iter().any(|p| {
            p.target_info.id == 2 && p.flags & CcdConstants::DISPLAYCONFIG_PATH_ACTIVE != 0
        }) && paths.iter().any(|p| {
            p.target_info.id == 1 && p.flags & CcdConstants::DISPLAYCONFIG_PATH_ACTIVE == 0
        })
    }));
}
#[test]
fn second_physical_intent_is_published_before_vdd_enable() {
    let started = Rc::new(RefCell::new(String::new()));
    let helper = Rc::new(RefCell::new(Vec::new()));
    let ccd = dual_physical();
    let mut hooks = coord_hooks(
        started.clone(),
        helper.clone(),
        true,
        true,
        Some(true),
        Duration::from_secs(2),
    );
    let dir_for_enable = started.clone();
    let ccd_for_enable = ccd.clone();
    let calls = helper.clone();
    hooks.run_driver_helper = Box::new(move |verb| {
        calls.borrow_mut().push(verb.into());
        if verb == "enable" {
            let intent: IntentFile =
                JsonUtil::read(SessionPaths::intent(dir_for_enable.borrow().as_str())).unwrap();
            assert_eq!(
                intent.keep_off.len(),
                2,
                "启用辅助输出前必须已写出两块屏的意图"
            );
            assert!(intent.vdd_assist);
            let mut paths = ccd_for_enable.paths();
            let mut rows = ccd_for_enable.rows();
            paths.push(fakes::path_default(false, 3));
            rows.push(bundled_vdd_row(3));
            ccd_for_enable.set_paths_rows(paths, rows);
        }
        0
    });
    let mut coordinator = RecoveryCoordinator::new(Box::new(ccd.clone()), hooks);
    let ids: Vec<_> = ccd
        .query_snapshot(CcdConstants::QUERY_FLAGS)
        .unwrap()
        .physical_screens()
        .map(|r| r.identity())
        .collect();
    assert!(coordinator.keep_off(ids[0].clone()).is_none());
    helper.borrow_mut().clear();
    assert!(coordinator.keep_off(ids[1].clone()).is_none());
    assert!(helper.borrow().iter().any(|v| v == "enable"));
    let _ = std::fs::remove_dir_all(started.borrow().as_str());
}

#[test]
fn display_instance_row_is_bundled_and_gameviewer_is_not() {
    let _guard = override_bundled_instances(vec![DISPLAY_INSTANCE.into()]);
    let bundled = display_instance_row(true, 3);
    assert!(bundled.is_bundled_vdd());
    let gameviewer = fakes::row(
        PathRole::Virtual,
        true,
        4,
        "GameViewer",
        r"ROOT\DISPLAY\0000",
        r"\\?\DISPLAY#GVV0001",
        "0000000000000001",
    );
    assert!(!gameviewer.is_bundled_vdd());
    assert!(DriverStatus::installed());
}
#[test]
fn active_display_instance_does_not_restore_second_close() {
    let _guard = override_bundled_instances(vec![DISPLAY_INSTANCE.into()]);
    let started = Rc::new(RefCell::new(String::new()));
    let helper = Rc::new(RefCell::new(Vec::new()));
    let ccd = dual_physical();
    let mut hooks = coord_hooks(
        started.clone(),
        helper.clone(),
        true,
        true,
        Some(true),
        Duration::from_secs(2),
    );
    let dir_for_enable = started.clone();
    let ccd_for_enable = ccd.clone();
    hooks.run_driver_helper = Box::new(move |verb| {
        if verb == "enable" {
            let intent: IntentFile =
                JsonUtil::read(SessionPaths::intent(dir_for_enable.borrow().as_str())).unwrap();
            assert_eq!(intent.keep_off.len(), 2);
            let mut paths = ccd_for_enable.paths();
            let mut rows = ccd_for_enable.rows();
            paths.push(fakes::path_default(false, 3));
            rows.push(display_instance_row(true, 3));
            ccd_for_enable.set_paths_rows(paths, rows);
        }
        0
    });
    let mut coordinator = RecoveryCoordinator::new(Box::new(ccd.clone()), hooks);
    let ids: Vec<_> = ccd
        .query_snapshot(CcdConstants::QUERY_FLAGS)
        .unwrap()
        .physical_screens()
        .map(|row| row.identity())
        .collect();
    assert!(coordinator.keep_off(ids[0].clone()).is_none());
    assert!(coordinator.keep_off(ids[1].clone()).is_none());
    let dir = started.borrow().clone();
    assert!(!SessionPaths::release(&dir).exists());
    let intent: IntentFile = JsonUtil::read(SessionPaths::intent(&dir)).unwrap();
    assert_eq!(intent.keep_off.len(), 2);
    let _ = std::fs::remove_dir_all(dir);
}
#[test]
fn inactive_display_instance_is_activated_before_both_physical_stay_off() {
    let _guard = override_bundled_instances(vec![DISPLAY_INSTANCE.into()]);
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
    JsonUtil::write_atomic(SessionPaths::arm(&dir.0), &ArmFile { pid: 11 }).unwrap();
    session.tick();
    write_keep_internal_off(&dir.0, false);
    session.tick();
    write_keep_off(
        &dir.0,
        &[(1, r"\\?\DISPLAY#CMN#1"), (2, r"\\?\DISPLAY#PDA#1")],
        true,
    );
    session.tick();
    assert!(!session.exited);

    let mut paths = ccd.paths();
    let mut rows = ccd.rows();
    paths.push(fakes::path(false, false, 3, 0x0001FFFF, 1));
    rows.push(display_instance_row(false, 3));
    ccd.set_paths_rows(paths, rows);
    session.tick();

    assert!(!session.exited);
    assert!(
        !ccd.rows()
            .iter()
            .any(|row| row.target_id == 1 && row.active),
        "内置屏应保持关闭"
    );
    assert!(
        !ccd.rows()
            .iter()
            .any(|row| row.target_id == 2 && row.active),
        "外接屏应保持关闭"
    );
    assert!(ccd
        .rows()
        .iter()
        .any(|row| row.is_bundled_vdd() && row.active));
    assert!(
        !ccd.applied_paths()
            .iter()
            .any(|paths| both_physical_targets_active(paths)),
        "激活辅助路径时不能把两块物理屏一起点亮"
    );
    let events = std::fs::read_to_string(SessionPaths::events(&dir.0)).unwrap();
    assert!(events.contains("ROOT#DISPLAY#0002"));
    assert!(events.contains("vdd-activate"));
    assert_activated_vdd_uses_virtual_modes(&ccd);
}
#[test]
fn activate_inactive_display_instance_retries_validate_87() {
    let _guard = override_bundled_instances(vec![DISPLAY_INSTANCE.into()]);
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
    JsonUtil::write_atomic(SessionPaths::arm(&dir.0), &ArmFile { pid: 11 }).unwrap();
    session.tick();
    write_keep_internal_off(&dir.0, false);
    session.tick();
    write_keep_off(
        &dir.0,
        &[(1, r"\\?\DISPLAY#CMN#1"), (2, r"\\?\DISPLAY#PDA#1")],
        true,
    );
    session.tick();
    let mut paths = ccd.paths();
    let mut rows = ccd.rows();
    paths.push(fakes::path(false, false, 3, 0x0001FFFF, 1));
    rows.push(display_instance_row(false, 3));
    ccd.set_paths_rows(paths, rows);
    ccd.push_validate_rc(87);
    session.tick();

    assert!(!session.exited);
    assert!(!ccd
        .rows()
        .iter()
        .any(|row| row.target_id == 1 && row.active));
    assert!(!ccd
        .rows()
        .iter()
        .any(|row| row.target_id == 2 && row.active));
    assert!(ccd
        .rows()
        .iter()
        .any(|row| row.is_bundled_vdd() && row.active));
    assert!(ccd.applied_paths().iter().any(|paths| {
        paths.iter().any(|path| {
            path.target_info.id == 3 && path.flags & CcdConstants::DISPLAYCONFIG_PATH_ACTIVE != 0
        }) && !paths.iter().any(|path| {
            path.target_info.id == 1 && path.flags & CcdConstants::DISPLAYCONFIG_PATH_ACTIVE != 0
        })
    }));
    let events = std::fs::read_to_string(SessionPaths::events(&dir.0)).unwrap();
    assert!(!events.contains("未改拓扑"));
    assert_activated_vdd_uses_virtual_modes(&ccd);
}
#[test]
fn missing_display_instance_restores_once() {
    let _guard = override_bundled_instances(vec![DISPLAY_INSTANCE.into()]);
    let dir = TempSession::new();
    let ccd = dual_physical();
    save_topology(&dir.0, &ccd);
    let clock = Rc::new(SharedClock::new(0.0));
    let mut session = RecoverySession::new(options(
        dir.0.clone(),
        ccd.clone(),
        FakeHotkey::new(),
        11,
        Rc::new(SharedParent::new()),
        clock.clone(),
    ));
    session.start();
    JsonUtil::write_atomic(SessionPaths::arm(&dir.0), &ArmFile { pid: 11 }).unwrap();
    session.tick();
    write_keep_internal_off(&dir.0, false);
    session.tick();
    write_keep_off(
        &dir.0,
        &[(1, r"\\?\DISPLAY#CMN#1"), (2, r"\\?\DISPLAY#PDA#1")],
        true,
    );
    session.tick();
    assert!(!session.exited);
    let before = ccd
        .flags()
        .iter()
        .filter(|flag| **flag == CcdConstants::APPLY_FLAGS)
        .count();
    clock.set(30.0);
    session.tick();
    assert!(session.exited);
    let events = std::fs::read_to_string(SessionPaths::events(&dir.0)).unwrap();
    assert!(events.contains("全部路径里没有辅助虚拟输出设备"));
    let after = ccd
        .flags()
        .iter()
        .filter(|flag| **flag == CcdConstants::APPLY_FLAGS)
        .count();
    assert!(after <= before + 1);
    session.tick();
    assert_eq!(
        after,
        ccd.flags()
            .iter()
            .filter(|flag| **flag == CcdConstants::APPLY_FLAGS)
            .count()
    );
}
#[test]
fn clone_failure_cannot_repeat_on_later_ticks() {
    let dir = TempSession::new();
    let ccd = internal_plus_vdd();
    save_topology(&dir.0, &ccd);
    ccd.set_validate_rc(87);
    ccd.set_clone_rc(31);
    let mut session = arm_with_intent(
        &dir.0,
        ccd.clone(),
        Rc::new(SharedParent::new()),
        Rc::new(SharedClock::new(0.0)),
        true,
    );
    assert!(session.exited);
    let flags = ccd.flags();
    for _ in 0..10 {
        session.tick();
    }
    assert_eq!(flags, ccd.flags());
    assert_eq!(
        flags
            .iter()
            .filter(|f| **f == (CcdConstants::SDC_APPLY | CcdConstants::SDC_TOPOLOGY_CLONE))
            .count(),
        0
    );
}
#[test]
fn failed_reapply_restores_and_does_not_loop() {
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
    ccd.set_validate_rc(31);
    ccd.activate_path(0);
    session.tick();
    assert!(session.exited);
    assert!(ccd.rows().iter().all(|p| p.active));
    let flags = ccd.flags();
    for _ in 0..10 {
        session.tick();
    }
    assert_eq!(flags, ccd.flags());
}
#[test]
fn restoration_waits_for_delayed_enumeration() {
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
    ccd.set_stale_captures(4);
    publish_release(&dir.0);
    session.tick();
    assert_eq!(session.result.restore_state, RestoreState::Complete);
    assert_eq!(session.result.fallback_rc, None);
}
#[test]
fn restore_only_never_consumes_close_intent() {
    let dir = TempSession::new();
    let ccd = dual_physical();
    save_topology(&dir.0, &ccd);
    ccd.deactivate_path(0);
    write_keep_internal_off(&dir.0, false);
    let mut opt = options(
        dir.0.clone(),
        ccd.clone(),
        FakeHotkey::new(),
        11,
        Rc::new(SharedParent::new()),
        Rc::new(SharedClock::new(0.0)),
    );
    opt.restore_only = true;
    let mut session = RecoverySession::new(opt);
    session.start();
    assert!(session.exited);
    assert!(ccd.rows().iter().all(|p| p.active));
    assert!(!ccd.validated());
}
#[test]
fn disconnected_target_does_not_block_verified_physical_recovery() {
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
    ccd.set_after_apply(|view| {
        view.paths[1].target_info.target_available = 0;
        view.paths[1].flags &= !1;
        view.rows[1].active = false;
    });
    publish_release(&dir.0);
    session.tick();
    assert_eq!(session.result.restore_state, RestoreState::Complete);
    assert!(!session.result.restored_topology);
}
#[test]
fn enumeration_error_during_restore_is_unknown_not_success() {
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
    ccd.set_capture_error("unavailable");
    publish_release(&dir.0);
    session.tick();
    assert_eq!(session.result.restore_state, RestoreState::Unknown);
    assert!(!session.exited);
    let flags = ccd.flags();
    for _ in 0..10 {
        session.tick();
    }
    assert_eq!(flags, ccd.flags());
}
#[test]
fn unknown_session_protocol_never_applies() {
    let dir = TempSession::new();
    let ccd = dual_physical();
    save_topology(&dir.0, &ccd);
    JsonUtil::write_atomic(SessionPaths::metadata(&dir.0), &SessionMetadata::default()).unwrap();
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
    assert!(session.result.restoration_outcome().is_err());
}
#[test]
fn activate_inactive_bundled_uses_session_vdd_resolution() {
    let _guard = override_bundled_instances(vec![DISPLAY_INSTANCE.into()]);
    let dir = TempSession::new();
    let ccd = dual_physical();
    save_topology(&dir.0, &ccd);
    JsonUtil::write_atomic(
        SessionPaths::vdd_resolution(&dir.0),
        &crate::VddResolutionRequest {
            width: 2560,
            height: 1440,
        },
    )
    .unwrap();
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
    write_keep_internal_off(&dir.0, false);
    session.tick();
    write_keep_off(
        &dir.0,
        &[(1, r"\\?\DISPLAY#CMN#1"), (2, r"\\?\DISPLAY#PDA#1")],
        true,
    );
    session.tick();
    let mut paths = ccd.paths();
    let mut rows = ccd.rows();
    paths.push(fakes::path(false, false, 3, 0x0001FFFF, 1));
    rows.push(display_instance_row(false, 3));
    ccd.set_paths_rows(paths, rows);
    session.tick();

    assert!(!session.exited);
    let paths = ccd.paths();
    let modes = ccd.modes();
    let path = paths
        .iter()
        .find(|path| {
            path.target_info.id == 3 && path.flags & CcdConstants::DISPLAYCONFIG_PATH_ACTIVE != 0
        })
        .expect("辅助路径应处于活动");
    let source_index = (path.source_info.mode_info_idx >> 16) as usize;
    let target_index = (path.target_info.mode_info_idx >> 16) as usize;
    assert_eq!(modes[source_index].source_mode().width, 2560);
    assert_eq!(modes[source_index].source_mode().height, 1440);
    let signal = unsafe {
        modes[target_index]
            .union
            .target_mode
            .target_video_signal_info
    };
    assert_eq!(signal.active_size.cx, 2560);
    assert_eq!(signal.active_size.cy, 1440);
}
#[test]
fn clone_validate_87_retry_marks_adjusted_clone_true() {
    let dir = TempSession::new();
    let ccd = internal_plus_vdd();
    save_topology(&dir.0, &ccd);
    ccd.push_validate_rc(87);
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
    write_keep_internal_off(&dir.0, true);
    session.tick();

    assert!(!session.exited);
    assert_eq!(session.result.adjusted_clone, Some(true));
}
