use crate::fakes;
use crate::native::{CcdConstants, DisplayConfigModeInfo, DisplayConfigSourceMode, PointL};
use crate::{
    resolve_vdd_resolution, BundledVddSettings, CcdFrame, DisplaySnapshot, PathRole,
    ScreenIdentity, VddModePreference, VddResolutionChoice,
};

fn uuid_like() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}
fn source_mode_info(width: u32, height: u32, x: i32, y: i32) -> DisplayConfigModeInfo {
    let mut mode = DisplayConfigModeInfo::default();
    mode.info_type = CcdConstants::DISPLAYCONFIG_MODE_INFO_TYPE_SOURCE;
    mode.set_source_mode(DisplayConfigSourceMode {
        width,
        height,
        pixel_format: 0,
        position: PointL { x, y },
    });
    mode
}

fn physical_frame(modes: &[(u32, u32, u32, i32, i32)]) -> (CcdFrame, Vec<ScreenIdentity>) {
    let mut rows = Vec::new();
    let mut paths = Vec::new();
    let mut mode_infos = Vec::new();
    let mut ids = Vec::new();
    for (index, (target, width, height, x, y)) in modes.iter().copied().enumerate() {
        let row = fakes::row_simple(
            PathRole::External,
            target,
            &format!("Panel{target}"),
            &format!(r"\\?\DISPLAY#CMN{target}#1"),
        );
        ids.push(row.identity());
        rows.push(row);
        paths.push(fakes::path(
            false,
            true,
            target,
            ((index as u32) << 16) | 0xFFFF,
            1,
        ));
        mode_infos.push(source_mode_info(width, height, x, y));
    }
    (
        CcdFrame {
            paths,
            modes: mode_infos,
            snapshot: DisplaySnapshot::new(rows, 0),
        },
        ids,
    )
}

#[test]
fn xml_for_keeps_one_resolution_and_fallback_text() {
    assert_eq!(
        BundledVddSettings::xml_for(1920, 1200),
        BundledVddSettings::XML
    );
    let xml = BundledVddSettings::xml_for(2560, 1440);
    assert!(xml.contains("<width>2560</width>"));
    assert!(xml.contains("<height>1440</height>"));
    assert!(xml.contains("<refresh_rate>60</refresh_rate>"));
    assert_eq!(xml.matches("<resolution>").count(), 1);
    assert!(BundledVddSettings::owns_text(&xml));
    assert!(!BundledVddSettings::owns_text(
        "<vdd_settings>foreign</vdd_settings>"
    ));
}

