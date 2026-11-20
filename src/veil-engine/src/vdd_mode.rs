use crate::capability::ScreenIdentity;
use crate::native::CcdFrame;
use crate::topology::PathOps;
use serde::{Deserialize, Serialize};

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
            .replace(
                "<height>1200</height>",
                &format!("<height>{height}</height>"),
            )
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "camelCase")]
pub enum VddModePreference {
    #[default]
    MatchPhysical,
    Fixed {
        width: u32,
        height: u32,
    },
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
