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
    let snap = DisplaySnapshot::new(
        vec![fakes::row_simple(
            PathRole::Internal,
            1,
            "Panel",
            r"\\?\DISPLAY#CMN1540#1",
        )],
        0,
    );
    let plan = Gate::plan_keep_off(
        &snap,
        &[fakes::id(1, "0000000000000001", r"\\?\DISPLAY#CMN1540#1")],
        false,
    );
    assert_eq!(plan.action, KeepOffAction::Blocked);
    assert!(plan.block_reason.unwrap().contains("驱动包"));
    assert!(!plan.needs_bundled_vdd);
    assert!(!plan.may_adjust_clone);
}

#[test]
fn game_viewer_even_with_internal_tech_does_not_unlock_last_physical() {
    let _bundled = override_bundled_instances(vec![]);
    let viewer = Roles::classify(
        false,
        true,
        r"ROOT\DISPLAY\0000",
        r"\\?\DISPLAY#GVV#1",
        "GameViewer",
        r"\\.\DISPLAY3",
    );
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
    let _bundled = override_bundled_instances(vec![]);
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
fn display_instance_identity_is_tested_without_host_pnp_state() {
    let row = fakes::row(
        PathRole::Virtual,
        true,
        2,
        "GameViewer",
        r"ROOT\DISPLAY\0000",
        r"\\?\DISPLAY#GV#1",
        "0000000000000001",
    );
    {
        let _bundled = override_bundled_instances(vec![]);
        assert!(!row.is_bundled_vdd());
    }
    {
        let _bundled = override_bundled_instances(vec![r"ROOT\DISPLAY\0000".into()]);
        assert!(row.is_bundled_vdd());
    }
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
    let snap = DisplaySnapshot::new(
        vec![fakes::row_simple(
            PathRole::Internal,
            1,
            "Panel",
            r"\\?\DISPLAY#CMN1540#1",
        )],
        0,
    );
    let plan = Gate::plan_keep_off(&snap, &[snap.paths[0].identity()], true);
    assert_eq!(plan.action, KeepOffAction::EnableBundledVdd);
    assert!(plan.needs_bundled_vdd);
    assert!(plan.block_reason.unwrap().contains("辅助虚拟输出"));
}

#[test]
fn payload_only_vdd_requests_install() {
    let snap = DisplaySnapshot::new(
        vec![fakes::row_simple(
            PathRole::Internal,
            1,
            "Panel",
            r"\\?\DISPLAY#CMN1540#1",
        )],
        0,
    );
    let plan = Gate::plan_keep_off(
        &snap,
        &[snap.paths[0].identity()],
        BundledVddAvailability::PayloadOnly,
    );
    assert_eq!(plan.action, KeepOffAction::InstallBundledVdd);
    assert!(plan.needs_bundled_vdd);
    assert!(plan.block_reason.unwrap().contains("安装"));
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
    let role = Roles::classify(
        false,
        true,
        r"ROOT\DISPLAY\0000",
        r"\\?\DISPLAY#GVV0001",
        "GameViewer",
        r"\\.\DISPLAY1",
    );
    assert_eq!(role, PathRole::Virtual);
    assert!(!Roles::is_bundled_vdd(
        r"ROOT\DISPLAY\0000",
        r"\\?\DISPLAY#GVV0001",
        "GameViewer"
    ));
}

#[test]
fn placeholder_is_not_physical() {
    let role = Roles::classify(true, false, "", r"\\?\DISPLAY#DEFAULT_MONITOR#1", "", "");
    assert_eq!(role, PathRole::Placeholder);
}

#[test]
fn game_viewer_is_virtual_but_not_bundled() {
    let role = Roles::classify(
        false,
        false,
        r"ROOT\DISPLAY\0000",
        r"\\?\DISPLAY#GVV0001",
        "GameViewer",
        r"\\.\DISPLAY3",
    );
    assert_eq!(role, PathRole::Virtual);
    assert!(!Roles::is_bundled_vdd(
        r"ROOT\DISPLAY\0000",
        r"\\?\DISPLAY#GVV0001",
        "GameViewer"
    ));
}

#[test]
fn mtt_vdd_hardware_path_is_bundled_without_friendly_name() {
    assert!(Roles::is_bundled_vdd(
        r"ROOT#MttVDD\0000",
        r"\\?\DISPLAY#ABC123#1",
        "Generic Monitor"
    ));
    assert!(!Roles::is_bundled_vdd(
        r"ROOT\DISPLAY\0000",
        r"\\?\DISPLAY#GVV0001",
        "VDD by MTT"
    ));
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
    let role = Roles::classify(
        false,
        false,
        r"PCI\VEN_8086",
        r"\\?\DISPLAY#PDA0238#1",
        "S24Q6-Q24G8",
        r"\\.\DISPLAY1",
    );
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
    let result = PathOps::deactivate(
        &original,
        &[],
        &identities,
        &[fakes::id(1, "0000000000000001", "")],
        false,
    )
    .unwrap();
    assert_eq!(result.disabled_count, 1);
    assert_eq!(result.remaining_active, 1);
    assert_eq!(before, struct_bytes(&original));
    assert_eq!(
        result.paths[0].flags & CcdConstants::DISPLAYCONFIG_PATH_ACTIVE,
        0
    );
    assert_eq!(result.paths[0].flags, 8);
    assert_eq!(result.paths[0].source_info.mode_info_idx, 0x0001FFFF);
    assert_eq!(
        struct_bytes(&[original[1]]),
        struct_bytes(&[result.paths[1]])
    );
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
    let (shifted, moved) =
        PathOps::move_remaining_to_origin(&[remaining], &[target_mode, source_mode]);
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
    let identities = vec![
        fakes::id(1, "0000000000000001", ""),
        fakes::id(2, "0000000000000001", ""),
    ];
    let result = PathOps::deactivate(
        &paths,
        &modes,
        &identities,
        &[fakes::id(2, "0000000000000001", "")],
        true,
    )
    .unwrap();
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
    let result = PathOps::deactivate(
        &paths,
        &modes,
        &identities,
        &[fakes::id(1, "0000000000000001", "")],
        true,
    )
    .unwrap();
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
    assert_eq!(
        TopologyBlob::fingerprint(&paths, &modes),
        TopologyBlob::fingerprint(&out_paths, &out_modes)
    );
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
    let result =
        DisplayPlanner::validate_deactivate(&ccd, &[ccd.rows()[0].identity()], true).unwrap();
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
        vec![fakes::row_simple(
            PathRole::Internal,
            1,
            "Panel",
            r"\\?\DISPLAY#CMN1540#1",
        )],
    );
    let result =
        DisplayPlanner::validate_deactivate(&ccd, &[ccd.rows()[0].identity()], false).unwrap();
    assert!(!result.ok());
    assert_eq!(result.remaining_active, 0);
    assert!(ccd.flags().is_empty());
}

