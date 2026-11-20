use crate::fakes::{self, FakeCcd};
use crate::native::{CcdApi, CcdConstants, DisplayConfigModeInfo};
use crate::session::{JsonUtil, ReadyFile, ResultFile, SessionPaths};
use crate::session::{RestoreState, PROTOCOL_VERSION};
use crate::*;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use super::support::*;

#[test]
fn cancelled_enable_prompt_does_not_touch_helper_or_screens() {
    let helper = Rc::new(RefCell::new(Vec::new()));
    let ccd = Rc::new(FakeCcd::with_paths_rows(
        vec![fakes::path_default(true, 1)],
        vec![DisplayConfigModeInfo {
            info_type: 1,
            ..Default::default()
        }],
        vec![fakes::row_simple(
            PathRole::Internal,
            1,
            "Panel",
            r"\\?\DISPLAY#CMN#1",
        )],
    ));
    let mut coordinator = RecoveryCoordinator::new(
        Box::new(ccd.clone()),
        coord_hooks(
            Rc::new(RefCell::new(String::new())),
            helper.clone(),
            true,
            true,
            Some(false),
            Duration::from_secs(15),
        ),
    );
    let identity = ccd
        .query_snapshot(CcdConstants::QUERY_FLAGS)
        .unwrap()
        .physical_screens()
        .next()
        .unwrap()
        .identity();
    assert_eq!(
        coordinator.keep_off(identity).as_deref(),
        Some(RecoveryCoordinator::ENABLE_VDD_CANCELLED)
    );
    assert!(helper.borrow().is_empty());
    assert!(!coordinator.has_session());
    assert_eq!(
        ccd.query_snapshot(CcdConstants::QUERY_FLAGS)
            .unwrap()
            .active_physical()
            .count(),
        1
    );
}
#[test]
fn cancelled_install_prompt_does_not_touch_helper_or_screens() {
    let helper = Rc::new(RefCell::new(Vec::new()));
    let ccd = Rc::new(FakeCcd::with_paths_rows(
        vec![fakes::path_default(true, 1)],
        vec![DisplayConfigModeInfo {
            info_type: 1,
            ..Default::default()
        }],
        vec![fakes::row_simple(
            PathRole::Internal,
            1,
            "Panel",
            r"\\?\DISPLAY#CMN#1",
        )],
    ));
    let mut hooks = coord_hooks(
        Rc::new(RefCell::new(String::new())),
        helper.clone(),
        false,
        true,
        Some(false),
        Duration::from_secs(15),
    );
    hooks.bundled_vdd_payload = Box::new(|| true);
    let mut coordinator = RecoveryCoordinator::new(Box::new(ccd.clone()), hooks);
    let identity = ccd
        .query_snapshot(CcdConstants::QUERY_FLAGS)
        .unwrap()
        .physical_screens()
        .next()
        .unwrap()
        .identity();
    assert_eq!(
        coordinator.keep_off(identity).as_deref(),
        Some(RecoveryCoordinator::INSTALL_VDD_CANCELLED)
    );
    assert!(helper.borrow().is_empty());
    assert!(!coordinator.has_session());
}
#[test]
fn failed_install_does_not_enable_or_change_screens() {
    let helper = Rc::new(RefCell::new(Vec::new()));
    let helper2 = helper.clone();
    let ccd = Rc::new(FakeCcd::with_paths_rows(
        vec![fakes::path_default(true, 1)],
        vec![DisplayConfigModeInfo {
            info_type: 1,
            ..Default::default()
        }],
        vec![fakes::row_simple(
            PathRole::Internal,
            1,
            "Panel",
            r"\\?\DISPLAY#CMN#1",
        )],
    ));
    let mut hooks = coord_hooks(
        Rc::new(RefCell::new(String::new())),
        helper.clone(),
        false,
        true,
        Some(true),
        Duration::from_secs(15),
    );
    hooks.bundled_vdd_payload = Box::new(|| true);
    hooks.run_driver_helper = Box::new(move |verb| {
        helper2.borrow_mut().push(verb.into());
        1
    });
    let mut coordinator = RecoveryCoordinator::new(Box::new(ccd.clone()), hooks);
    let identity = ccd
        .query_snapshot(CcdConstants::QUERY_FLAGS)
        .unwrap()
        .physical_screens()
        .next()
        .unwrap()
        .identity();
    let error = coordinator.keep_off(identity);
    assert_eq!(
        error.as_deref(),
        Some(RecoveryCoordinator::INSTALL_VDD_FAILED)
    );
    assert_eq!(*helper.borrow(), vec!["install-driver".to_string()]);
    assert_eq!(
        ccd.query_snapshot(CcdConstants::QUERY_FLAGS)
            .unwrap()
            .active_physical()
            .count(),
        1
    );
}
#[test]
fn confirmed_install_runs_install_then_enable() {
    let helper = Rc::new(RefCell::new(Vec::new()));
    let helper2 = helper.clone();
    let ccd = Rc::new(FakeCcd::with_paths_rows(
        vec![fakes::path_default(true, 1)],
        vec![DisplayConfigModeInfo {
            info_type: 1,
            ..Default::default()
        }],
        vec![fakes::row_simple(
            PathRole::Internal,
            1,
            "Panel",
            r"\\?\DISPLAY#CMN#1",
        )],
    ));
    let ccd2 = ccd.clone();
    let mut hooks = coord_hooks(
        Rc::new(RefCell::new(String::new())),
        helper.clone(),
        false,
        true,
        Some(true),
        Duration::from_secs(15),
    );
    hooks.bundled_vdd_payload = Box::new(|| true);
    hooks.run_driver_helper = Box::new(move |verb| {
        helper2.borrow_mut().push(verb.into());
        if verb == "enable" {
            ccd2.set_paths_rows(
                vec![fakes::path_default(true, 1), fakes::path_default(false, 2)],
                vec![
                    fakes::row_simple(PathRole::Internal, 1, "Panel", r"\\?\DISPLAY#CMN#1"),
                    fakes::row(
                        PathRole::Virtual,
                        true,
                        2,
                        "VDD by MTT",
                        r"ROOT\MttVDD\0000",
                        r"\\?\DISPLAY#MTT1337#1",
                        "0000000000000001",
                    ),
                ],
            );
        }
        0
    });
    let mut coordinator = RecoveryCoordinator::new(Box::new(ccd.clone()), hooks);
    let identity = ccd
        .query_snapshot(CcdConstants::QUERY_FLAGS)
        .unwrap()
        .physical_screens()
        .next()
        .unwrap()
        .identity();
    let error = coordinator.keep_off(identity);
    assert!(error.is_none(), "{error:?}");
    assert_eq!(
        *helper.borrow(),
        vec!["install-driver".to_string(), "enable".to_string()]
    );
    assert!(coordinator.has_session());
    let _ = std::fs::remove_dir_all(coordinator.session_directory().unwrap());
}
#[test]
fn panel_install_does_not_change_screens() {
    let helper = Rc::new(RefCell::new(Vec::new()));
    let helper2 = helper.clone();
    let ccd = Rc::new(FakeCcd::with_paths_rows(
        vec![fakes::path_default(true, 1)],
        vec![DisplayConfigModeInfo {
            info_type: 1,
            ..Default::default()
        }],
        vec![fakes::row_simple(
            PathRole::Internal,
            1,
            "Panel",
            r"\\?\DISPLAY#CMN#1",
        )],
    ));
    let mut hooks = coord_hooks(
        Rc::new(RefCell::new(String::new())),
        helper.clone(),
        false,
        true,
        Some(true),
        Duration::from_secs(15),
    );
    hooks.bundled_vdd_payload = Box::new(|| true);
    hooks.run_driver_helper = Box::new(move |verb| {
        helper2.borrow_mut().push(verb.into());
        0
    });
    let mut coordinator = RecoveryCoordinator::new(Box::new(ccd.clone()), hooks);
    assert!(coordinator.install_auxiliary_output().is_none());
    assert_eq!(*helper.borrow(), vec!["install-driver".to_string()]);
    assert!(!coordinator.has_session());
    assert_eq!(
        ccd.query_snapshot(CcdConstants::QUERY_FLAGS)
            .unwrap()
            .active_physical()
            .count(),
        1
    );
}
#[test]
fn panel_install_without_payload_explains() {
    let helper = Rc::new(RefCell::new(Vec::new()));
    let ccd = Rc::new(FakeCcd::with_paths_rows(
        vec![fakes::path_default(true, 1)],
        vec![DisplayConfigModeInfo {
            info_type: 1,
            ..Default::default()
        }],
        vec![fakes::row_simple(
            PathRole::Internal,
            1,
            "Panel",
            r"\\?\DISPLAY#CMN#1",
        )],
    ));
    let hooks = coord_hooks(
        Rc::new(RefCell::new(String::new())),
        helper.clone(),
        false,
        true,
        Some(true),
        Duration::from_secs(15),
    );
    let mut coordinator = RecoveryCoordinator::new(Box::new(ccd), hooks);
    assert_eq!(
        coordinator.install_auxiliary_output().as_deref(),
        Some(RecoveryCoordinator::AUXILIARY_PAYLOAD_MISSING)
    );
    assert!(helper.borrow().is_empty());
}
#[test]
fn dead_recovery_retains_restore_context_and_preexisting_vdd() {
    let started = Rc::new(RefCell::new(String::new()));
    let helper = Rc::new(RefCell::new(Vec::new()));
    let ccd = internal_plus_vdd();
    let mut coordinator = RecoveryCoordinator::new(
        Box::new(ccd.clone()),
        coord_hooks(
            started.clone(),
            helper.clone(),
            true,
            false,
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
    coordinator.poll();
    assert!(coordinator.has_session());
    assert!(coordinator
        .status_text
        .as_deref()
        .unwrap()
        .starts_with(RecoveryCoordinator::RECOVERY_EXITED));
    assert!(coordinator
        .status_text
        .as_deref()
        .unwrap()
        .contains("记录："));
    assert!(!helper.borrow().iter().any(|v| v == "disable"));
    let result: ResultFile =
        JsonUtil::read(SessionPaths::result(started.borrow().as_str())).unwrap();
    assert_eq!(result.reason, RecoveryCoordinator::RECOVERY_EXIT_REASON);
    assert!(!result.ok);
    let _ = std::fs::remove_dir_all(started.borrow().as_str());
}
#[test]
fn alive_recovery_without_result_keeps_session() {
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
    coordinator.poll();
    assert!(coordinator.has_session());
    assert!(!helper.borrow().iter().any(|v| v == "disable"));
    assert!(!SessionPaths::result(started.borrow().as_str()).exists());
    let _ = std::fs::remove_dir_all(started.borrow().as_str());
}
#[test]
fn dead_recovery_does_not_disable_vdd_when_no_physical_remains() {
    let started = Rc::new(RefCell::new(String::new()));
    let helper = Rc::new(RefCell::new(Vec::new()));
    let ccd = internal_plus_vdd();
    let mut coordinator = RecoveryCoordinator::new(
        Box::new(ccd.clone()),
        coord_hooks(
            started.clone(),
            helper.clone(),
            true,
            false,
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
    ccd.deactivate_path(0);
    coordinator.poll();
    assert!(coordinator.has_session());
    assert!(!helper.borrow().iter().any(|v| v == "disable"));
    assert!(coordinator
        .status_text
        .as_deref()
        .unwrap()
        .contains(RecoveryCoordinator::RECOVERY_EXITED));
    let _ = std::fs::remove_dir_all(started.borrow().as_str());
}
#[test]
fn confirmed_enable_then_missing_virtual_path_requests_restore_before_cleanup() {
    let helper = Rc::new(RefCell::new(Vec::new()));
    let ccd = Rc::new(FakeCcd::with_paths_rows(
        vec![fakes::path_default(true, 1)],
        vec![DisplayConfigModeInfo {
            info_type: 1,
            ..Default::default()
        }],
        vec![fakes::row_simple(
            PathRole::Internal,
            1,
            "Panel",
            r"\\?\DISPLAY#CMN#1",
        )],
    ));
    let mut coordinator = RecoveryCoordinator::new(
        Box::new(ccd.clone()),
        coord_hooks(
            Rc::new(RefCell::new(String::new())),
            helper.clone(),
            true,
            true,
            Some(true),
            Duration::ZERO,
        ),
    );
    let identity = ccd
        .query_snapshot(CcdConstants::QUERY_FLAGS)
        .unwrap()
        .physical_screens()
        .next()
        .unwrap()
        .identity();
    let error = coordinator.keep_off(identity);
    assert_eq!(
        error.as_deref(),
        Some("辅助虚拟输出未能出现活动虚拟路径，物理屏未改动。")
    );
    assert_eq!(*helper.borrow(), vec!["enable".to_string()]);
    let dir = coordinator.session_directory().unwrap().to_path_buf();
    assert!(SessionPaths::release(&dir).exists());
    assert!(coordinator.restore_request_pending());
    let events = std::fs::read_to_string(SessionPaths::events(&dir)).unwrap();
    assert!(events.contains("vdd-path-wait-end"));
    assert!(events.contains("active=1 virtual=0 bundled=0"));
    assert!(events.contains("helper-operation"));
    JsonUtil::write_atomic(
        SessionPaths::result(&dir),
        &ResultFile {
            protocol_version: PROTOCOL_VERSION,
            restore_state: RestoreState::Complete,
            ..Default::default()
        },
    )
    .unwrap();
    coordinator.poll();
    assert_eq!(
        *helper.borrow(),
        vec!["enable".to_string(), "disable".to_string()]
    );
    std::fs::remove_dir_all(dir).unwrap();
    assert!(!coordinator.has_session());
}
#[test]
fn inactive_bundled_path_is_activated_before_keep_off() {
    let helper = Rc::new(RefCell::new(Vec::new()));
    let ccd = Rc::new(FakeCcd::with_paths_rows(
        vec![
            fakes::path_default(true, 1),
            fakes::path(false, false, 3, 0x0001FFFF, 1),
        ],
        vec![DisplayConfigModeInfo {
            info_type: 1,
            ..Default::default()
        }],
        vec![
            fakes::row_simple(PathRole::Internal, 1, "Panel", r"\\?\DISPLAY#CMN#1"),
            fakes::row(
                PathRole::Virtual,
                false,
                3,
                "VDD by MTT",
                r"\\?\ROOT#DISPLAY#0002#{e6f07b5f-ee97-4a90-b076-33f57bf4eaa7}",
                r"\\?\DISPLAY#MTT1337#1&33320f0&11&UID256",
                "0000000000000001",
            ),
        ],
    ));
    let mut coordinator = RecoveryCoordinator::new(
        Box::new(ccd.clone()),
        coord_hooks(
            Rc::new(RefCell::new(String::new())),
            helper.clone(),
            true,
            true,
            Some(true),
            Duration::from_secs(15),
        ),
    );
    let identity = ccd
        .query_snapshot(CcdConstants::QUERY_FLAGS)
        .unwrap()
        .physical_screens()
        .next()
        .unwrap()
        .identity();
    let error = coordinator.keep_off(identity);
    assert_eq!(error, None);
    assert!(ccd
        .rows()
        .iter()
        .any(|row| row.is_bundled_vdd() && row.active));
    let dir = coordinator.session_directory().unwrap().to_path_buf();
    let events = std::fs::read_to_string(SessionPaths::events(&dir)).unwrap();
    assert!(events.contains("vdd-activate"));
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn cancel_after_helper_enable_does_not_publish_close_intent() {
    let cancelled = Rc::new(std::cell::Cell::new(false));
    let helper = Rc::new(RefCell::new(Vec::new()));
    let ccd = Rc::new(FakeCcd::with_paths_rows(
        vec![fakes::path_default(true, 1)],
        vec![DisplayConfigModeInfo {
            info_type: 1,
            ..Default::default()
        }],
        vec![fakes::row_simple(
            PathRole::Internal,
            1,
            "Panel",
            r"\\?\DISPLAY#CMN#1",
        )],
    ));
    let mut hooks = coord_hooks(
        Rc::new(RefCell::new(String::new())),
        helper.clone(),
        true,
        true,
        Some(true),
        Duration::ZERO,
    );
    let cancelled_for_helper = cancelled.clone();
    let helper_for_hook = helper.clone();
    hooks.run_driver_helper = Box::new(move |verb| {
        helper_for_hook.borrow_mut().push(verb.into());
        if verb == "enable" {
            cancelled_for_helper.set(true);
        }
        0
    });
    hooks.cancel_requested = Box::new(move || cancelled.get());
    let mut coordinator = RecoveryCoordinator::new(Box::new(ccd.clone()), hooks);
    let identity = ccd.query_snapshot(CcdConstants::QUERY_FLAGS).unwrap().paths[0].identity();
    let error = coordinator.keep_off(identity).unwrap();
    let dir = coordinator.session_directory().unwrap().to_path_buf();
    assert!(error.contains("已取消"));
    assert!(!SessionPaths::intent(&dir).exists());
    assert!(SessionPaths::release(&dir).exists());
    assert_eq!(*helper.borrow(), vec!["enable".to_string()]);
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn cancel_while_recovery_becomes_ready_does_not_publish_close_intent() {
    let cancelled = Rc::new(std::cell::Cell::new(false));
    let ccd = internal_plus_vdd();
    let mut hooks = coord_hooks(
        Rc::new(RefCell::new(String::new())),
        Rc::new(RefCell::new(Vec::new())),
        true,
        true,
        None,
        Duration::ZERO,
    );
    let cancelled_for_start = cancelled.clone();
    hooks.start_recovery = Box::new(move |dir, _| {
        JsonUtil::write_atomic(
            SessionPaths::ready(dir),
            &ReadyFile {
                pid: 4242,
                hotkey_registered: true,
                hotkey: CcdConstants::HOTKEY_TEXT.into(),
            },
        )
        .unwrap();
        cancelled_for_start.set(true);
        4242
    });
    hooks.cancel_requested = Box::new(move || cancelled.get());
    let identity = ccd
        .rows()
        .iter()
        .find(|r| r.is_physical())
        .unwrap()
        .identity();
    let mut coordinator = RecoveryCoordinator::new(Box::new(ccd), hooks);
    let error = coordinator.keep_off(identity).unwrap();
    let dir = coordinator.session_directory().unwrap().to_path_buf();
    assert!(error.contains("已取消"));
    assert!(!SessionPaths::intent(&dir).exists());
    assert!(SessionPaths::release(&dir).exists());
    std::fs::remove_dir_all(dir).unwrap();
}
