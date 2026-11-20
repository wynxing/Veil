use crate::fakes::{self, FakeCcd, FakeHotkey, RcHotkey, SharedClock, SharedParent};
use crate::native::{CcdConstants, DisplayConfigModeInfo, DisplayConfigPathInfo, Hotkey};
use crate::session::{
    ArmFile, IntentFile, JsonUtil, ReadyFile, ReleaseFile, ResultFile, ScreenIdentityDto,
    SessionPaths,
};
use crate::session::{RestoreState, PROTOCOL_VERSION};
use crate::*;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

pub(super) struct TempSession(pub(super) PathBuf);
impl TempSession {
    pub(super) fn new() -> Self {
        let p = std::env::temp_dir().join(format!("veil-test-{}", uuid_like()));
        std::fs::create_dir_all(&p).unwrap();
        Self(p)
    }
}
impl Drop for TempSession {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
pub(super) fn uuid_like() -> String {
    format!(
        "{:x}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    )
}
pub(super) fn dual_physical() -> Rc<FakeCcd> {
    Rc::new(FakeCcd::with_paths_rows(
        vec![fakes::path_default(true, 1), fakes::path_default(false, 2)],
        vec![DisplayConfigModeInfo {
            info_type: 1,
            ..Default::default()
        }],
        vec![
            fakes::row_simple(PathRole::Internal, 1, "Panel", r"\\?\DISPLAY#CMN#1"),
            fakes::row_simple(PathRole::External, 2, "S24", r"\\?\DISPLAY#PDA#1"),
        ],
    ))
}
pub(super) fn three_physical() -> Rc<FakeCcd> {
    Rc::new(FakeCcd::with_paths_rows(
        vec![
            fakes::path_default(true, 1),
            fakes::path_default(false, 2),
            fakes::path_default(false, 3),
        ],
        vec![DisplayConfigModeInfo {
            info_type: 1,
            ..Default::default()
        }],
        vec![
            fakes::row_simple(PathRole::Internal, 1, "Panel", r"\\?\DISPLAY#CMN#1"),
            fakes::row_simple(PathRole::External, 2, "S24", r"\\?\DISPLAY#PDA#1"),
            fakes::row_simple(PathRole::External, 3, "S27", r"\\?\DISPLAY#DEL#1"),
        ],
    ))
}
pub(super) fn internal_plus_vdd() -> Rc<FakeCcd> {
    Rc::new(FakeCcd::with_paths_rows(
        vec![fakes::path_default(true, 1), fakes::path_default(false, 2)],
        vec![DisplayConfigModeInfo {
            info_type: 1,
            ..Default::default()
        }],
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
    ))
}
pub(super) fn save_topology(dir: &std::path::Path, ccd: &FakeCcd) {
    TopologyBlob::save(SessionPaths::topology(dir), &ccd.paths(), &ccd.modes()).unwrap();
    TopologyBlob::save(SessionPaths::baseline(dir), &ccd.paths(), &ccd.modes()).unwrap();
    JsonUtil::write_atomic(
        SessionPaths::metadata(dir),
        &crate::session::SessionMetadata {
            protocol_version: crate::session::PROTOCOL_VERSION,
            physical_targets: ccd
                .rows()
                .iter()
                .filter(|p| p.is_physical() && p.active)
                .map(|p| ScreenIdentityDto::from_identity(&p.identity()))
                .collect(),
            vdd_owned: false,
        },
    )
    .unwrap();
}
pub(super) fn write_keep_internal_off(dir: &std::path::Path, vdd_assist: bool) {
    write_keep_off(dir, &[(1, r"\\?\DISPLAY#CMN#1")], vdd_assist);
}
pub(super) fn write_keep_off(dir: &std::path::Path, targets: &[(u32, &str)], vdd_assist: bool) {
    JsonUtil::write_atomic(
        SessionPaths::intent(dir),
        &IntentFile {
            request_id: crate::session::request_id(),
            keep_off: targets
                .iter()
                .map(|(id, mon)| ScreenIdentityDto {
                    adapter_luid: "0000000000000001".into(),
                    target_id: *id,
                    monitor_path: (*mon).into(),
                })
                .collect(),
            vdd_assist,
        },
    )
    .unwrap();
}
pub(super) fn options(
    dir: PathBuf,
    ccd: Rc<FakeCcd>,
    hotkey: FakeHotkey,
    self_pid: i32,
    parent: Rc<SharedParent>,
    clock: Rc<SharedClock>,
) -> RecoveryOptions {
    options_ex(dir, ccd, Box::new(hotkey), self_pid, parent, clock, 20.0)
}
pub(super) fn options_ex(
    dir: PathBuf,
    ccd: Rc<FakeCcd>,
    hotkey: Box<dyn Hotkey>,
    self_pid: i32,
    parent: Rc<SharedParent>,
    clock: Rc<SharedClock>,
    vdd_wait_seconds: f64,
) -> RecoveryOptions {
    RecoveryOptions {
        restore_only: false,
        directory: dir,
        self_pid,
        parent_pid: 22,
        ccd: Box::new(ccd),
        hotkey,
        clock: Box::new(clock),
        parent: Box::new(parent),
        power: Box::new(crate::fakes::FakePower::new()),
        arm_timeout_seconds: 10.0,
        gap_seconds: 3.0,
        reapply_settle_attempts: 4,
        reapply_settle_pause: Duration::from_millis(0),
        pause: Box::new(|_| {}),
        vdd_wait_seconds,
    }
}
pub(super) fn arm_with_intent(
    dir: &std::path::Path,
    ccd: Rc<FakeCcd>,
    parent: Rc<SharedParent>,
    clock: Rc<SharedClock>,
    vdd_assist: bool,
) -> RecoverySession {
    let mut session = RecoverySession::new(options(
        dir.to_path_buf(),
        ccd,
        FakeHotkey::new(),
        11,
        parent,
        clock,
    ));
    session.start();
    JsonUtil::write_atomic(SessionPaths::arm(dir), &ArmFile { pid: 11 }).unwrap();
    session.tick();
    write_keep_internal_off(dir, vdd_assist);
    session.tick();
    session
}
pub(super) fn coord_hooks(
    started: Rc<RefCell<String>>,
    helper: Rc<RefCell<Vec<String>>>,
    installed: bool,
    alive: bool,
    confirm: Option<bool>,
    wait: Duration,
) -> RecoveryCoordinatorHooks {
    let started2 = started.clone();
    let helper2 = helper.clone();
    RecoveryCoordinatorHooks {
        start_recovery: Box::new(move |dir, _| {
            *started2.borrow_mut() = dir.to_string();
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
        run_driver_helper: Box::new(move |verb| {
            helper2.borrow_mut().push(verb.into());
            if verb == "disable" && helper2.borrow().iter().filter(|v| *v == "disable").count() > 0
            {
                // default success; tests override by checking after
            }
            0
        }),
        confirm_enable_vdd: confirm
            .map(|ok| Box::new(move |_: &str| ok) as Box<dyn FnMut(&str) -> bool>),
        bundled_vdd_installed: Box::new(move || installed),
        bundled_vdd_payload: Box::new(|| false),
        is_alive: Box::new(move |_| alive),
        virtual_path_wait: wait,
        on_progress: Box::new(|_| {}),
        cancel_requested: Box::new(|| false),
    }
}
#[allow(dead_code)]
pub(super) fn _rc_hotkey() {
    let _ = RcHotkey(Rc::new(RefCell::new(FakeHotkey::new())));
}
pub(super) fn publish_release(dir: &std::path::Path) {
    JsonUtil::write_atomic(
        SessionPaths::release(dir),
        &ReleaseFile {
            at: 0.0,
            request_id: crate::session::request_id(),
        },
    )
    .unwrap();
}
pub(super) fn complete_result() -> ResultFile {
    ResultFile {
        protocol_version: PROTOCOL_VERSION,
        restore_state: RestoreState::Complete,
        restored_targets: true,
        ..Default::default()
    }
}
pub(super) fn write_live_ready(dir: impl AsRef<std::path::Path>) {
    JsonUtil::write_atomic(
        SessionPaths::ready(dir),
        &ReadyFile {
            pid: std::process::id() as i32,
            hotkey_registered: true,
            hotkey: CcdConstants::HOTKEY_TEXT.into(),
        },
    )
    .unwrap();
}
pub(super) fn both_physical_targets_active(paths: &[DisplayConfigPathInfo]) -> bool {
    let active: Vec<u32> = paths
        .iter()
        .filter(|p| p.flags & CcdConstants::DISPLAYCONFIG_PATH_ACTIVE != 0)
        .map(|p| p.target_info.id)
        .collect();
    active.contains(&1) && active.contains(&2)
}
pub(super) fn bundled_vdd_row(target_id: u32) -> crate::PathRow {
    fakes::row(
        PathRole::Virtual,
        true,
        target_id,
        "VDD by MTT",
        r"ROOT\MttVDD\0000",
        r"\\?\DISPLAY#MTT1337#1",
        "0000000000000001",
    )
}
pub(super) const DISPLAY_INSTANCE: &str = r"ROOT\DISPLAY\0002";
pub(super) const DISPLAY_INSTANCE_ADAPTER: &str =
    r"\\?\ROOT#DISPLAY#0002#{e6f07b5f-ee97-4a90-b076-33f57bf4eaa7}";

pub(super) fn display_instance_row(active: bool, target_id: u32) -> crate::PathRow {
    fakes::row(
        PathRole::Virtual,
        active,
        target_id,
        "Generic Monitor",
        DISPLAY_INSTANCE_ADAPTER,
        r"\\?\DISPLAY#ABC123#1",
        "0000000000000001",
    )
}
pub(super) fn assert_activated_vdd_uses_virtual_modes(ccd: &FakeCcd) {
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
    let desktop_index = (path.target_info.mode_info_idx & 0xFFFF) as usize;
    assert_eq!(path.source_info.mode_info_idx & 0xFFFF, 0xFFFF);
    assert_eq!(
        modes[source_index].info_type,
        CcdConstants::DISPLAYCONFIG_MODE_INFO_TYPE_SOURCE
    );
    assert_eq!(
        modes[source_index].source_mode().pixel_format,
        CcdConstants::DISPLAYCONFIG_PIXELFORMAT_32BPP
    );
    assert_eq!(
        modes[target_index].info_type,
        CcdConstants::DISPLAYCONFIG_MODE_INFO_TYPE_TARGET
    );
    assert_eq!(
        modes[desktop_index].info_type,
        CcdConstants::DISPLAYCONFIG_MODE_INFO_TYPE_DESKTOP_IMAGE
    );
    let signal = unsafe {
        modes[target_index]
            .union
            .target_mode
            .target_video_signal_info
    };
    assert_ne!(signal.h_sync_freq.numerator, 0);
    assert!(signal.total_size.cx > signal.active_size.cx);
    assert_eq!(signal.active_size.cx, 1920);
    assert_eq!(signal.active_size.cy, 1200);
}
pub(super) type OwnedCoordinatorFixture = (
    RecoveryCoordinator,
    Rc<FakeCcd>,
    Rc<RefCell<String>>,
    Rc<RefCell<Vec<String>>>,
);

pub(super) fn owned_coordinator(disable_rc: i32) -> OwnedCoordinatorFixture {
    owned_coordinator_sequence(vec![disable_rc])
}
pub(super) fn owned_coordinator_sequence(mut disable_codes: Vec<i32>) -> OwnedCoordinatorFixture {
    let started = Rc::new(RefCell::new(String::new()));
    let helper = Rc::new(RefCell::new(Vec::new()));
    let ccd = internal_plus_vdd();
    ccd.deactivate_path(1);
    let mut hooks = coord_hooks(
        started.clone(),
        helper.clone(),
        true,
        true,
        Some(true),
        Duration::from_millis(10),
    );
    let calls = helper.clone();
    let hardware = ccd.clone();
    hooks.run_driver_helper = Box::new(move |verb| {
        calls.borrow_mut().push(verb.into());
        if verb == "enable" {
            hardware.activate_path(1);
            0
        } else {
            if disable_codes.len() > 1 {
                disable_codes.remove(0)
            } else {
                disable_codes[0]
            }
        }
    });
    let mut coordinator = RecoveryCoordinator::new(Box::new(ccd.clone()), hooks);
    assert!(coordinator.keep_off(ccd.rows()[0].identity()).is_none());
    (coordinator, ccd, started, helper)
}
