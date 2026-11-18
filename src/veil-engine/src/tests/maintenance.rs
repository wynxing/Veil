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
fn atomic_write_failure_is_bounded_and_preserves_destination() {
    let dir = TempSession::new();
    let path = dir.0.join("locked.json");
    JsonUtil::write_atomic(&path, &complete_result()).unwrap();
    use std::os::windows::fs::OpenOptionsExt;
    let handle = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(&path)
        .unwrap();
    let start = std::time::Instant::now();
    assert!(JsonUtil::write_atomic(&path, &ResultFile::default()).is_err());
    assert!(start.elapsed() < Duration::from_secs(2));
    assert!(JsonUtil::read::<ResultFile>(&path)
        .unwrap()
        .restoration_outcome()
        .is_ok());
    drop(handle);
    assert_eq!(std::fs::read_dir(&dir.0).unwrap().count(), 1);
}
#[test]
fn uninstall_skips_legacy_keep_off_results() {
    let root = TempSession::new();
    let dir = root.0.join("session-legacy");
    std::fs::create_dir(&dir).unwrap();
    std::fs::write(
        SessionPaths::result(&dir),
        r#"{"ok":true,"reason":"release","restoreRc":0,"restoredTargets":true}"#,
    )
    .unwrap();
    OpenSessionRelease::wait_after_release(&root.0, Duration::ZERO).unwrap();
}
#[test]
fn uninstall_skips_abandoned_pending_sessions() {
    let root = TempSession::new();
    let dir = root.0.join("session-abandoned");
    std::fs::create_dir(&dir).unwrap();
    JsonUtil::write_atomic(
        SessionPaths::metadata(&dir),
        &SessionMetadata {
            protocol_version: PROTOCOL_VERSION,
            ..Default::default()
        },
    )
    .unwrap();
    JsonUtil::write_atomic(
        SessionPaths::ready(&dir),
        &ReadyFile {
            pid: 1_999_999_999,
            hotkey_registered: true,
            hotkey: CcdConstants::HOTKEY_TEXT.into(),
        },
    )
    .unwrap();
    OpenSessionRelease::wait_after_release(&root.0, Duration::ZERO).unwrap();
}
#[test]
fn sweep_concluded_removes_history_and_keeps_live() {
    let root = TempSession::new();
    let legacy = root.0.join("session-legacy");
    let abandoned = root.0.join("session-abandoned");
    let live = root.0.join("session-live");
    for dir in [&legacy, &abandoned, &live] {
        std::fs::create_dir(dir).unwrap();
    }
    std::fs::write(
        SessionPaths::result(&legacy),
        r#"{"ok":true,"reason":"release"}"#,
    )
    .unwrap();
    JsonUtil::write_atomic(
        SessionPaths::metadata(&abandoned),
        &SessionMetadata {
            protocol_version: PROTOCOL_VERSION,
            ..Default::default()
        },
    )
    .unwrap();
    JsonUtil::write_atomic(
        SessionPaths::metadata(&live),
        &SessionMetadata {
            protocol_version: PROTOCOL_VERSION,
            ..Default::default()
        },
    )
    .unwrap();
    write_live_ready(&live);
    assert_eq!(OpenSessionRelease::sweep_concluded(&root.0).unwrap(), 2);
    assert!(!legacy.exists());
    assert!(!abandoned.exists());
    assert!(live.exists());
}
#[test]
fn uninstall_timeout_returns_protocol_exit_code() {
    let root = TempSession::new();
    let dir = root.0.join("session-pending");
    std::fs::create_dir(&dir).unwrap();
    JsonUtil::write_atomic(
        SessionPaths::metadata(&dir),
        &SessionMetadata {
            protocol_version: PROTOCOL_VERSION,
            ..Default::default()
        },
    )
    .unwrap();
    write_live_ready(&dir);
    assert_eq!(
        OpenSessionRelease::wait_after_release(&root.0, Duration::ZERO)
            .unwrap_err()
            .exit_code(),
        2
    );
}
#[test]
fn missing_protocol_fields_are_not_defaulted_to_success() {
    let result: ResultFile =
        serde_json::from_str(r#"{"ok":true,"restoreRc":0,"restoredTargets":true}"#).unwrap();
    assert!(result.restoration_outcome().is_err());
    let mut result = complete_result();
    result.ok = false;
    assert!(
        result.restoration_outcome().is_ok(),
        "operation failure is independent of restoration"
    );
}
#[test]
fn maintenance_gate_blocks_true_and_unknown_states() {
    assert!(crate::maintenance::require_available(Ok(true)).is_err());
    assert!(crate::maintenance::require_available(Err("access denied".into())).is_err());
    assert!(crate::maintenance::require_available(Ok(false)).is_ok());
}
#[test]
fn uninstall_actions_check_results_before_removal_and_hold_marker_through_commit() {
    let xml = include_str!("../../../../installer/Veil.Setup/Package.wxs");
    assert!(xml.contains("Id=\"RestoreDisplays\""));
    assert!(xml.contains("Condition=\"REMOVE=&quot;ALL&quot; AND NOT UPGRADINGPRODUCTCODE\""));
    assert!(xml.contains("Id=\"InstallVdd\""));
    assert!(xml.contains("ExeCommand=\"install-driver\""));
    assert!(xml.contains("Return=\"ignore\""));
    assert!(xml.contains("ComponentGroupRef Id=\"VddFiles\""));
    assert!(xml.contains("Feature Id=\"App\""));
    assert!(!xml.contains("Feature Id=\"BundledVdd\""));
    assert!(xml.contains("Condition=\"NOT Installed AND INSTALLVDD=1\""));
    assert!(xml.contains("Action=\"UninstallVdd\" After=\"RestoreDisplays\""));
    assert!(xml.contains("Action=\"RestoreDisplays\" After=\"BeginMaintenance\""));
    assert!(xml.contains("Id=\"SweepHistoricalSessions\""));
    assert!(xml.contains("Before=\"RemoveExistingProducts\""));
    assert!(xml.contains("WIX_UPGRADE_DETECTED"));
    assert!(xml.contains("Execute=\"commit\""));
    assert!(xml.contains("Execute=\"rollback\""));
    let bundle = include_str!("../../../../installer/Veil.Bundle/Bundle.wxs");
    assert!(bundle.contains("Id=\"RetireOldVeil\""));
    assert!(bundle.contains("retire-old"));
    assert!(bundle.contains("[InstallFolder]\\Veil.App.exe"));
}
#[test]
fn uninstall_waits_for_acknowledgement_of_new_release() {
    let root = TempSession::new();
    let dir = root.0.join("session-retry");
    std::fs::create_dir(&dir).unwrap();
    JsonUtil::write_atomic(
        SessionPaths::metadata(&dir),
        &SessionMetadata {
            protocol_version: PROTOCOL_VERSION,
            ..Default::default()
        },
    )
    .unwrap();
    write_live_ready(&dir);
    JsonUtil::write_atomic(
        SessionPaths::heartbeat(&dir),
        &HeartbeatFile {
            state: RecoveryState::RestoreFailed,
            processed_request_id: 1,
            ..Default::default()
        },
    )
    .unwrap();
    let writer = dir.clone();
    let worker = std::thread::spawn(move || {
        while !SessionPaths::release(&writer).exists() {
            std::thread::sleep(Duration::from_millis(5));
        }
        JsonUtil::write_atomic(SessionPaths::result(&writer), &complete_result()).unwrap();
    });
    assert!(OpenSessionRelease::wait_after_release(&root.0, Duration::from_secs(2)).is_ok());
    worker.join().unwrap();
}
#[test]
fn operation_lock_serializes_independent_threads() {
    let name = format!("Local\\Veil.Test.{}", uuid_like());
    let guard = crate::maintenance::OperationLock::named(&name).unwrap();
    let (sender, receiver) = std::sync::mpsc::channel();
    let thread = std::thread::spawn(move || {
        let _guard = crate::maintenance::OperationLock::named(&name).unwrap();
        sender.send(()).unwrap();
    });
    assert!(receiver.recv_timeout(Duration::from_millis(40)).is_err());
    drop(guard);
    receiver.recv_timeout(Duration::from_secs(2)).unwrap();
    thread.join().unwrap();
}
#[test]
fn restore_session_selection_rejects_service_and_ambiguous_users() {
    use crate::maintenance::select_restore_session;
    assert!(select_restore_session(&[]).is_err());
    assert!(select_restore_session(&[0]).is_err());
    assert!(select_restore_session(&[1, 2]).is_err());
    assert_eq!(select_restore_session(&[3]).unwrap(), 3);
    use crate::maintenance::can_act_as_restore_proxy;
    assert!(can_act_as_restore_proxy(0, 1).is_ok());
    assert!(can_act_as_restore_proxy(1, 1).is_ok());
    assert!(can_act_as_restore_proxy(2, 1).is_err());
}