#[test]
fn driver_reads_hardcoded_lab_directory() {
    assert_eq!(
        BundledVddSettings::DRIVER_READS_DIRECTORY,
        r"C:\VirtualDisplayDriver"
    );
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
    BundledVddSettings::write_xml(&[&install.to_string_lossy(), &driver.to_string_lossy()])
        .unwrap();
    let a = install.join(BundledVddSettings::FILE_NAME);
    let b = driver.join(BundledVddSettings::FILE_NAME);
    assert!(a.exists());
    assert!(b.exists());
    let a_text = std::fs::read_to_string(&a).unwrap().replace("\r\n", "\n");
    assert_eq!(BundledVddSettings::XML.replace("\r\n", "\n"), a_text);
    assert_eq!(
        std::fs::read_to_string(&a).unwrap(),
        std::fs::read_to_string(&b).unwrap()
    );
    assert!(BundledVddSettings::try_remove_owned_file(
        &install.to_string_lossy()
    ));
    assert!(!a.exists());
    std::fs::write(&b, "<vdd_settings>foreign</vdd_settings>").unwrap();
    assert!(!BundledVddSettings::try_remove_owned_file(
        &driver.to_string_lossy()
    ));
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
    format!(
        "{:x}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    )
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

fn write_keep_internal_off(dir: &std::path::Path, vdd_assist: bool) {
    write_keep_off(dir, &[(1, r"\\?\DISPLAY#CMN#1")], vdd_assist);
}

fn write_keep_off(dir: &std::path::Path, targets: &[(u32, &str)], vdd_assist: bool) {
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

fn arm_with_intent(
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
    assert!(ccd.flags().iter().any(|f| *f == CcdConstants::APPLY_FLAGS));
    assert!(!ccd
        .flags()
        .iter()
        .any(|f| f & CcdConstants::SDC_SAVE_TO_DATABASE != 0));
    let heartbeat: HeartbeatFile = JsonUtil::read(SessionPaths::heartbeat(&dir.0)).unwrap();
    assert!(heartbeat.screens.iter().any(|s| s.confirmed == "已关闭"));
    assert!(!heartbeat
        .screens
        .iter()
        .any(|s| s.wanted == "保持关闭" && s.confirmed == "已关闭" && s.detail.contains("失败")));
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
    assert!(heartbeat
        .screens
        .iter()
        .any(|s| s.target_id == 1 && s.wanted == "开启" && s.confirmed == "已显示"));
    assert!(heartbeat
        .screens
        .iter()
        .any(|s| s.target_id == 2 && s.wanted == "保持关闭" && s.confirmed == "已关闭"));
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
fn last_physical_with_bundled_vdd_keeps_button_and_explains() {
    let snap = DisplaySnapshot::new(
        vec![fakes::row_simple(
            PathRole::Internal,
            1,
            "Panel",
            r"\\?\DISPLAY#CMN#1",
        )],
        0,
    );
    let items = ScreenListBuilder::build(&snap, None, &[], true, true, true);
    assert!(items[0].can_keep_off);
    assert!(items[0].block_reason.contains("辅助虚拟输出"));
}

#[test]
fn last_physical_with_payload_only_keeps_button_and_explains_install() {
    let snap = DisplaySnapshot::new(
        vec![fakes::row_simple(
            PathRole::Internal,
            1,
            "Panel",
            r"\\?\DISPLAY#CMN#1",
        )],
        0,
    );
    let items = ScreenListBuilder::build(
        &snap,
        None,
        &[],
        BundledVddAvailability::PayloadOnly,
        true,
        true,
    );
    assert!(items[0].can_keep_off);
    assert!(items[0].block_reason.contains("安装"));
}

#[test]
fn auxiliary_install_item_hides_when_device_present() {
    let installed = AuxiliaryInstallItem::from_availability(true);
    assert!(!installed.visible);
    let payload = AuxiliaryInstallItem::from_availability(BundledVddAvailability::PayloadOnly);
    assert!(payload.visible);
    assert!(payload.enabled);
    let missing = AuxiliaryInstallItem::from_availability(BundledVddAvailability::Absent);
    assert!(missing.visible);
    assert!(!missing.enabled);
    assert!(missing.hint.contains("驱动包"));
}

#[test]
fn last_physical_is_disabled_with_reason() {
    let snap = DisplaySnapshot::new(
        vec![fakes::row_simple(
            PathRole::Internal,
            1,
            "Panel",
            r"\\?\DISPLAY#CMN#1",
        )],
        0,
    );
    let items = ScreenListBuilder::build(&snap, None, &[], false, true, true);
    assert!(!items[0].can_keep_off);
    assert!(items[0].block_reason.contains("驱动包"));
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
            ..Default::default()
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
    let snap = DisplaySnapshot::new(
        vec![fakes::row_simple(
            PathRole::External,
            2,
            "S24",
            r"\\?\DISPLAY#PDA#1",
        )],
        0,
    );
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
    assert!(items
        .iter()
        .any(|i| i.name == "Panel" && i.confirmed == "已关闭" && i.can_restore));
    assert!(items.iter().any(|i| i.name == "S24"));
}

#[test]
fn display_name_does_not_use_device_instance_path() {
    let row = fakes::row(
        PathRole::Internal,
        false,
        1,
        r"\\?\DISPLAY#CMN1540#4&2ff9cea1&0&UID8388688#{e6f07b5f-ee97-4a90-a60c-1898093096a0}",
        r"PCI\VEN_8086",
        r"\\?\DISPLAY#CMN1540#4&2ff9cea1&0&UID8388688#{e6f07b5f-ee97-4a90-a60c-1898093096a0}",
        "0000000000000001",
    );
    assert_eq!(row.display_name(), "CMN1540");
    assert_eq!(
        resolved_screen_name(
            None,
            None,
            Some(r"\\?\DISPLAY#CMN1540#4&2ff9cea1&0&UID8388688#{guid}"),
            r"\\?\DISPLAY#CMN1540#4&2ff9cea1&0&UID8388688#{guid}",
            true
        ),
        "CMN1540"
    );
    assert_eq!(
        resolved_screen_name(None, None, Some("内置屏"), r"\\?\DISPLAY#CMN1540#1", true),
        "内置屏"
    );
}

#[test]
fn closed_screen_raw_heartbeat_name_is_sanitized() {
    let snap = DisplaySnapshot::new(
        vec![fakes::row_simple(
            PathRole::External,
            2,
            "S24",
            r"\\?\DISPLAY#PDA#1",
        )],
        0,
    );
    let hb = HeartbeatFile {
        screens: vec![HeartbeatScreen {
            adapter_luid: "0000000000000001".into(),
            target_id: 1,
            monitor_path: r"\\?\DISPLAY#CMN1540#4&2ff9cea1&0&UID8388688#{e6f07b5f-ee97-4a90-a60c-1898093096a0}".into(),
            name: r"\\?\DISPLAY#CMN1540#4&2ff9cea1&0&UID8388688#{e6f07b5f-ee97-4a90-a60c-1898093096a0}".into(),
            wanted: "保持关闭".into(),
            confirmed: "已关闭".into(),
            kind: "内置".into(),
            ..Default::default()
        }],
        ..Default::default()
    };
    let items = ScreenListBuilder::build(&snap, Some(&hb), &[], false, true, true);
    let closed = items.iter().find(|i| i.confirmed == "已关闭").unwrap();
    assert_eq!(closed.name, "CMN1540");
    assert_eq!(closed.kind, "内置");
    assert!(!closed.name.contains(r"\\?\"));
}

#[test]
fn remaining_external_is_blocked_after_internal_keep_off() {
    let snap = DisplaySnapshot::new(
        vec![fakes::row_simple(
            PathRole::External,
            2,
            "S24",
            r"\\?\DISPLAY#PDA#1",
        )],
        0,
    );
    let hb = HeartbeatFile {
        screens: vec![HeartbeatScreen {
            adapter_luid: "0000000000000001".into(),
            target_id: 1,
            monitor_path: r"\\?\DISPLAY#CMN1540#1".into(),
            name: "CMN1540".into(),
            wanted: "保持关闭".into(),
            confirmed: "已关闭".into(),
            kind: "内置".into(),
            ..Default::default()
        }],
        ..Default::default()
    };
    let items = ScreenListBuilder::build(&snap, Some(&hb), &[], false, true, true);
    let external = items.iter().find(|i| i.name == "S24").unwrap();
    assert!(!external.can_keep_off);
    assert!(external.block_reason.contains("第二活动目标"));
}

#[test]
fn remaining_external_with_payload_offers_install() {
    let snap = DisplaySnapshot::new(
        vec![fakes::row_simple(
            PathRole::External,
            2,
            "S24",
            r"\\?\DISPLAY#PDA#1",
        )],
        0,
    );
    let hb = HeartbeatFile {
        screens: vec![HeartbeatScreen {
            adapter_luid: "0000000000000001".into(),
            target_id: 1,
            monitor_path: r"\\?\DISPLAY#CMN1540#1".into(),
            name: "CMN1540".into(),
            wanted: "保持关闭".into(),
            confirmed: "已关闭".into(),
            kind: "内置".into(),
            ..Default::default()
        }],
        ..Default::default()
    };
    let items = ScreenListBuilder::build(
        &snap,
        Some(&hb),
        &[],
        BundledVddAvailability::PayloadOnly,
        true,
        true,
    );
    let external = items.iter().find(|i| i.name == "S24").unwrap();
    assert!(external.can_keep_off);
    assert!(external.block_reason.contains("安装"));
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

fn publish_release(dir: &std::path::Path) {
    JsonUtil::write_atomic(
        SessionPaths::release(dir),
        &ReleaseFile {
            at: 0.0,
            request_id: crate::session::request_id(),
        },
    )
    .unwrap();
}
fn complete_result() -> ResultFile {
    ResultFile {
        protocol_version: PROTOCOL_VERSION,
        restore_state: RestoreState::Complete,
        restored_targets: true,
        ..Default::default()
    }
}

fn write_live_ready(dir: impl AsRef<std::path::Path>) {
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

fn both_physical_targets_active(paths: &[DisplayConfigPathInfo]) -> bool {
    let active: Vec<u32> = paths
        .iter()
        .filter(|p| p.flags & CcdConstants::DISPLAYCONFIG_PATH_ACTIVE != 0)
        .map(|p| p.target_info.id)
        .collect();
    active.contains(&1) && active.contains(&2)
}

fn bundled_vdd_row(target_id: u32) -> crate::PathRow {
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

const DISPLAY_INSTANCE: &str = r"ROOT\DISPLAY\0002";
const DISPLAY_INSTANCE_ADAPTER: &str =
    r"\\?\ROOT#DISPLAY#0002#{e6f07b5f-ee97-4a90-b076-33f57bf4eaa7}";

fn display_instance_row(active: bool, target_id: u32) -> crate::PathRow {
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

fn assert_activated_vdd_uses_virtual_modes(ccd: &FakeCcd) {
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

fn owned_coordinator(
    disable_rc: i32,
) -> (
    RecoveryCoordinator,
    Rc<FakeCcd>,
    Rc<RefCell<String>>,
    Rc<RefCell<Vec<String>>>,
) {
    owned_coordinator_sequence(vec![disable_rc])
}

fn owned_coordinator_sequence(
    mut disable_codes: Vec<i32>,
) -> (
    RecoveryCoordinator,
    Rc<FakeCcd>,
    Rc<RefCell<String>>,
    Rc<RefCell<Vec<String>>>,
) {
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

#[test]
fn maintenance_gate_blocks_true_and_unknown_states() {
    assert!(crate::maintenance::require_available(Ok(true)).is_err());
    assert!(crate::maintenance::require_available(Err("access denied".into())).is_err());
    assert!(crate::maintenance::require_available(Ok(false)).is_ok());
}

#[test]
fn uninstall_actions_check_results_before_removal_and_hold_marker_through_commit() {
    let xml = include_str!("../../../installer/Veil.Setup/Package.wxs");
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
    let bundle = include_str!("../../../installer/Veil.Bundle/Bundle.wxs");
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
fn xml_copy_failure_removes_only_new_files() {
    let root = TempSession::new();
    let first = root.0.join("first");
    let second = root.0.join("second");
    std::fs::create_dir(&second).unwrap();
    let foreign = second.join(BundledVddSettings::FILE_NAME);
    std::fs::write(&foreign, "foreign").unwrap();
    assert!(
        BundledVddSettings::write_xml(&[&first.to_string_lossy(), &second.to_string_lossy()])
            .is_err()
    );
    assert!(!first.join(BundledVddSettings::FILE_NAME).exists());
    assert_eq!(std::fs::read_to_string(foreign).unwrap(), "foreign");
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