#[test]
fn write_resolution_replaces_owned_xml_and_keeps_foreign_files() {
    let root = std::env::temp_dir().join(format!("veil-vdd-resolution-{}", uuid_like()));
    let owned = root.join("owned");
    let foreign_dir = root.join("foreign");
    let _ = std::fs::remove_dir_all(&root);
    BundledVddSettings::write_xml(&[&owned.to_string_lossy()]).unwrap();
    let created =
        BundledVddSettings::write_resolution(&[&owned.to_string_lossy()], 2560, 1440).unwrap();
    assert!(created.is_empty());
    let text = std::fs::read_to_string(owned.join(BundledVddSettings::FILE_NAME)).unwrap();
    assert!(text.contains("<width>2560</width>"));
    assert!(BundledVddSettings::owns_text(&text));
    assert!(BundledVddSettings::try_remove_owned_file(
        &owned.to_string_lossy()
    ));
    std::fs::create_dir_all(&foreign_dir).unwrap();
    let foreign = foreign_dir.join(BundledVddSettings::FILE_NAME);
    std::fs::write(&foreign, "<vdd_settings>foreign</vdd_settings>").unwrap();
    assert!(
        BundledVddSettings::write_resolution(&[&foreign_dir.to_string_lossy()], 2560, 1440)
            .is_err()
    );
    assert_eq!(
        std::fs::read_to_string(&foreign).unwrap(),
        "<vdd_settings>foreign</vdd_settings>"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn resolve_vdd_resolution_follows_selected_physical_mode() {
    let (frame, ids) = physical_frame(&[(1, 2560, 1440, 0, 0)]);
    let choice = resolve_vdd_resolution(&frame, &ids, VddModePreference::MatchPhysical);
    assert_eq!(choice.width, 2560);
    assert_eq!(choice.height, 1440);
    assert!(!choice.fell_back);
}

#[test]
fn resolve_vdd_resolution_prefers_origin_then_largest() {
    let (frame, ids) = physical_frame(&[(1, 2560, 1440, 1920, 0), (2, 1920, 1080, 0, 0)]);
    let choice = resolve_vdd_resolution(&frame, &ids, VddModePreference::MatchPhysical);
    assert_eq!((choice.width, choice.height), (1920, 1080));

    let (frame, ids) = physical_frame(&[(1, 1280, 800, 10, 10), (2, 2560, 1440, 20, 20)]);
    let choice = resolve_vdd_resolution(&frame, &ids, VddModePreference::MatchPhysical);
    assert_eq!((choice.width, choice.height), (2560, 1440));
    assert!(!choice.fell_back);
}

#[test]
fn resolve_vdd_resolution_ignores_screens_not_being_closed() {
    let (frame, ids) = physical_frame(&[(1, 1280, 800, 0, 0), (2, 3840, 2160, 1280, 0)]);
    let choice = resolve_vdd_resolution(&frame, &ids[1..], VddModePreference::MatchPhysical);
    assert_eq!((choice.width, choice.height), (3840, 2160));
}

#[test]
fn resolve_vdd_resolution_falls_back_without_a_usable_mode() {
    let (mut frame, ids) = physical_frame(&[(1, 2560, 1440, 0, 0)]);
    frame.paths[0].source_info.mode_info_idx = 0xFFFF_FFFF;
    let choice = resolve_vdd_resolution(&frame, &ids, VddModePreference::MatchPhysical);
    assert_eq!(choice, VddResolutionChoice::fallback());

    let (frame, ids) = physical_frame(&[(1, 9000, 1000, 0, 0)]);
    let choice = resolve_vdd_resolution(&frame, &ids, VddModePreference::MatchPhysical);
    assert_eq!(choice, VddResolutionChoice::fallback());
}

#[test]
fn resolve_vdd_resolution_uses_fixed_preset_or_falls_back() {
    let (frame, ids) = physical_frame(&[(1, 1920, 1080, 0, 0)]);
    let choice = resolve_vdd_resolution(
        &frame,
        &ids,
        VddModePreference::Fixed {
            width: 2560,
            height: 1440,
        },
    );
    assert_eq!(
        (choice.width, choice.height, choice.fell_back),
        (2560, 1440, false)
    );

    let choice = resolve_vdd_resolution(
        &frame,
        &ids,
        VddModePreference::Fixed {
            width: 100,
            height: 100,
        },
    );
    assert_eq!(choice, VddResolutionChoice::fallback());
}

#[test]
fn vdd_mode_preference_roundtrip_rejects_out_of_range() {
    let root = std::env::temp_dir().join(format!("veil-vdd-pref-{}", uuid_like()));
    let path = root.join("vdd-mode.json");
    let preference = VddModePreference::Fixed {
        width: 2560,
        height: 1600,
    };
    preference.save_to(&path).unwrap();
    assert_eq!(VddModePreference::load_from(&path), preference);
    std::fs::write(&path, r#"{"mode":"fixed","width":100,"height":100}"#).unwrap();
    assert_eq!(
        VddModePreference::load_from(&path),
        VddModePreference::MatchPhysical
    );
    assert_eq!(
        VddModePreference::load_from(root.join("missing.json").as_path()),
        VddModePreference::MatchPhysical
    );
    let _ = std::fs::remove_dir_all(&root);
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

#[test]
fn xml_copy_failure_removes_only_new_files() {
    let root = std::env::temp_dir().join(format!(
        "veil-vdd-xml-copy-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let first = root.join("first");
    let second = root.join("second");
    std::fs::create_dir(&second).unwrap();
    let foreign = second.join(BundledVddSettings::FILE_NAME);
    std::fs::write(&foreign, "foreign").unwrap();
    assert!(
        BundledVddSettings::write_xml(&[&first.to_string_lossy(), &second.to_string_lossy()])
            .is_err()
    );
    assert!(!first.join(BundledVddSettings::FILE_NAME).exists());
    assert_eq!(std::fs::read_to_string(foreign).unwrap(), "foreign");
    let _ = std::fs::remove_dir_all(&root);
}
