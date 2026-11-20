use crate::fakes::{self, FakeHotkey, SharedClock, SharedParent};
use crate::session::{HeartbeatFile, IntentFile, JsonUtil, ResultFile, SessionPaths};
use crate::session::{RecoveryState, RestoreState};
use crate::*;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use super::support::*;

#[derive(Clone, Copy, PartialEq, Eq)]
enum SecondCloseSchedule {
    RecoveryFirst,
    HelperFirst,
    StaleRequest,
    DifferentTargets,
    NoRemainingPath,
    ValidateFailure,
    HelperFailure,
    MissingVirtualPath,
    Cancelled,
    RestoreFailed,
}

fn exercise_second_close_race(first_screen: usize, schedule: SecondCloseSchedule) {
    let recovery_before_helper = !matches!(
        schedule,
        SecondCloseSchedule::HelperFirst | SecondCloseSchedule::ValidateFailure
    );
    let expected_error = match schedule {
        SecondCloseSchedule::RecoveryFirst | SecondCloseSchedule::HelperFirst => None,
        SecondCloseSchedule::StaleRequest | SecondCloseSchedule::DifferentTargets => Some("校验 0"),
        SecondCloseSchedule::NoRemainingPath => Some("没有第二活动目标"),
        SecondCloseSchedule::ValidateFailure => Some("校验 31"),
        SecondCloseSchedule::HelperFailure => Some(crate::coordinator::ENABLE_VDD_FAILED),
        SecondCloseSchedule::MissingVirtualPath => Some(crate::coordinator::VDD_PATH_MISSING),
        SecondCloseSchedule::Cancelled => Some("已取消本次保持关闭请求"),
        SecondCloseSchedule::RestoreFailed => Some("恢复未完成"),
    };
    let _guard = override_bundled_instances(vec![DISPLAY_INSTANCE.into()]);
    let started = Rc::new(RefCell::new(String::new()));
    let helper = Rc::new(RefCell::new(Vec::new()));
    let ccd = dual_physical();
    let worker: Rc<RefCell<Option<RecoverySession>>> = Rc::new(RefCell::new(None));
    let cancelled = Rc::new(std::cell::Cell::new(false));
    let mut hooks = coord_hooks(
        started.clone(),
        helper.clone(),
        true,
        true,
        Some(true),
        Duration::from_secs(2),
    );
    if schedule == SecondCloseSchedule::MissingVirtualPath {
        hooks.virtual_path_wait = Duration::ZERO;
    }
    let worker_for_start = worker.clone();
    let ccd_for_start = ccd.clone();
    let dir_for_start = started.clone();
    hooks.start_recovery = Box::new(move |dir, _| {
        *dir_for_start.borrow_mut() = dir.into();
        let mut session = RecoverySession::new(options(
            PathBuf::from(dir),
            ccd_for_start.clone(),
            FakeHotkey::new(),
            4242,
            Rc::new(SharedParent::new()),
            Rc::new(SharedClock::new(0.0)),
        ));
        session.start();
        *worker_for_start.borrow_mut() = Some(session);
        4242
    });
    let worker_for_helper = worker.clone();
    let ccd_for_helper = ccd.clone();
    let dir_for_helper = started.clone();
    let calls = helper.clone();
    let cancelled_for_helper = cancelled.clone();
    hooks.run_driver_helper = Box::new(move |verb| {
        calls.borrow_mut().push(verb.into());
        if verb == "enable" {
            let mut intent: IntentFile =
                JsonUtil::read(SessionPaths::intent(dir_for_helper.borrow().as_str())).unwrap();
            assert_eq!(intent.keep_off.len(), 2, "启用前必须发布两屏关闭要求");
            if recovery_before_helper {
                worker_for_helper.borrow_mut().as_mut().unwrap().tick();
            }
            if schedule == SecondCloseSchedule::MissingVirtualPath {
                return 0;
            }
            let mut paths = ccd_for_helper.paths();
            let mut rows = ccd_for_helper.rows();
            paths.push(fakes::path(
                false,
                !recovery_before_helper,
                3,
                0x0001FFFF,
                1,
            ));
            rows.push(display_instance_row(!recovery_before_helper, 3));
            ccd_for_helper.set_paths_rows(paths, rows);
            if recovery_before_helper {
                worker_for_helper.borrow_mut().as_mut().unwrap().tick();
                assert!(!ccd_for_helper
                    .rows()
                    .iter()
                    .any(|row| row.is_physical() && row.active));
            }
            match schedule {
                SecondCloseSchedule::StaleRequest | SecondCloseSchedule::DifferentTargets => {
                    if schedule == SecondCloseSchedule::StaleRequest {
                        intent.request_id -= 1;
                    } else {
                        intent.keep_off[0].target_id = 99;
                    }
                    JsonUtil::write_atomic(
                        SessionPaths::intent(dir_for_helper.borrow().as_str()),
                        &intent,
                    )
                    .unwrap();
                }
                SecondCloseSchedule::ValidateFailure => ccd_for_helper.set_validate_rc(31),
                SecondCloseSchedule::HelperFailure => return 31,
                SecondCloseSchedule::Cancelled => cancelled_for_helper.set(true),
                SecondCloseSchedule::RestoreFailed => {
                    let heartbeat_path = SessionPaths::heartbeat(dir_for_helper.borrow().as_str());
                    let mut heartbeat: HeartbeatFile = JsonUtil::read(&heartbeat_path).unwrap();
                    heartbeat.state = RecoveryState::RestoreFailed;
                    heartbeat.detail = Some("恢复未完成".into());
                    JsonUtil::write_atomic(heartbeat_path, &heartbeat).unwrap();
                }
                _ => {}
            }
        } else if verb == "disable" {
            if let Some(index) = ccd_for_helper
                .rows()
                .iter()
                .position(|row| row.is_bundled_vdd())
            {
                ccd_for_helper.deactivate_path(index);
            }
        }
        0
    });
    hooks.cancel_requested = Box::new(move || cancelled.get());
    if schedule == SecondCloseSchedule::NoRemainingPath {
        let ccd_for_progress = ccd.clone();
        hooks.on_progress = Box::new(move |stage| {
            let rows = ccd_for_progress.rows();
            if stage == "正在校验显示配置并准备安全恢复"
                && !rows.iter().any(|row| row.is_physical() && row.active)
            {
                if let Some(index) = rows
                    .iter()
                    .position(|row| row.is_bundled_vdd() && row.active)
                {
                    ccd_for_progress.deactivate_path(index);
                }
            }
        });
    }
    let ids: Vec<_> = ccd.rows().iter().map(|row| row.identity()).collect();
    let mut coordinator = RecoveryCoordinator::new(Box::new(ccd.clone()), hooks);
    assert!(coordinator.keep_off(ids[first_screen].clone()).is_none());
    let dir = TempSession(PathBuf::from(started.borrow().as_str()));
    worker.borrow_mut().as_mut().unwrap().tick();
    worker.borrow_mut().as_mut().unwrap().tick();
    assert!(!ccd.rows()[first_screen].active);
    let second_result = coordinator.keep_off(ids[1 - first_screen].clone());
    if let Some(expected) = expected_error {
        assert!(
            second_result
                .as_deref()
                .is_some_and(|error| error.contains(expected)),
            "应拒绝本次请求：{second_result:?}"
        );
        assert!(SessionPaths::release(&dir.0).exists());
        assert_eq!(
            &*helper.borrow(),
            &["enable"],
            "未确认恢复前不能清理辅助输出"
        );
        let events = std::fs::read_to_string(SessionPaths::events(&dir.0)).unwrap();
        if matches!(
            schedule,
            SecondCloseSchedule::StaleRequest
                | SecondCloseSchedule::DifferentTargets
                | SecondCloseSchedule::NoRemainingPath
                | SecondCloseSchedule::ValidateFailure
        ) {
            assert!(events.contains("keep-off-precheck-rejected"));
            assert!(!events.contains("keep-off-precheck-already-off"));
            assert!(events.contains("disabled_count="));
            assert!(events.contains("remaining_active="));
        }
        return;
    }
    assert!(
        second_result.is_none(),
        "第二次关屏不应回滚：{second_result:?}"
    );
    let pending_intent: IntentFile = JsonUtil::read(SessionPaths::intent(&dir.0)).unwrap();
    assert!(
        coordinator.heartbeat.as_ref().unwrap().processed_request_id < pending_intent.request_id,
        "主程序预检查不能冒充恢复进程对新请求的确认"
    );
    let apply_count = ccd.applied_paths().len();
    worker.borrow_mut().as_mut().unwrap().tick();
    coordinator.poll();
    if recovery_before_helper {
        assert_eq!(
            ccd.applied_paths().len(),
            apply_count,
            "已关闭状态不能再次 APPLY"
        );
    }
    assert!(!SessionPaths::release(&dir.0).exists());
    assert!(!SessionPaths::result(&dir.0).exists());
    assert_eq!(&*helper.borrow(), &["enable"]);
    assert!(!ccd.rows().iter().any(|row| row.is_physical() && row.active));
    assert!(ccd
        .rows()
        .iter()
        .any(|row| row.is_bundled_vdd() && row.active));
    assert!(!ccd
        .applied_paths()
        .iter()
        .any(|paths| both_physical_targets_active(paths)));
    let hb = coordinator.heartbeat.as_ref().unwrap();
    let intent: IntentFile = JsonUtil::read(SessionPaths::intent(&dir.0)).unwrap();
    assert_eq!(hb.processed_request_id, intent.request_id);
    assert_eq!(hb.state, RecoveryState::Holding);
    assert!(hb
        .screens
        .iter()
        .all(|screen| screen.confirmed == ConfirmedState::Off));
    let events = std::fs::read_to_string(SessionPaths::events(&dir.0)).unwrap();
    if recovery_before_helper {
        assert!(events.contains("keep-off-precheck-already-off"));
        assert!(events.contains("early_request=Some("));
        assert!(events.contains("disabled_count=0 remaining_active=1 used_apply=false"));
    } else {
        assert!(!events.contains("keep-off-precheck-already-off"));
    }

    assert!(coordinator.restore_all().is_none());
    worker.borrow_mut().as_mut().unwrap().tick();
    coordinator.poll();
    assert!(ccd
        .rows()
        .iter()
        .filter(|row| row.is_physical())
        .all(|row| row.active));
    assert!(!ccd
        .rows()
        .iter()
        .any(|row| row.is_bundled_vdd() && row.active));
    assert_eq!(&*helper.borrow(), &["enable", "disable"]);
    let result: ResultFile = JsonUtil::read(SessionPaths::result(&dir.0)).unwrap();
    assert_eq!(result.restore_state, RestoreState::Complete);
    assert!(result.restored_targets);
}

