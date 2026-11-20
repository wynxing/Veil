use crate::capability::DisplaySnapshot;
use crate::native::{
    CcdApi, CcdConstants, CcdFrame, DisplayConfig2DRegion, DisplayConfigDesktopImageInfo,
    DisplayConfigModeInfo, DisplayConfigPathInfo, DisplayConfigRational, DisplayConfigSourceMode,
    DisplayConfigTargetMode, DisplayConfigVideoSignalInfo, PointL, RectL,
};

pub(crate) enum InactiveBundledActivation {
    Applied,
    NotFound,
    Heartbeat {
        detail: String,
        include_selection: bool,
        mark_attempted: bool,
    },
    ApplyBlocked {
        detail: String,
        rc: i32,
    },
    ApplyFailed {
        rc: i32,
    },
}

pub(crate) fn activate_inactive_bundled_path(
    ccd: &dyn CcdApi,
    active: &CcdFrame,
    all: &CcdFrame,
    width: u32,
    height: u32,
    require_available: bool,
) -> InactiveBundledActivation {
    if active.paths.len() != active.snapshot.paths.len()
        || all.paths.len() != all.snapshot.paths.len()
    {
        return InactiveBundledActivation::Heartbeat {
            detail: "路径与描述数量不一致，未改拓扑。".into(),
            include_selection: false,
            mark_attempted: false,
        };
    }
    let pick_bundled = |available: bool| {
        all.snapshot.paths.iter().enumerate().find(|(index, row)| {
            row.is_bundled_vdd()
                && !row.active
                && (all.paths[*index].target_info.target_available != 0) == available
        })
    };
    let Some((bundled_index, _)) = pick_bundled(true).or_else(|| {
        if require_available {
            None
        } else {
            pick_bundled(false)
        }
    }) else {
        return InactiveBundledActivation::NotFound;
    };
    let mut paths: Vec<_> = active
        .paths
        .iter()
        .zip(active.snapshot.paths.iter())
        .filter(|(_, row)| row.active)
        .map(|(path, _)| *path)
        .collect();
    let mut bundled = all.paths[bundled_index];
    bundled.flags |= CcdConstants::DISPLAYCONFIG_PATH_ACTIVE
        | CcdConstants::DISPLAYCONFIG_PATH_SUPPORT_VIRTUAL_MODE;
    let mut modes = active.modes.clone();
    if let Err(error) = attach_active_modes(&mut bundled, &mut modes, width, height) {
        return InactiveBundledActivation::Heartbeat {
            detail: error,
            include_selection: true,
            mark_attempted: false,
        };
    }
    paths.push(bundled);
    let mut validate_rc = match ccd.set(&paths, &modes, CcdConstants::VALIDATE_FLAGS) {
        Ok(rc) => rc,
        Err(error) => {
            return InactiveBundledActivation::Heartbeat {
                detail: error,
                include_selection: true,
                mark_attempted: false,
            };
        }
    };
    if validate_rc == 87 {
        if let Some(path) = paths.last_mut() {
            path.target_info.scaling = CcdConstants::DISPLAYCONFIG_SCALING_PREFERRED;
        }
        validate_rc = match ccd.set(&paths, &modes, CcdConstants::VALIDATE_FLAGS) {
            Ok(rc) => rc,
            Err(error) => {
                return InactiveBundledActivation::Heartbeat {
                    detail: error,
                    include_selection: true,
                    mark_attempted: false,
                };
            }
        };
    }
    if validate_rc != 0 {
        let bundled = paths.last();
        let detail = match bundled {
            Some(path) => format!(
                "激活辅助路径校验 {validate_rc}，活动路径 {}，辅助源 {:08x} 目标 {:08x}，未改拓扑。",
                paths.len(),
                path.source_info.mode_info_idx,
                path.target_info.mode_info_idx
            ),
            None => format!("激活辅助路径校验 {validate_rc}，未改拓扑。"),
        };
        return InactiveBundledActivation::ApplyBlocked {
            detail,
            rc: validate_rc,
        };
    }
    let apply_rc = match ccd.set(&paths, &modes, CcdConstants::APPLY_FLAGS) {
        Ok(rc) => rc,
        Err(error) => {
            return InactiveBundledActivation::Heartbeat {
                detail: error,
                include_selection: true,
                mark_attempted: true,
            };
        }
    };
    if apply_rc != 0 {
        return InactiveBundledActivation::ApplyFailed { rc: apply_rc };
    }
    InactiveBundledActivation::Applied
}

