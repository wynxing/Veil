use crate::fakes::{self, FakeCcd, FakeHotkey, RcHotkey, SharedClock, SharedParent};
use crate::native::{
    CcdAbi, CcdApi, CcdConstants, DisplayConfigModeInfo, DisplayConfigPathInfo, DisplayConfigPathSourceInfo,
    DisplayConfigPathTargetInfo, DisplayConfigSourceMode, DisplayConfigVideoSignalInfo, Hotkey, Luid, PointL,
};
use crate::session::{
    ArmFile, HeartbeatFile, HeartbeatScreen, IntentFile, JsonUtil, OpenSessionRelease, ReadyFile, ReleaseFile,
    ResultFile, ScreenIdentityDto, SessionPaths, VddRequestFile,
};
use crate::*;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

#[test]
fn path_and_mode_sizes_match_probe() {
    assert_eq!(std::mem::size_of::<Luid>(), 8);
    assert_eq!(std::mem::size_of::<DisplayConfigPathSourceInfo>(), 20);
    assert_eq!(std::mem::size_of::<DisplayConfigPathTargetInfo>(), 48);
    assert_eq!(std::mem::size_of::<DisplayConfigPathInfo>(), 72);
    assert_eq!(std::mem::size_of::<DisplayConfigVideoSignalInfo>(), 48);
    assert_eq!(std::mem::size_of::<DisplayConfigModeInfo>(), 64);
    assert_eq!(CcdAbi::MODE_UNION_OFFSET, 16);
    assert_eq!(std::mem::size_of::<usize>(), 8);
    CcdAbi::ensure_expected_layout().unwrap();
}

#[test]
fn query_and_set_flags_match_probe() {
    assert_eq!(CcdConstants::QUERY_FLAGS, 82);
    assert_eq!(CcdConstants::VALIDATE_FLAGS, 164960);
    assert_eq!(CcdConstants::SDC_APPLY, 0x80);
    assert_eq!(CcdConstants::SDC_SAVE_TO_DATABASE, 0x200);
    assert_eq!(CcdConstants::HOTKEY_MODIFIERS, 0x4007);
    assert_eq!(CcdConstants::VK_F10, 0x79);
}

#[test]
fn last_physical_without_bundled_vdd_is_blocked() {
    let snap = DisplaySnapshot::new(vec![fakes::row_simple(PathRole::Internal, 1, "Panel", r"\\?\DISPLAY#CMN1540#1")], 0);
    let plan = Gate::plan_keep_off(&snap, &[fakes::id(1, "0000000000000001", r"\\?\DISPLAY#CMN1540#1")], false);
    assert_eq!(plan.action, KeepOffAction::Blocked);
    assert!(plan.block_reason.unwrap().contains("第二活动目标"));
    assert!(!plan.needs_bundled_vdd);
    assert!(!plan.may_adjust_clone);
}

#[test]
fn game_viewer_even_with_internal_tech_does_not_unlock_last_physical() {
    let viewer = Roles::classify(false, true, r"ROOT\DISPLAY\0000", r"\\?\DISPLAY#GVV#1", "GameViewer", r"\\.\DISPLAY3");
    assert_eq!(viewer, PathRole::Virtual);
    let snap = DisplaySnapshot::new(
        vec![
            fakes::row_simple(PathRole::Internal, 1, "Panel", r"\\?\DISPLAY#CMN1540#1"),
            fakes::row(
                PathRole::Virtual,
                true,
                2,
                "GameViewer",
                r"ROOT\DISPLAY\0000",
                r"\\?\DISPLAY#GVV#1",
                "0000000000000001",
            ),
        ],
        0,
    );
    let plan = Gate::plan_keep_off(&snap, &[snap.paths[0].identity()], false);
    assert_eq!(plan.action, KeepOffAction::Blocked);
    assert!(!snap.physical_screens().any(|p| p.role == PathRole::Virtual));
}

#[test]
fn third_party_virtual_does_not_unlock_last_physical() {
    let snap = DisplaySnapshot::new(
        vec![
            fakes::row_simple(PathRole::Internal, 1, "Panel", r"\\?\DISPLAY#CMN1540#1"),
            fakes::row(
                PathRole::Virtual,
                true,
                2,
                "GameViewer",
                r"ROOT\DISPLAY\0000",
                r"\\?\DISPLAY#GV#1",
                "0000000000000001",
            ),
        ],
        0,
    );
    let plan = Gate::plan_keep_off(&snap, &[snap.paths[0].identity()], false);
    assert_eq!(plan.action, KeepOffAction::Blocked);
    assert!(!snap.has_active_bundled_vdd());
    assert!(snap.has_active_third_party_virtual());
}

