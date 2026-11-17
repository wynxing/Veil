use crate::native::{CcdConstants, CcdFrame};
use crate::topology::PathOps;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathRole {
    Internal,
    External,
    Virtual,
    Placeholder,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScreenIdentity {
    pub adapter_luid: String,
    pub target_id: u32,
    pub monitor_path: String,
}

impl ScreenIdentity {
    pub fn new(
        adapter_luid: impl Into<String>,
        target_id: u32,
        monitor_path: impl Into<String>,
    ) -> Self {
        Self {
            adapter_luid: adapter_luid.into(),
            target_id,
            monitor_path: monitor_path.into(),
        }
    }

    pub fn matches(&self, other: &ScreenIdentity) -> bool {
        if !self.adapter_luid.eq_ignore_ascii_case(&other.adapter_luid)
            || self.target_id != other.target_id
        {
            return false;
        }
        if self.monitor_path.is_empty() || other.monitor_path.is_empty() {
            return true;
        }
        self.monitor_path.eq_ignore_ascii_case(&other.monitor_path)
    }
}

impl std::fmt::Display for ScreenIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{}:{}",
            self.adapter_luid, self.target_id, self.monitor_path
        )
    }
}

#[derive(Clone, Debug)]
pub struct PathRow {
    pub index: i32,
    pub active: bool,
    pub flags: u32,
    pub source_id: u32,
    pub target_id: u32,
    pub adapter_luid: String,
    pub output_technology: u32,
    pub internal: bool,
    pub source_name: String,
    pub adapter_path: String,
    pub monitor_name: String,
    pub monitor_path: String,
    pub placeholder: bool,
    pub role: PathRole,
    pub edid_manufacture_id: u16,
    pub edid_product_code_id: u16,
}

impl PathRow {
    pub fn identity(&self) -> ScreenIdentity {
        ScreenIdentity::new(&self.adapter_luid, self.target_id, &self.monitor_path)
    }

    pub fn is_physical(&self) -> bool {
        matches!(self.role, PathRole::Internal | PathRole::External)
    }

    pub fn is_bundled_vdd(&self) -> bool {
        self.role == PathRole::Virtual
            && (Roles::is_bundled_vdd(&self.adapter_path, &self.monitor_path, &self.monitor_name)
                || crate::devices::known_bundled_instance_ids()
                    .iter()
                    .any(|instance| {
                        crate::driver_policy::adapter_matches_bundled_instance(
                            &self.adapter_path,
                            instance,
                        )
                    }))
    }

    pub fn display_name(&self) -> String {
        resolved_screen_name(
            Some(&self.monitor_name),
            Some(&self.source_name),
            None,
            &self.monitor_path,
            false,
        )
    }

    pub fn kind_label(&self) -> &'static str {
        match self.role {
            PathRole::Internal => "内置",
            PathRole::External => "外接",
            _ => "物理",
        }
    }
}