pub(crate) fn keeps_active_bundled_vdd(
    snapshot: &DisplaySnapshot,
    paths: &[DisplayConfigPathInfo],
) -> bool {
    paths.iter().any(|path| {
        path.flags & CcdConstants::DISPLAYCONFIG_PATH_ACTIVE != 0
            && snapshot.paths.iter().any(|row| {
                row.is_bundled_vdd()
                    && row
                        .adapter_luid
                        .eq_ignore_ascii_case(&path.target_info.adapter_id.to_hex())
                    && row.target_id == path.target_info.id
            })
    })
}

pub(crate) fn invalidate_inactive_source_indices(paths: &mut [DisplayConfigPathInfo]) {
    for path in paths.iter_mut() {
        if path.flags & CcdConstants::DISPLAYCONFIG_PATH_ACTIVE != 0 {
            continue;
        }
        path.source_info.mode_info_idx = 0xFFFF_FFFF;
    }
}

fn attach_active_modes(
    path: &mut DisplayConfigPathInfo,
    modes: &mut Vec<DisplayConfigModeInfo>,
    width: u32,
    height: u32,
) -> Result<(), String> {
    if modes.len() > u16::MAX as usize - 3 {
        return Err("模式表已满，无法激活辅助路径。".into());
    }
    let total_w = width + 160;
    let total_h = height + 119;
    let origin_x = modes
        .iter()
        .filter(|mode| mode.info_type == CcdConstants::DISPLAYCONFIG_MODE_INFO_TYPE_SOURCE)
        .map(|mode| {
            let source = mode.source_mode();
            source.position.x.saturating_add(source.width as i32)
        })
        .max()
        .unwrap_or(0);
    let source_index = modes.len() as u32;
    let mut source = DisplayConfigModeInfo {
        info_type: CcdConstants::DISPLAYCONFIG_MODE_INFO_TYPE_SOURCE,
        id: path.source_info.id,
        adapter_id: path.source_info.adapter_id,
        ..DisplayConfigModeInfo::default()
    };
    source.set_source_mode(DisplayConfigSourceMode {
        width,
        height,
        pixel_format: CcdConstants::DISPLAYCONFIG_PIXELFORMAT_32BPP,
        position: PointL { x: origin_x, y: 0 },
    });
    modes.push(source);
    let target_index = modes.len() as u32;
    let mut target = DisplayConfigModeInfo {
        info_type: CcdConstants::DISPLAYCONFIG_MODE_INFO_TYPE_TARGET,
        id: path.target_info.id,
        adapter_id: path.target_info.adapter_id,
        ..DisplayConfigModeInfo::default()
    };
    let signal = DisplayConfigVideoSignalInfo {
        pixel_rate: u64::from(total_w) * u64::from(total_h) * 60,
        h_sync_freq: DisplayConfigRational {
            numerator: total_h * 60,
            denominator: 1,
        },
        v_sync_freq: DisplayConfigRational {
            numerator: 60,
            denominator: 1,
        },
        active_size: DisplayConfig2DRegion {
            cx: width,
            cy: height,
        },
        total_size: DisplayConfig2DRegion {
            cx: total_w,
            cy: total_h,
        },
        video_standard: 255,
        scan_line_ordering: CcdConstants::DISPLAYCONFIG_SCANLINE_ORDERING_PROGRESSIVE,
    };
    target.union.target_mode = DisplayConfigTargetMode {
        target_video_signal_info: signal,
    };
    modes.push(target);
    let desktop_index = modes.len() as u32;
    let mut desktop = DisplayConfigModeInfo {
        info_type: CcdConstants::DISPLAYCONFIG_MODE_INFO_TYPE_DESKTOP_IMAGE,
        id: path.target_info.id,
        adapter_id: path.target_info.adapter_id,
        ..DisplayConfigModeInfo::default()
    };
    let size = PointL {
        x: width as i32,
        y: height as i32,
    };
    let image = RectL {
        left: 0,
        top: 0,
        right: size.x,
        bottom: size.y,
    };
    desktop.union.desktop_image_info = DisplayConfigDesktopImageInfo {
        path_source_size: size,
        desktop_image_region: image,
        desktop_image_clip: image,
    };
    modes.push(desktop);
    path.source_info.mode_info_idx = (source_index << 16) | 0xFFFF;
    path.target_info.mode_info_idx = (target_index << 16) | (desktop_index & 0xFFFF);
    if path.target_info.rotation == 0 {
        path.target_info.rotation = 1;
    }
    path.target_info.scaling = CcdConstants::DISPLAYCONFIG_SCALING_IDENTITY;
    path.target_info.scan_line_ordering = CcdConstants::DISPLAYCONFIG_SCANLINE_ORDERING_PROGRESSIVE;
    path.target_info.refresh_rate = DisplayConfigRational {
        numerator: 60,
        denominator: 1,
    };
    Ok(())
}