#[test]
fn bundled_vdd_allows_closing_all_physical() {
    let snap = DisplaySnapshot::new(
        vec![
            fakes::row_simple(PathRole::Internal, 1, "Panel", r"\\?\DISPLAY#CMN1540#1"),
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
        0,
    );
    let plan = Gate::plan_keep_off(&snap, &[snap.paths[0].identity()], false);
    assert_eq!(plan.action, KeepOffAction::Deactivate);
    assert!(plan.may_adjust_clone);
    assert!(snap.has_active_bundled_vdd());
}

#[test]
fn installed_but_inactive_vdd_requests_enable() {
    let snap = DisplaySnapshot::new(vec![fakes::row_simple(PathRole::Internal, 1, "Panel", r"\\?\DISPLAY#CMN1540#1")], 0);
    let plan = Gate::plan_keep_off(&snap, &[snap.paths[0].identity()], true);
    assert_eq!(plan.action, KeepOffAction::EnableBundledVdd);
    assert!(plan.needs_bundled_vdd);
    assert!(plan.block_reason.unwrap().contains("隐藏虚拟输出"));
}

#[test]
fn two_physical_allows_native_deactivate_without_clone() {
    let snap = DisplaySnapshot::new(
        vec![
            fakes::row_simple(PathRole::Internal, 1, "Panel", r"\\?\DISPLAY#CMN#1"),
            fakes::row_simple(PathRole::External, 2, "S24", r"\\?\DISPLAY#PDA#1"),
        ],
        0,
    );
    let plan = Gate::plan_keep_off(&snap, &[snap.paths[0].identity()], false);
    assert_eq!(plan.action, KeepOffAction::Deactivate);
    assert!(!plan.may_adjust_clone);
    assert!(!plan.needs_bundled_vdd);
    assert_eq!(plan.remaining_physical_active, 1);
}

#[test]
fn physical_list_omits_virtual() {
    let snap = DisplaySnapshot::new(
        vec![
            fakes::row_simple(PathRole::Internal, 1, "Panel", r"\\?\DISPLAY#CMN1540#1"),
            fakes::row(
                PathRole::Virtual,
                true,
                2,
                "VDD by MTT",
                r"ROOT\MttVDD\0000",
                r"\\?\DISPLAY#MTT1337#1",
                "0000000000000001",
            ),
            fakes::row_simple(PathRole::External, 3, "S24", r"\\?\DISPLAY#PDA#1"),
        ],
        0,
    );
    let roles: Vec<_> = snap.physical_screens().map(|p| p.role).collect();
    assert_eq!(roles, vec![PathRole::Internal, PathRole::External]);
}

#[test]
fn virtual_needles_win_over_internal_technology() {
    let role = Roles::classify(false, true, r"ROOT\DISPLAY\0000", r"\\?\DISPLAY#GVV0001", "GameViewer", r"\\.\DISPLAY1");
    assert_eq!(role, PathRole::Virtual);
    assert!(!Roles::is_bundled_vdd(r"ROOT\DISPLAY\0000", r"\\?\DISPLAY#GVV0001", "GameViewer"));
}

#[test]
fn placeholder_is_not_physical() {
    let role = Roles::classify(true, false, "", r"\\?\DISPLAY#DEFAULT_MONITOR#1", "", "");
    assert_eq!(role, PathRole::Placeholder);
}

#[test]
fn game_viewer_is_virtual_but_not_bundled() {
    let role = Roles::classify(false, false, r"ROOT\DISPLAY\0000", r"\\?\DISPLAY#GVV0001", "GameViewer", r"\\.\DISPLAY3");
    assert_eq!(role, PathRole::Virtual);
    assert!(!Roles::is_bundled_vdd(r"ROOT\DISPLAY\0000", r"\\?\DISPLAY#GVV0001", "GameViewer"));
}

#[test]
fn mtt_vdd_hardware_path_is_bundled_without_friendly_name() {
    assert!(Roles::is_bundled_vdd(r"ROOT#MttVDD\0000", r"\\?\DISPLAY#ABC123#1", "Generic Monitor"));
    assert!(!Roles::is_bundled_vdd(r"ROOT\DISPLAY\0000", r"\\?\DISPLAY#GVV0001", "VDD by MTT"));
}

#[test]
fn mtt_vdd_is_bundled_virtual() {
    let role = Roles::classify(
        false,
        false,
        r"ROOT\MttVDD\0000",
        r"\\?\DISPLAY#MTT1337#1",
        "Generic Monitor (VDD by MTT)",
        r"\\.\DISPLAY21",
    );
    assert_eq!(role, PathRole::Virtual);
    assert!(Roles::is_bundled_vdd(
        r"ROOT\MttVDD\0000",
        r"\\?\DISPLAY#MTT1337#1",
        "Generic Monitor (VDD by MTT)"
    ));
}

#[test]
fn external_dp_is_physical() {
    let role = Roles::classify(false, false, r"PCI\VEN_8086", r"\\?\DISPLAY#PDA0238#1", "S24Q6-Q24G8", r"\\.\DISPLAY1");
    assert_eq!(role, PathRole::External);
}

#[test]
fn deactivation_preserves_auxiliary_union_and_original() {
    let original = vec![
        fakes::path_default(true, 1),
        fakes::path_default(false, 2),
        fakes::path(true, false, 3, 0x0001FFFF, 1),
    ];
    let before = struct_bytes(&original);
    let identities = vec![
        fakes::id(1, "0000000000000001", ""),
        fakes::id(2, "0000000000000001", ""),
        fakes::id(3, "0000000000000001", ""),
    ];
    let result = PathOps::deactivate(&original, &[], &identities, &[fakes::id(1, "0000000000000001", "")], false).unwrap();
    assert_eq!(result.disabled_count, 1);
    assert_eq!(result.remaining_active, 1);
    assert_eq!(before, struct_bytes(&original));
    assert_eq!(result.paths[0].flags & CcdConstants::DISPLAYCONFIG_PATH_ACTIVE, 0);
    assert_eq!(result.paths[0].flags, 8);
    assert_eq!(result.paths[0].source_info.mode_info_idx, 0x0001FFFF);
    assert_eq!(struct_bytes(&[original[1]]), struct_bytes(&[result.paths[1]]));
}

#[test]
fn remaining_source_moves_to_origin_without_mutating_input() {
    let remaining = fakes::path(true, true, 1, 1, 1);
    let target_mode = DisplayConfigModeInfo {
        info_type: 2,
        ..Default::default()
    };
    let mut source_mode = DisplayConfigModeInfo {
        info_type: 1,
        ..Default::default()
    };
    source_mode.set_source_mode(DisplayConfigSourceMode {
        width: 1920,
        height: 1200,
        pixel_format: 0,
        position: PointL { x: 2560, y: 12 },
    });
    let original = struct_bytes(&[source_mode]);
    let (shifted, moved) = PathOps::move_remaining_to_origin(&[remaining], &[target_mode, source_mode]);
    assert!(moved);
    assert_eq!(shifted[1].source_mode().position.x, 0);
    assert_eq!(shifted[1].source_mode().position.y, 0);
    assert_eq!(original, struct_bytes(&[source_mode]));
}

#[test]
fn virtual_packed_source_index_moves_to_origin() {
    let remaining = fakes::path(true, true, 1, (1u32 << 16) | 0xFFFF, 1);
    let mut modes = vec![DisplayConfigModeInfo::default(); 2];
    modes[0].info_type = 2;
    modes[1].info_type = 1;
    modes[1].set_source_mode(DisplayConfigSourceMode {
        position: PointL { x: 2560, y: 0 },
        ..Default::default()
    });
    let (shifted, moved) = PathOps::move_remaining_to_origin(&[remaining], &modes);
    assert!(moved);
    assert_eq!(shifted[1].source_mode().position.x, 0);
}

#[test]
fn closing_primary_external_moves_origin() {
    let paths = vec![
        fakes::path(true, true, 1, 0, 1),
        fakes::path(false, true, 2, 1, 1),
    ];
    let mut modes = vec![DisplayConfigModeInfo::default(); 2];
    modes[0].info_type = 1;
    modes[0].set_source_mode(DisplayConfigSourceMode {
        position: PointL { x: 2560, y: 0 },
        ..Default::default()
    });
    modes[1].info_type = 1;
    modes[1].set_source_mode(DisplayConfigSourceMode {
        position: PointL { x: 0, y: 0 },
        ..Default::default()
    });
    let identities = vec![fakes::id(1, "0000000000000001", ""), fakes::id(2, "0000000000000001", "")];
    let result = PathOps::deactivate(&paths, &modes, &identities, &[fakes::id(2, "0000000000000001", "")], true).unwrap();
    assert!(result.adjusted_origin);
    assert_eq!(result.modes[0].source_mode().position.x, 0);
    assert_eq!(result.disabled_count, 1);
    assert_eq!(result.remaining_active, 1);
}

#[test]
fn origin_adjust_moves_only_one_remaining_source() {
    let paths = vec![
        fakes::path(true, true, 1, 0, 1),
        fakes::path(false, true, 2, 1, 1),
        fakes::path(false, true, 3, 2, 2),
    ];
    let mut modes = vec![DisplayConfigModeInfo::default(); 3];
    for (i, x) in [(0, 100), (1, 200), (2, 300)] {
        modes[i].info_type = 1;
        modes[i].set_source_mode(DisplayConfigSourceMode {
            position: PointL { x, y: 0 },
            ..Default::default()
        });
    }
    let identities = vec![
        fakes::id(1, "0000000000000001", ""),
        fakes::id(2, "0000000000000001", ""),
        fakes::id(3, "0000000000000002", ""),
    ];
    let result = PathOps::deactivate(&paths, &modes, &identities, &[fakes::id(1, "0000000000000001", "")], true).unwrap();
    assert!(result.adjusted_origin);
    let moved = result
        .modes
        .iter()
        .filter(|m| m.source_mode().position.x == 0 && m.source_mode().position.y == 0)
        .count();
    assert_eq!(moved, 1);
    assert_eq!(result.remaining_active, 2);
}

#[test]
fn topology_round_trip_preserves_path_bytes() {
    let paths = vec![fakes::path_default(true, 1), fakes::path_default(false, 2)];
    let modes = vec![DisplayConfigModeInfo {
        info_type: 1,
        id: 7,
        ..Default::default()
    }];
    let blob = TopologyBlob::from_arrays(&paths, &modes);
    let (out_paths, out_modes) = blob.to_arrays().unwrap();
    assert_eq!(struct_bytes(&paths), struct_bytes(&out_paths));
    assert_eq!(struct_bytes(&modes), struct_bytes(&out_modes));
    assert_eq!(TopologyBlob::fingerprint(&paths, &modes), TopologyBlob::fingerprint(&out_paths, &out_modes));
}

#[test]
fn topology_size_mismatch_throws() {
    let mut blob = TopologyBlob::from_arrays(&[fakes::path_default(true, 1)], &[]);
    blob.path_count = 2;
    assert!(blob.to_arrays().is_err());
}

#[test]
fn validate_never_sets_apply() {
    let ccd = FakeCcd::with_paths_rows(
        vec![fakes::path_default(true, 1), fakes::path_default(false, 2)],
        vec![],
        vec![
            fakes::row_simple(PathRole::Internal, 1, "Panel", r"\\?\DISPLAY#CMN1540#1"),
            fakes::row_simple(PathRole::External, 2, "S24", r"\\?\DISPLAY#PDA#1"),
        ],
    );
    let result = DisplayPlanner::validate_deactivate(&ccd, &[ccd.rows()[0].identity()], true).unwrap();
    assert!(result.ok());
    assert!(!result.used_apply);
    for flags in ccd.flags() {
        assert_eq!(flags & CcdConstants::SDC_APPLY, 0);
        assert_ne!(flags & CcdConstants::SDC_VALIDATE, 0);
        assert_eq!(flags & CcdConstants::SDC_SAVE_TO_DATABASE, 0);
    }
}

#[test]
fn zero_remaining_does_not_count_as_ok() {
    let ccd = FakeCcd::with_paths_rows(
        vec![fakes::path_default(true, 1)],
        vec![],
        vec![fakes::row_simple(PathRole::Internal, 1, "Panel", r"\\?\DISPLAY#CMN1540#1")],
    );
    let result = DisplayPlanner::validate_deactivate(&ccd, &[ccd.rows()[0].identity()], false).unwrap();
    assert!(!result.ok());
    assert_eq!(result.remaining_active, 0);
    assert!(ccd.flags().is_empty());
}

#[test]
fn driver_reads_hardcoded_lab_directory() {
    assert_eq!(BundledVddSettings::DRIVER_READS_DIRECTORY, r"C:\VirtualDisplayDriver");
    assert_eq!(BundledVddSettings::FILE_NAME, "vdd_settings.xml");
    assert!(BundledVddSettings::XML.contains("<count>1</count>"));
    assert!(BundledVddSettings::XML.contains("<width>1920</width>"));
    assert!(BundledVddSettings::XML.contains("<height>1200</height>"));
}

#[test]
fn write_xml_copies_into_install_and_driver_directories() {
    let root = std::env::temp_dir().join(format!("veil-vdd-settings-{}", std::process::id()));
    let install = root.join("program-vdd");
    let driver = root.join("driver-read");
    let _ = std::fs::remove_dir_all(&root);
    BundledVddSettings::write_xml(&[&install.to_string_lossy(), &driver.to_string_lossy()]).unwrap();
    let a = install.join(BundledVddSettings::FILE_NAME);
    let b = driver.join(BundledVddSettings::FILE_NAME);
    assert!(a.exists());
    assert!(b.exists());
    let a_text = std::fs::read_to_string(&a).unwrap().replace("\r\n", "\n");
    assert_eq!(BundledVddSettings::XML.replace("\r\n", "\n"), a_text);
    assert_eq!(std::fs::read_to_string(&a).unwrap(), std::fs::read_to_string(&b).unwrap());
    assert!(BundledVddSettings::try_remove_owned_file(&install.to_string_lossy()));
    assert!(!a.exists());
    std::fs::write(&b, "<vdd_settings>foreign</vdd_settings>").unwrap();
    assert!(!BundledVddSettings::try_remove_owned_file(&driver.to_string_lossy()));
    assert!(b.exists());
    assert!(BundledVddSettings::write_xml(&[&driver.to_string_lossy()]).is_err());
    let extra = root.join("rollback");
    let created = BundledVddSettings::write_xml(&[&extra.to_string_lossy()]).unwrap();
    assert_eq!(created.len(), 1);
    BundledVddSettings::rollback_created(&created);
    assert!(!extra.join(BundledVddSettings::FILE_NAME).exists());
    let _ = std::fs::remove_dir_all(&root);
}

struct TempSession(PathBuf);
impl TempSession {
    fn new() -> Self {
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
fn uuid_like() -> String {
    format!("{:x}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos())
}

fn dual_physical() -> Rc<FakeCcd> {
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

fn three_physical() -> Rc<FakeCcd> {
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

fn internal_plus_vdd() -> Rc<FakeCcd> {
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

fn save_topology(dir: &std::path::Path, ccd: &FakeCcd) {
    TopologyBlob::save(SessionPaths::topology(dir), &ccd.paths(), &ccd.modes()).unwrap();
}

fn write_keep_internal_off(dir: &std::path::Path, vdd_assist: bool) {
    write_keep_off(dir, &[(1, r"\\?\DISPLAY#CMN#1")], vdd_assist);
}

fn write_keep_off(dir: &std::path::Path, targets: &[(u32, &str)], vdd_assist: bool) {
    JsonUtil::write_atomic(
        SessionPaths::intent(dir),
        &IntentFile {
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

fn options(
    dir: PathBuf,
    ccd: Rc<FakeCcd>,
    hotkey: FakeHotkey,
    self_pid: i32,
    parent: Rc<SharedParent>,
    clock: Rc<SharedClock>,
) -> RecoveryOptions {
    options_ex(dir, ccd, Box::new(hotkey), self_pid, parent, clock, 20.0)
}

fn options_ex(
    dir: PathBuf,
    ccd: Rc<FakeCcd>,
    hotkey: Box<dyn Hotkey>,
    self_pid: i32,
    parent: Rc<SharedParent>,
    clock: Rc<SharedClock>,
    vdd_wait_seconds: f64,
) -> RecoveryOptions {
    RecoveryOptions {
        directory: dir,
        self_pid,
        parent_pid: 22,
        ccd: Box::new(ccd),
        hotkey,
        clock: Box::new(clock),
        parent: Box::new(parent),
        arm_timeout_seconds: 10.0,
        gap_seconds: 3.0,
        reapply_settle_attempts: 4,
        reapply_settle_pause: Duration::from_millis(0),
        pause: Box::new(|_| {}),
        vdd_wait_seconds,
    }
}

fn arm_with_intent(
    dir: &std::path::Path,
    ccd: Rc<FakeCcd>,
    parent: Rc<SharedParent>,
    clock: Rc<SharedClock>,
    vdd_assist: bool,
) -> RecoverySession {
    let mut session = RecoverySession::new(options(dir.to_path_buf(), ccd, FakeHotkey::new(), 11, parent, clock));
    session.start();
    JsonUtil::write_atomic(SessionPaths::arm(dir), &ArmFile { pid: 11 }).unwrap();
    session.tick();
    write_keep_internal_off(dir, vdd_assist);
    session.tick();
    session
}

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
    JsonUtil::write_atomic(SessionPaths::release(&dir.0), &ReleaseFile { at: 1.0 }).unwrap();
    session.tick();
    assert!(session.exited);
    assert!(!ccd.applied());
    assert_eq!(session.result.reason, "release");
    assert!(!session.result.ok);
    assert!(session.result.error.as_deref().unwrap_or("").contains("未改物理屏"));
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
    assert!(session.result.error.as_deref().unwrap_or("").contains("arm PID"));
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
    assert!(ccd.flags().iter().any(|f| *f == CcdConstants::APPLY_FLAGS));
    assert!(!ccd.flags().iter().any(|f| f & CcdConstants::SDC_SAVE_TO_DATABASE != 0));
    let heartbeat: HeartbeatFile = JsonUtil::read(SessionPaths::heartbeat(&dir.0)).unwrap();
    assert!(heartbeat.screens.iter().any(|s| s.confirmed == "已关闭"));
    assert!(!heartbeat.screens.iter().any(|s| s.wanted == "保持关闭" && s.confirmed == "已关闭" && s.detail.contains("失败")));
}

#[test]
fn failed_validate_does_not_apply_and_shows_failure_not_closed() {
    let dir = TempSession::new();
    let ccd = dual_physical();
    ccd.set_validate_rc(87);
    save_topology(&dir.0, &ccd);
    let session = arm_with_intent(&dir.0, ccd.clone(), Rc::new(SharedParent::new()), Rc::new(SharedClock::new(0.0)), false);
    assert!(!ccd.applied());
    let heartbeat: HeartbeatFile = JsonUtil::read(SessionPaths::heartbeat(&dir.0)).unwrap();
    assert!(heartbeat.screens.iter().any(|s| s.confirmed == "失败"));
    assert!(!heartbeat.screens.iter().any(|s| s.confirmed == "已关闭"));
    assert!(!session.exited);
}

#[test]
fn apply_failure_restores() {
    let dir = TempSession::new();
    let ccd = dual_physical();
    ccd.set_next_apply_rc(31);
    save_topology(&dir.0, &ccd);
    let session = arm_with_intent(&dir.0, ccd.clone(), Rc::new(SharedParent::new()), Rc::new(SharedClock::new(0.0)), false);
    assert!(ccd.applied());
    assert_eq!(session.result.restore_rc, Some(0));
    assert!(!session.exited);
}

#[test]
fn parent_exit_restores_and_writes_result() {
    let dir = TempSession::new();
    let ccd = dual_physical();
    save_topology(&dir.0, &ccd);
    let parent = Rc::new(SharedParent::new());
    let mut session = arm_with_intent(&dir.0, ccd, parent.clone(), Rc::new(SharedClock::new(0.0)), false);
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
    let mut session = arm_with_intent(&dir.0, ccd.clone(), Rc::new(SharedParent::new()), Rc::new(SharedClock::new(0.0)), false);
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
fn execution_gap_restores_then_reapplies_once() {
    let dir = TempSession::new();
    let ccd = dual_physical();
    save_topology(&dir.0, &ccd);
    let clock = Rc::new(SharedClock::new(0.0));
    let mut session = arm_with_intent(&dir.0, ccd.clone(), Rc::new(SharedParent::new()), clock.clone(), false);
    let apply_count = ccd.flags().iter().filter(|f| **f == CcdConstants::APPLY_FLAGS).count();
    clock.set(10.0);
    session.tick();
    assert!(session.result.reapply_attempted);
    assert!(ccd.flags().iter().filter(|f| **f == CcdConstants::APPLY_FLAGS).count() > apply_count);
    assert!(!session.exited);
    clock.set(20.0);
    session.tick();
    assert!(session.exited);
    assert_eq!(session.result.reason, "execution-gap");
}

#[test]
fn stale_capture_after_restore_still_reapplies() {
    let dir = TempSession::new();
    let ccd = dual_physical();
    save_topology(&dir.0, &ccd);
    let clock = Rc::new(SharedClock::new(0.0));
    let mut session = arm_with_intent(&dir.0, ccd.clone(), Rc::new(SharedParent::new()), clock.clone(), false);
    let apply_count = ccd.flags().iter().filter(|f| **f == CcdConstants::APPLY_FLAGS).count();
    ccd.set_stale_captures(2);
    clock.set(10.0);
    session.tick();
    assert!(session.result.reapply_attempted);
    assert!(ccd.flags().iter().filter(|f| **f == CcdConstants::APPLY_FLAGS).count() > apply_count);
    assert!(!session.exited);
    let events = std::fs::read_to_string(SessionPaths::events(&dir.0)).unwrap();
    assert!(events.contains("reapply-settle"));
    assert!(events.contains("reapplied"));
    assert!(!events.contains("already-off"));
}

#[test]
fn reapply_without_second_target_requests_bundled_vdd() {
    let dir = TempSession::new();
    let ccd = internal_plus_vdd();
    save_topology(&dir.0, &ccd);
    let clock = Rc::new(SharedClock::new(0.0));
    let mut session = arm_with_intent(&dir.0, ccd.clone(), Rc::new(SharedParent::new()), clock.clone(), true);
    ccd.set_after_apply(|inner| {
        inner.paths.truncate(1);
        inner.rows.truncate(1);
        inner.rows[0].active = true;
    });
    clock.set(10.0);
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
    assert!(std::fs::read_to_string(SessionPaths::events(&dir.0)).unwrap().contains("reapplied"));
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
    assert!(heartbeat.screens.iter().any(|s| s.target_id == 1 && s.wanted == "开启" && s.confirmed == "已显示"));
    assert!(heartbeat.screens.iter().any(|s| s.target_id == 2 && s.wanted == "保持关闭" && s.confirmed == "已关闭"));
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
    let mut session = arm_with_intent(&dir.0, ccd.clone(), Rc::new(SharedParent::new()), Rc::new(SharedClock::new(0.0)), false);
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
    clock.set(10.0);
    session.tick();
    assert!(session.result.reapply_attempted);
    assert!(!session.exited);
    assert!(SessionPaths::vdd_request(&dir.0).exists());
    let apply_count = ccd.flags().iter().filter(|f| **f == CcdConstants::APPLY_FLAGS).count();
    clock.set(12.0);
    session.tick();
    assert!(session.exited);
    assert_eq!(session.result.reason, "execution-gap");
    let later = ccd.flags().iter().filter(|f| **f == CcdConstants::APPLY_FLAGS).count();
    assert!(later <= apply_count + 1, "timeout may restore once, must not loop APPLY");
    let events = std::fs::read_to_string(SessionPaths::events(&dir.0)).unwrap();
    assert!(!events.contains("reapplied"));
    assert!(events.contains("等待自带 VDD 超时"));
}

#[test]
fn request_all_and_wait_returns_when_result_appears() {
    let root = TempSession::new();
    let session_dir = root.0.join("session-wait");
    std::fs::create_dir_all(&session_dir).unwrap();
    let result = SessionPaths::result(&session_dir);
    let writer = session_dir.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(40));
        JsonUtil::write_atomic(
            SessionPaths::result(&writer),
            &ResultFile {
                ok: true,
                reason: "release".into(),
                ..Default::default()
            },
        )
        .unwrap();
    });
    OpenSessionRelease::wait_after_release(&root.0, Duration::from_secs(2));
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
        coord_hooks(started.clone(), helper, false, true, None, Duration::from_secs(15)),
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
    let intent: IntentFile = JsonUtil::read(SessionPaths::intent(started.borrow().as_str())).unwrap();
    assert_eq!(intent.keep_off.len(), 1);
    assert_eq!(intent.keep_off[0].target_id, ids[1].target_id);
    let _ = std::fs::remove_dir_all(started.borrow().as_str());
}

#[test]
fn request_all_and_wait_returns_immediately_without_sessions() {
    let root = TempSession::new();
    OpenSessionRelease::wait_after_release(&root.0, Duration::from_secs(2));
}

#[test]
fn dual_physical_never_uses_clone_flags() {
    let dir = TempSession::new();
    let ccd = dual_physical();
    save_topology(&dir.0, &ccd);
    let _ = arm_with_intent(&dir.0, ccd.clone(), Rc::new(SharedParent::new()), Rc::new(SharedClock::new(0.0)), false);
    assert!(!ccd.flags().iter().any(|f| f & CcdConstants::SDC_TOPOLOGY_CLONE != 0));
}

fn coord_hooks(
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
            if verb == "disable" && helper2.borrow().iter().filter(|v| *v == "disable").count() > 0 {
                // default success; tests override by checking after
            }
            0
        }),
        confirm_enable_vdd: confirm.map(|ok| Box::new(move || ok) as Box<dyn FnMut() -> bool>),
        bundled_vdd_installed: Box::new(move || installed),
        is_alive: Box::new(move |_| alive),
        virtual_path_wait: wait,
    }
}

#[test]
fn format_result_maps_release() {
    assert_eq!(
        RecoveryCoordinator::format_result(Some(&ResultFile {
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
fn poll_enables_bundled_vdd_once_when_reapply_requests_it() {
    let started = Rc::new(RefCell::new(String::new()));
    let helper = Rc::new(RefCell::new(Vec::new()));
    let ccd = internal_plus_vdd();
    let mut coordinator = RecoveryCoordinator::new(
        Box::new(ccd.clone()),
        coord_hooks(started.clone(), helper.clone(), true, true, None, Duration::from_secs(15)),
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
        coord_hooks(started.clone(), helper.clone(), false, true, None, Duration::from_secs(15)),
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
    JsonUtil::write_atomic(
        SessionPaths::heartbeat(started.borrow().as_str()),
        &HeartbeatFile {
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
            }],
        },
    )
    .unwrap();
    JsonUtil::write_atomic(
        SessionPaths::result(started.borrow().as_str()),
        &ResultFile {
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
    assert!(coordinator.status_text.as_deref().unwrap().starts_with("已恢复全部。"));
    assert!(coordinator.status_text.as_deref().unwrap().contains("记录："));
    assert!(!coordinator.hotkey_registered);
    assert!(!helper.borrow().iter().any(|v| v == "disable"));
    let _ = std::fs::remove_dir_all(started.borrow().as_str());
}

#[test]
fn session_result_disables_bundled_vdd_and_shows_failure() {
    let started = Rc::new(RefCell::new(String::new()));
    let helper = Rc::new(RefCell::new(Vec::new()));
    let helper2 = helper.clone();
    let ccd = internal_plus_vdd();
    let mut hooks = coord_hooks(started.clone(), helper.clone(), true, true, None, Duration::from_secs(15));
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
            }],
        },
    )
    .unwrap();
    JsonUtil::write_atomic(
        SessionPaths::result(started.borrow().as_str()),
        &ResultFile {
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
    assert!(helper.borrow().iter().any(|v| v == "disable"));
    assert!(coordinator
        .status_text
        .as_deref()
        .unwrap()
        .contains(RecoveryCoordinator::DISABLE_VDD_FAILED));
    assert!(coordinator.status_text.as_deref().unwrap().contains("Ctrl+Alt+Shift+F10"));
    let _ = std::fs::remove_dir_all(started.borrow().as_str());
}

#[test]
fn cancelled_enable_prompt_does_not_touch_helper_or_screens() {
    let helper = Rc::new(RefCell::new(Vec::new()));
    let ccd = Rc::new(FakeCcd::with_paths_rows(
        vec![fakes::path_default(true, 1)],
        vec![DisplayConfigModeInfo {
            info_type: 1,
            ..Default::default()
        }],
        vec![fakes::row_simple(PathRole::Internal, 1, "Panel", r"\\?\DISPLAY#CMN#1")],
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
    let identity = ccd.query_snapshot(CcdConstants::QUERY_FLAGS).unwrap().physical_screens().next().unwrap().identity();
    assert_eq!(
        coordinator.keep_off(identity).as_deref(),
        Some(RecoveryCoordinator::ENABLE_VDD_CANCELLED)
    );
    assert!(helper.borrow().is_empty());
    assert!(!coordinator.has_session());
    assert_eq!(ccd.query_snapshot(CcdConstants::QUERY_FLAGS).unwrap().active_physical().count(), 1);
}

#[test]
fn dead_recovery_unsticks_and_disables_bundled_vdd() {
    let started = Rc::new(RefCell::new(String::new()));
    let helper = Rc::new(RefCell::new(Vec::new()));
    let ccd = internal_plus_vdd();
    let mut coordinator = RecoveryCoordinator::new(
        Box::new(ccd.clone()),
        coord_hooks(started.clone(), helper.clone(), true, false, None, Duration::from_secs(15)),
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
    assert!(!coordinator.has_session());
    assert!(coordinator.status_text.as_deref().unwrap().starts_with(RecoveryCoordinator::RECOVERY_EXITED));
    assert!(coordinator.status_text.as_deref().unwrap().contains("记录："));
    assert!(helper.borrow().iter().any(|v| v == "disable"));
    let result: ResultFile = JsonUtil::read(SessionPaths::result(started.borrow().as_str())).unwrap();
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
        coord_hooks(started.clone(), helper.clone(), true, true, None, Duration::from_secs(15)),
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
        coord_hooks(started.clone(), helper.clone(), true, false, None, Duration::from_secs(15)),
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
    assert!(!coordinator.has_session());
    assert!(!helper.borrow().iter().any(|v| v == "disable"));
    assert!(coordinator
        .status_text
        .as_deref()
        .unwrap()
        .contains(RecoveryCoordinator::RECOVERY_EXITED_LAST_PATH));
    let _ = std::fs::remove_dir_all(started.borrow().as_str());
}

#[test]
fn confirmed_enable_then_missing_virtual_path_disables() {
    let helper = Rc::new(RefCell::new(Vec::new()));
    let ccd = Rc::new(FakeCcd::with_paths_rows(
        vec![fakes::path_default(true, 1)],
        vec![DisplayConfigModeInfo {
            info_type: 1,
            ..Default::default()
        }],
        vec![fakes::row_simple(PathRole::Internal, 1, "Panel", r"\\?\DISPLAY#CMN#1")],
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
    let identity = ccd.query_snapshot(CcdConstants::QUERY_FLAGS).unwrap().physical_screens().next().unwrap().identity();
    let error = coordinator.keep_off(identity);
    assert_eq!(error.as_deref(), Some("自带 VDD 未能出现活动虚拟路径，物理屏未改动。"));
    assert_eq!(*helper.borrow(), vec!["enable".to_string(), "disable".to_string()]);
    assert!(!coordinator.has_session());
}

#[test]
fn virtual_screens_are_not_listed() {
    let snap = DisplaySnapshot::new(
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
            fakes::row_simple(PathRole::External, 3, "S24", r"\\?\DISPLAY#PDA#1"),
        ],
        0,
    );
    let items = ScreenListBuilder::build(&snap, None, &[], false, true, true);
    let names: Vec<_> = items.iter().map(|i| i.name.as_str()).collect();
    assert_eq!(names, vec!["Panel", "S24"]);
    assert_eq!(items[0].keep_off_automation_id(), "KeepOffButton-1");
    assert_eq!(items[1].restore_automation_id(), "RestoreButton-3");
}

#[test]
fn last_physical_is_disabled_with_reason() {
    let snap = DisplaySnapshot::new(vec![fakes::row_simple(PathRole::Internal, 1, "Panel", r"\\?\DISPLAY#CMN#1")], 0);
    let items = ScreenListBuilder::build(&snap, None, &[], false, true, true);
    assert!(!items[0].can_keep_off);
    assert!(items[0].block_reason.contains("第二活动目标"));
}

#[test]
fn failure_is_never_shown_as_closed() {
    let snap = DisplaySnapshot::new(
        vec![
            fakes::row_simple(PathRole::Internal, 1, "Panel", r"\\?\DISPLAY#CMN#1"),
            fakes::row_simple(PathRole::External, 2, "S24", r"\\?\DISPLAY#PDA#1"),
        ],
        0,
    );
    let hb = HeartbeatFile {
        hotkey_registered: true,
        screens: vec![HeartbeatScreen {
            adapter_luid: "0000000000000001".into(),
            target_id: 1,
            monitor_path: r"\\?\DISPLAY#CMN#1".into(),
            name: "Panel".into(),
            wanted: "保持关闭".into(),
            confirmed: "失败".into(),
            detail: "校验 87".into(),
        }],
        ..Default::default()
    };
    let items = ScreenListBuilder::build(&snap, Some(&hb), &[], false, true, true);
    let panel = items.iter().find(|i| i.name == "Panel").unwrap();
    assert_eq!(panel.confirmed, "失败");
    assert_ne!(panel.confirmed, "已关闭");
    assert!(panel.block_reason.contains("87"));
}

#[test]
fn hotkey_unavailable_blocks_keep_off() {
    let snap = DisplaySnapshot::new(
        vec![
            fakes::row_simple(PathRole::Internal, 1, "Panel", r"\\?\DISPLAY#CMN#1"),
            fakes::row_simple(PathRole::External, 2, "S24", r"\\?\DISPLAY#PDA#1"),
        ],
        0,
    );
    let items = ScreenListBuilder::build(&snap, None, &[], false, true, false);
    assert!(items.iter().all(|i| !i.can_keep_off));
    assert!(items[0].block_reason.contains("热键"));
}

#[test]
fn heartbeat_keeps_closed_screen_when_snapshot_omits_it() {
    let snap = DisplaySnapshot::new(vec![fakes::row_simple(PathRole::External, 2, "S24", r"\\?\DISPLAY#PDA#1")], 0);
    let hb = HeartbeatFile {
        screens: vec![HeartbeatScreen {
            adapter_luid: "0000000000000001".into(),
            target_id: 1,
            monitor_path: r"\\?\DISPLAY#CMN#1".into(),
            name: "Panel".into(),
            wanted: "保持关闭".into(),
            confirmed: "已关闭".into(),
            ..Default::default()
        }],
        ..Default::default()
    };
    let items = ScreenListBuilder::build(&snap, Some(&hb), &[], false, true, true);
    assert!(items.iter().any(|i| i.name == "Panel" && i.confirmed == "已关闭" && i.can_restore));
    assert!(items.iter().any(|i| i.name == "S24"));
}

#[test]
fn cleared_heartbeat_after_restore_shows_active_screens_on() {
    let snap = DisplaySnapshot::new(
        vec![
            fakes::row_simple(PathRole::Internal, 1, "Panel", r"\\?\DISPLAY#CMN#1"),
            fakes::row_simple(PathRole::External, 2, "S24", r"\\?\DISPLAY#PDA#1"),
        ],
        0,
    );
    let items = ScreenListBuilder::build(&snap, None, &[], false, true, true);
    for i in &items {
        assert_eq!(i.wanted, "开启");
        assert_eq!(i.confirmed, "已显示");
        assert!(i.can_keep_off);
        assert!(!i.can_restore);
    }
}

#[allow(dead_code)]
fn _rc_hotkey() {
    let _ = RcHotkey(Rc::new(RefCell::new(FakeHotkey::new())));
}
