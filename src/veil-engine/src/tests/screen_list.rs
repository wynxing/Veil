use crate::fakes::{self};
use crate::session::{HeartbeatFile, HeartbeatScreen};
use crate::*;

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
            wanted: WantedState::KeepOff,
            confirmed: ConfirmedState::Failed,
            detail: "校验 87".into(),
            ..Default::default()
        }],
        ..Default::default()
    };
    let items = ScreenListBuilder::build(&snap, Some(&hb), &[], false, true, true);
    let panel = items.iter().find(|i| i.name == "Panel").unwrap();
    assert_eq!(panel.confirmed, ConfirmedState::Failed);
    assert_ne!(panel.confirmed, ConfirmedState::Off);
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
            wanted: WantedState::KeepOff,
            confirmed: ConfirmedState::Off,
            ..Default::default()
        }],
        ..Default::default()
    };
    let items = ScreenListBuilder::build(&snap, Some(&hb), &[], false, true, true);
    assert!(items
        .iter()
        .any(|i| i.name == "Panel" && i.confirmed == ConfirmedState::Off && i.can_restore));
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
            wanted: WantedState::KeepOff,
            confirmed: ConfirmedState::Off,
            kind: "内置".into(),
            ..Default::default()
        }],
        ..Default::default()
    };
    let items = ScreenListBuilder::build(&snap, Some(&hb), &[], false, true, true);
    let closed = items
        .iter()
        .find(|i| i.confirmed == ConfirmedState::Off)
        .unwrap();
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
            wanted: WantedState::KeepOff,
            confirmed: ConfirmedState::Off,
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
            wanted: WantedState::KeepOff,
            confirmed: ConfirmedState::Off,
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
        assert_eq!(i.wanted, WantedState::On);
        assert_eq!(i.confirmed, ConfirmedState::On);
        assert!(i.can_keep_off);
        assert!(!i.can_restore);
    }
}

#[test]
fn screen_state_enums_keep_protocol_v2_json_format() {
    let raw = r#"{"adapterLuid":"luid","targetId":1,"wanted":"保持关闭","confirmed":"已关闭"}"#;
    let parsed: HeartbeatScreen = serde_json::from_str(raw).unwrap();
    assert_eq!(parsed.wanted, WantedState::KeepOff);
    assert_eq!(parsed.confirmed, ConfirmedState::Off);
    let encoded = serde_json::to_string(&parsed).unwrap();
    assert!(encoded.contains(r#""wanted":"保持关闭""#));
    assert!(encoded.contains(r#""confirmed":"已关闭""#));

    let defaults: HeartbeatScreen = serde_json::from_str("{}").unwrap();
    assert_eq!(defaults.wanted, WantedState::On);
    assert_eq!(defaults.confirmed, ConfirmedState::Unknown);
}