#[test]
fn second_close_race_external_then_internal() {
    exercise_second_close_race(1, SecondCloseSchedule::RecoveryFirst);
}

#[test]
fn second_close_race_internal_then_external() {
    exercise_second_close_race(0, SecondCloseSchedule::RecoveryFirst);
}

#[test]
fn second_close_race_helper_returns_before_recovery() {
    exercise_second_close_race(1, SecondCloseSchedule::HelperFirst);
}

#[test]
fn second_close_race_rejects_stale_request() {
    exercise_second_close_race(1, SecondCloseSchedule::StaleRequest);
}

#[test]
fn second_close_race_rejects_different_targets() {
    exercise_second_close_race(1, SecondCloseSchedule::DifferentTargets);
}

#[test]
fn second_close_race_rejects_no_remaining_path() {
    exercise_second_close_race(1, SecondCloseSchedule::NoRemainingPath);
}

#[test]
fn second_close_race_rejects_validation_failure() {
    exercise_second_close_race(1, SecondCloseSchedule::ValidateFailure);
}

#[test]
fn second_close_race_rejects_helper_failure() {
    exercise_second_close_race(1, SecondCloseSchedule::HelperFailure);
}

#[test]
fn second_close_race_rejects_missing_virtual_path() {
    exercise_second_close_race(1, SecondCloseSchedule::MissingVirtualPath);
}

#[test]
fn second_close_race_honors_cancellation_after_helper() {
    exercise_second_close_race(1, SecondCloseSchedule::Cancelled);
}

#[test]
fn second_close_race_rejects_restore_failed_heartbeat() {
    exercise_second_close_race(1, SecondCloseSchedule::RestoreFailed);
}

#[test]
fn already_off_without_early_request_is_not_accepted() {
    let ccd = internal_plus_vdd();
    let identity = ccd.rows()[0].identity();
    ccd.deactivate_path(0);
    let helper = Rc::new(RefCell::new(Vec::new()));
    let mut coordinator = RecoveryCoordinator::new(
        Box::new(ccd.clone()),
        coord_hooks(
            Rc::new(RefCell::new(String::new())),
            helper.clone(),
            true,
            true,
            Some(true),
            Duration::from_secs(2),
        ),
    );
    let error = coordinator.keep_off(identity);
    assert!(error
        .as_deref()
        .is_some_and(|error| error.contains("校验 0")));
    assert!(!coordinator.has_session());
    assert!(helper.borrow().is_empty());
    assert!(!ccd.applied());
}
