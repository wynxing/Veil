use crate::fakes::{self, FakeCcd};
use crate::native::{
    CcdAbi, CcdConstants, DisplayConfigModeInfo, DisplayConfigPathInfo,
    DisplayConfigPathSourceInfo, DisplayConfigPathTargetInfo, DisplayConfigSourceMode,
    DisplayConfigVideoSignalInfo, Luid, PointL,
};
use crate::*;

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