pub fn looks_like_device_path(value: &str) -> bool {
    let text = value.trim();
    if text.is_empty() {
        return false;
    }
    let upper = text.to_ascii_uppercase();
    upper.starts_with(r"\\?\")
        || upper.starts_with(r"\\.\")
        || (upper.contains("DISPLAY#") && (upper.contains('&') || upper.contains('{')))
}

pub fn short_monitor_id(monitor_path: &str) -> Option<String> {
    let upper = monitor_path.to_ascii_uppercase();
    let rest = upper.split("DISPLAY#").nth(1)?;
    let id = rest.split('#').next()?.trim();
    if id.is_empty() || id == "DEFAULT_MONITOR" {
        return None;
    }
    Some(id.to_string())
}

pub fn resolved_screen_name(
    monitor_name: Option<&str>,
    source_name: Option<&str>,
    previous_name: Option<&str>,
    monitor_path: &str,
    omitted: bool,
) -> String {
    for candidate in [monitor_name, source_name, previous_name]
        .into_iter()
        .flatten()
    {
        let text = candidate.trim();
        if text.is_empty() || looks_like_device_path(text) || text == "未命名" {
            continue;
        }
        return text.to_string();
    }
    if let Some(short) = short_monitor_id(monitor_path) {
        return short;
    }
    for candidate in [monitor_name, previous_name].into_iter().flatten() {
        if let Some(short) = short_monitor_id(candidate) {
            return short;
        }
    }
    if omitted {
        "已关闭的物理屏".into()
    } else {
        "未命名".into()
    }
}

#[derive(Clone, Debug)]
pub struct DisplaySnapshot {
    pub paths: Vec<PathRow>,
    pub gdi_monitor_count: i32,
}

impl DisplaySnapshot {
    pub fn new(paths: Vec<PathRow>, gdi_monitor_count: i32) -> Self {
        Self {
            paths,
            gdi_monitor_count,
        }
    }

    pub fn active_paths(&self) -> impl Iterator<Item = &PathRow> {
        self.paths.iter().filter(|p| p.active)
    }

    pub fn physical_screens(&self) -> impl Iterator<Item = &PathRow> {
        self.paths.iter().filter(|p| p.is_physical())
    }

    pub fn active_physical(&self) -> impl Iterator<Item = &PathRow> {
        self.paths.iter().filter(|p| p.active && p.is_physical())
    }

    pub fn has_active_bundled_vdd(&self) -> bool {
        self.paths.iter().any(|p| p.active && p.is_bundled_vdd())
    }

    pub fn has_active_third_party_virtual(&self) -> bool {
        self.paths
            .iter()
            .any(|p| p.active && p.role == PathRole::Virtual && !p.is_bundled_vdd())
    }
}

pub struct Roles;

impl Roles {
    const VIRTUAL_NEEDLES: &'static [&'static str] = &[
        r"root\display",
        "root#display",
        "iddcx",
        "virtual",
        "usbmmidd",
        "virtualdisplay",
        "indirect",
        "idd sample",
        "mtt",
    ];

    pub fn is_internal_technology(output_technology: u32) -> bool {
        matches!(
            output_technology,
            CcdConstants::OUTPUT_TECHNOLOGY_INTERNAL
                | CcdConstants::OUTPUT_TECHNOLOGY_DISPLAY_PORT_EMBEDDED
                | CcdConstants::OUTPUT_TECHNOLOGY_UDI_EMBEDDED
        )
    }

    pub fn looks_virtual(
        adapter_path: &str,
        monitor_path: &str,
        monitor_name: &str,
        source_name: &str,
    ) -> bool {
        let blob =
            format!("{adapter_path} {monitor_path} {monitor_name} {source_name}").to_lowercase();
        Self::VIRTUAL_NEEDLES.iter().any(|n| blob.contains(n))
    }

    pub fn is_bundled_vdd(adapter_path: &str, monitor_path: &str, _monitor_name: &str) -> bool {
        let adapter = adapter_path.to_lowercase();
        let monitor = monitor_path.to_lowercase();
        adapter.contains(r"root\mttvdd")
            || adapter.contains("root#mttvdd")
            || adapter.contains("mttvdd")
            || monitor.contains("mtt1337")
            || monitor.contains("mttvdd")
    }

    pub fn classify(
        placeholder: bool,
        internal_technology: bool,
        adapter_path: &str,
        monitor_path: &str,
        monitor_name: &str,
        source_name: &str,
    ) -> PathRole {
        if placeholder {
            return PathRole::Placeholder;
        }
        if Self::looks_virtual(adapter_path, monitor_path, monitor_name, source_name) {
            return PathRole::Virtual;
        }
        if internal_technology {
            return PathRole::Internal;
        }
        PathRole::External
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeepOffAction {
    None,
    Deactivate,
    EnableBundledVdd,
    InstallBundledVdd,
    Blocked,
}

#[derive(Clone, Debug)]
pub struct KeepOffPlan {
    pub action: KeepOffAction,
    pub block_reason: Option<String>,
    pub adjust_origin: bool,
    pub needs_bundled_vdd: bool,
    pub may_adjust_clone: bool,
    pub selected_active_count: i32,
    pub remaining_physical_active: i32,
}

impl KeepOffPlan {
    pub fn is_allowed(&self) -> bool {
        matches!(
            self.action,
            KeepOffAction::Deactivate
                | KeepOffAction::EnableBundledVdd
                | KeepOffAction::InstallBundledVdd
        )
    }

    pub fn none(detail: impl Into<String>) -> Self {
        Self {
            action: KeepOffAction::None,
            block_reason: Some(detail.into()),
            adjust_origin: false,
            needs_bundled_vdd: false,
            may_adjust_clone: false,
            selected_active_count: 0,
            remaining_physical_active: 0,
        }
    }

    pub fn block(reason: impl Into<String>) -> Self {
        Self {
            action: KeepOffAction::Blocked,
            block_reason: Some(reason.into()),
            adjust_origin: false,
            needs_bundled_vdd: false,
            may_adjust_clone: false,
            selected_active_count: 0,
            remaining_physical_active: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BundledVddAvailability {
    Absent,
    PayloadOnly,
    Installed,
}

impl BundledVddAvailability {
    pub fn from_flags(installed: bool, payload: bool) -> Self {
        if installed {
            Self::Installed
        } else if payload {
            Self::PayloadOnly
        } else {
            Self::Absent
        }
    }
}

impl From<bool> for BundledVddAvailability {
    fn from(installed: bool) -> Self {
        if installed {
            Self::Installed
        } else {
            Self::Absent
        }
    }
}

pub struct Gate;

impl Gate {
    pub const LAST_PATH_REASON: &'static str =
        "没有第二活动目标，且找不到已校验的辅助虚拟输出驱动包，无法停用最后一条物理路径。";
    pub const ENABLE_VDD_REASON: &'static str =
        "将启用辅助虚拟输出。这是显示驱动，显示拓扑可能短暂变化。";
    pub const INSTALL_VDD_REASON: &'static str =
        "将安装并启用辅助虚拟输出。这是显示驱动，显示拓扑可能短暂变化。";
    pub const THIRD_PARTY_VIRTUAL_IGNORED: &'static str = "第三方虚拟屏不能作为第二目标。";

    pub fn plan_keep_off(
        snapshot: &DisplaySnapshot,
        selected: &[ScreenIdentity],
        bundled_vdd: impl Into<BundledVddAvailability>,
    ) -> KeepOffPlan {
        let bundled_vdd = bundled_vdd.into();
        if selected.is_empty() {
            return KeepOffPlan::none("未选择物理屏。");
        }
        let active_physical: Vec<&PathRow> = snapshot.active_physical().collect();
        let turning_off: Vec<&PathRow> = active_physical
            .iter()
            .copied()
            .filter(|row| selected.iter().any(|id| id.matches(&row.identity())))
            .collect();
        let remaining_physical = active_physical
            .iter()
            .filter(|row| !selected.iter().any(|id| id.matches(&row.identity())))
            .count() as i32;
        if turning_off.is_empty() {
            if snapshot.active_paths().next().is_some() {
                return KeepOffPlan {
                    action: KeepOffAction::Deactivate,
                    block_reason: None,
                    adjust_origin: false,
                    needs_bundled_vdd: false,
                    may_adjust_clone: snapshot.has_active_bundled_vdd() && remaining_physical == 0,
                    selected_active_count: 0,
                    remaining_physical_active: remaining_physical,
                };
            }
            return KeepOffPlan::block("没有可关闭的已连接物理屏。");
        }
        if remaining_physical >= 1 {
            return KeepOffPlan {
                action: KeepOffAction::Deactivate,
                block_reason: None,
                adjust_origin: true,
                needs_bundled_vdd: false,
                may_adjust_clone: false,
                selected_active_count: turning_off.len() as i32,
                remaining_physical_active: remaining_physical,
            };
        }
        if snapshot.has_active_bundled_vdd() {
            return KeepOffPlan {
                action: KeepOffAction::Deactivate,
                block_reason: None,
                adjust_origin: true,
                needs_bundled_vdd: false,
                may_adjust_clone: true,
                selected_active_count: turning_off.len() as i32,
                remaining_physical_active: 0,
            };
        }
        if bundled_vdd == BundledVddAvailability::Installed {
            return KeepOffPlan {
                action: KeepOffAction::EnableBundledVdd,
                block_reason: Some(Self::ENABLE_VDD_REASON.into()),
                adjust_origin: true,
                needs_bundled_vdd: true,
                may_adjust_clone: true,
                selected_active_count: turning_off.len() as i32,
                remaining_physical_active: 0,
            };
        }
        if bundled_vdd == BundledVddAvailability::PayloadOnly {
            return KeepOffPlan {
                action: KeepOffAction::InstallBundledVdd,
                block_reason: Some(Self::INSTALL_VDD_REASON.into()),
                adjust_origin: true,
                needs_bundled_vdd: true,
                may_adjust_clone: true,
                selected_active_count: turning_off.len() as i32,
                remaining_physical_active: 0,
            };
        }
        KeepOffPlan::block(Self::LAST_PATH_REASON)
    }

    pub fn screen_keep_off_plan(
        snapshot: &DisplaySnapshot,
        screen: &ScreenIdentity,
        already_wanted: &[ScreenIdentity],
        bundled_vdd: impl Into<BundledVddAvailability>,
    ) -> KeepOffPlan {
        let mut selected = already_wanted.to_vec();
        if !selected.iter().any(|id| id.matches(screen)) {
            selected.push(screen.clone());
        }
        Self::plan_keep_off(snapshot, &selected, bundled_vdd)
    }

    pub fn screen_keep_off_block_reason(
        snapshot: &DisplaySnapshot,
        screen: &ScreenIdentity,
        already_wanted: &[ScreenIdentity],
        bundled_vdd: impl Into<BundledVddAvailability>,
    ) -> Option<String> {
        let plan = Self::screen_keep_off_plan(snapshot, screen, already_wanted, bundled_vdd);
        if plan.action == KeepOffAction::Blocked {
            plan.block_reason
        } else {
            None
        }
    }
}

pub struct BundledVddSettings;

impl BundledVddSettings {
    pub const DRIVER_READS_DIRECTORY: &'static str = r"C:\VirtualDisplayDriver";
    pub const FILE_NAME: &'static str = "vdd_settings.xml";
    pub const FALLBACK_WIDTH: u32 = 1920;
    pub const FALLBACK_HEIGHT: u32 = 1200;
    pub const MIN_WIDTH: u32 = 800;
    pub const MAX_WIDTH: u32 = 7680;
    pub const MIN_HEIGHT: u32 = 600;
    pub const MAX_HEIGHT: u32 = 4320;
    pub const XML: &'static str = r#"<?xml version="1.0" encoding="utf-8"?>
<vdd_settings>
  <monitors><count>1</count></monitors>
  <gpu><friendlyname>default</friendlyname></gpu>
  <global><g_refresh_rate>60</g_refresh_rate></global>
  <resolutions><resolution><width>1920</width><height>1200</height><refresh_rate>60</refresh_rate></resolution></resolutions>
  <options><CustomEdid>false</CustomEdid><PreventSpoof>false</PreventSpoof><EdidCeaOverride>false</EdidCeaOverride><HardwareCursor>true</HardwareCursor><SDR10bit>false</SDR10bit><HDRPlus>false</HDRPlus><logging>false</logging><debuglogging>false</debuglogging></options>
</vdd_settings>"#;

    pub fn xml_for(width: u32, height: u32) -> String {
        Self::XML
            .replace("<width>1920</width>", &format!("<width>{width}</width>"))
            .replace("<height>1200</height>", &format!("<height>{height}</height>"))
    }

    pub fn accepted(width: u32, height: u32) -> bool {
        (Self::MIN_WIDTH..=Self::MAX_WIDTH).contains(&width)
            && (Self::MIN_HEIGHT..=Self::MAX_HEIGHT).contains(&height)
    }

    pub fn write_xml(directories: &[&str]) -> Result<Vec<String>, String> {
        Self::write_resolution(directories, Self::FALLBACK_WIDTH, Self::FALLBACK_HEIGHT)
    }

    pub fn write_resolution(
        directories: &[&str],
        width: u32,
        height: u32,
    ) -> Result<Vec<String>, String> {
        let xml = Self::xml_for(width, height);
        let mut created = Vec::new();
        let mut seen = Vec::new();
        let result = (|| -> Result<(), String> {
            for dir in directories {
                if dir.trim().is_empty() {
                    continue;
                }
                if seen.iter().any(|s: &String| s.eq_ignore_ascii_case(dir)) {
                    continue;
                }
                seen.push((*dir).to_string());
                std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
                let path = std::path::Path::new(dir).join(Self::FILE_NAME);
                if path.exists() {
                    if !Self::owns_file(&path) {
                        return Err(format!(
                            "已有他人的 {}：{}",
                            Self::FILE_NAME,
                            path.display()
                        ));
                    }
                    std::fs::write(&path, &xml).map_err(|e| e.to_string())?;
                    continue;
                }
                std::fs::write(&path, &xml).map_err(|e| e.to_string())?;
                created.push(path.to_string_lossy().into_owned());
            }
            Ok(())
        })();
        if let Err(e) = result {
            Self::rollback_created(&created);
            return Err(e);
        }
        Ok(created)
    }

    pub fn write_requested(directories: &[&str]) -> Result<Vec<String>, String> {
        let (width, height) = VddResolutionRequest::read_active()
            .map(|request| (request.width, request.height))
            .filter(|(width, height)| Self::accepted(*width, *height))
            .unwrap_or((Self::FALLBACK_WIDTH, Self::FALLBACK_HEIGHT));
        Self::write_resolution(directories, width, height)
    }

    pub fn owns_text(text: &str) -> bool {
        let Some((width, height)) = single_resolution(text) else {
            return false;
        };
        Self::normalize(text) == Self::normalize(&Self::xml_for(width, height))
    }

    pub fn owns_file(path: &std::path::Path) -> bool {
        match std::fs::read_to_string(path) {
            Ok(text) => Self::owns_text(&text),
            Err(_) => false,
        }
    }

    pub fn try_remove_owned_file(directory: &str) -> bool {
        if directory.trim().is_empty() {
            return false;
        }
        let path = std::path::Path::new(directory).join(Self::FILE_NAME);
        if !Self::owns_file(&path) {
            return false;
        }
        std::fs::remove_file(path).is_ok()
    }

    pub fn rollback_created(paths: &[String]) {
        for path in paths {
            if path.trim().is_empty() {
                continue;
            }
            if let Some(dir) = std::path::Path::new(path).parent() {
                Self::try_remove_owned_file(&dir.to_string_lossy());
            }
        }
    }

    fn normalize(text: &str) -> String {
        text.replace("\r\n", "\n")
    }
}

fn single_tag(text: &str, tag: &str) -> Option<u32> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = text.find(&open)?;
    let after = start + open.len();
    if text[..start].contains(&open) || text[after..].contains(&open) {
        return None;
    }
    let end = text[after..].find(&close)? + after;
    text[after..end].trim().parse().ok()
}

fn single_resolution(text: &str) -> Option<(u32, u32)> {
    Some((single_tag(text, "width")?, single_tag(text, "height")?))
}

pub const VDD_MODE_PRESETS: &[(u32, u32)] = &[
    (1920, 1080),
    (1920, 1200),
    (2560, 1440),
    (2560, 1600),
    (3840, 2160),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "camelCase")]
pub enum VddModePreference {
    MatchPhysical,
    Fixed { width: u32, height: u32 },
}

impl Default for VddModePreference {
    fn default() -> Self {
        Self::MatchPhysical
    }
}

impl VddModePreference {
    pub fn preference_path() -> std::path::PathBuf {
        vdd_mode_directory().join("vdd-mode.json")
    }

    pub fn load() -> Self {
        Self::load_from(&Self::preference_path())
    }

    pub fn load_from(path: &std::path::Path) -> Self {
        let Ok(text) = std::fs::read_to_string(path) else {
            return Self::MatchPhysical;
        };
        match serde_json::from_str::<Self>(&text) {
            Ok(Self::Fixed { width, height }) if !BundledVddSettings::accepted(width, height) => {
                Self::MatchPhysical
            }
            Ok(preference) => preference,
            Err(_) => Self::MatchPhysical,
        }
    }

    pub fn save(&self) -> Result<(), String> {
        self.save_to(&Self::preference_path())
    }

    pub fn save_to(&self, path: &std::path::Path) -> Result<(), String> {
        crate::session::JsonUtil::write_atomic(path, self)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VddResolutionRequest {
    pub width: u32,
    pub height: u32,
}

impl VddResolutionRequest {
    pub fn active_path() -> std::path::PathBuf {
        vdd_mode_directory().join("vdd-mode-active.json")
    }

    pub fn write_active(&self) -> Result<(), String> {
        crate::session::JsonUtil::write_atomic(Self::active_path(), self)
    }

    pub fn read_active() -> Option<Self> {
        crate::session::JsonUtil::try_read(Self::active_path())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VddResolutionChoice {
    pub width: u32,
    pub height: u32,
    pub fell_back: bool,
}

impl VddResolutionChoice {
    pub fn fallback() -> Self {
        Self {
            width: BundledVddSettings::FALLBACK_WIDTH,
            height: BundledVddSettings::FALLBACK_HEIGHT,
            fell_back: true,
        }
    }
}

pub fn resolve_vdd_resolution(
    frame: &CcdFrame,
    selected: &[ScreenIdentity],
    preference: VddModePreference,
) -> VddResolutionChoice {
    if let VddModePreference::Fixed { width, height } = preference {
        if BundledVddSettings::accepted(width, height) {
            return VddResolutionChoice {
                width,
                height,
                fell_back: false,
            };
        }
        return VddResolutionChoice::fallback();
    }
    let mut matches: Vec<crate::native::DisplayConfigSourceMode> = Vec::new();
    for (path, row) in frame.paths.iter().zip(frame.snapshot.paths.iter()) {
        if !row.active || !row.is_physical() {
            continue;
        }
        if !selected.iter().any(|id| id.matches(&row.identity())) {
            continue;
        }
        let Some(index) = PathOps::source_mode_index(path, &frame.modes) else {
            continue;
        };
        let mode = frame.modes[index].source_mode();
        if BundledVddSettings::accepted(mode.width, mode.height) {
            matches.push(mode);
        }
    }
    if matches.is_empty() {
        return VddResolutionChoice::fallback();
    }
    matches.sort_by(|left, right| {
        let left_origin = left.position.x == 0 && left.position.y == 0;
        let right_origin = right.position.x == 0 && right.position.y == 0;
        right_origin
            .cmp(&left_origin)
            .then_with(|| {
                (right.width as u64 * right.height as u64)
                    .cmp(&(left.width as u64 * left.height as u64))
            })
            .then_with(|| right.width.cmp(&left.width))
    });
    VddResolutionChoice {
        width: matches[0].width,
        height: matches[0].height,
        fell_back: false,
    }
}

fn vdd_mode_directory() -> std::path::PathBuf {
    #[cfg(test)]
    {
        std::env::temp_dir().join("veil-vdd-mode")
    }
    #[cfg(not(test))]
    {
        crate::session::SessionPaths::root()
    }
}
