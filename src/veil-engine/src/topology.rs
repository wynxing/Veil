use crate::native::{CcdConstants, DisplayConfigModeInfo, DisplayConfigPathInfo, PointL};
use crate::ScreenIdentity;
use base64::Engine;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;

#[derive(Clone)]
pub struct DeactivateResult {
    pub paths: Vec<DisplayConfigPathInfo>,
    pub modes: Vec<DisplayConfigModeInfo>,
    pub disabled_count: i32,
    pub remaining_active: i32,
    pub adjusted_origin: bool,
}

impl DeactivateResult {
    pub fn can_apply(&self) -> bool {
        self.disabled_count > 0 && self.remaining_active > 0
    }
}

pub struct PathOps;

impl PathOps {
    pub fn deactivate(
        paths: &[DisplayConfigPathInfo],
        modes: &[DisplayConfigModeInfo],
        path_identities: &[ScreenIdentity],
        selected: &[ScreenIdentity],
        adjust_origin: bool,
    ) -> Result<DeactivateResult, String> {
        if paths.len() != path_identities.len() {
            return Err("path identity count must match paths.".into());
        }
        let mut changed = paths.to_vec();
        let mut disabled = 0;
        let mut remaining = 0;
        for i in 0..changed.len() {
            let active = (changed[i].flags & CcdConstants::DISPLAYCONFIG_PATH_ACTIVE) != 0;
            let selected_match = selected.iter().any(|id| id.matches(&path_identities[i]));
            if active && selected_match {
                changed[i].flags &= !CcdConstants::DISPLAYCONFIG_PATH_ACTIVE;
                disabled += 1;
            } else if (changed[i].flags & CcdConstants::DISPLAYCONFIG_PATH_ACTIVE) != 0 {
                remaining += 1;
            }
        }
        let mut out_modes = modes.to_vec();
        let mut moved = false;
        if adjust_origin && remaining > 0 {
            let (modes, did) = Self::move_remaining_to_origin(&changed, modes);
            out_modes = modes;
            moved = did;
        }
        Ok(DeactivateResult {
            paths: changed,
            modes: out_modes,
            disabled_count: disabled,
            remaining_active: remaining,
            adjusted_origin: moved,
        })
    }

    pub fn move_remaining_to_origin(
        paths: &[DisplayConfigPathInfo],
        modes: &[DisplayConfigModeInfo],
    ) -> (Vec<DisplayConfigModeInfo>, bool) {
        let mut changed = modes.to_vec();
        let remaining_active: Vec<_> = paths
            .iter()
            .copied()
            .filter(|p| (p.flags & CcdConstants::DISPLAYCONFIG_PATH_ACTIVE) != 0)
            .collect();
        let already_at_origin = remaining_active.iter().any(|path| {
            if let Some(idx) = Self::source_mode_index(path, &changed) {
                let pos = changed[idx].source_mode().position;
                pos.x == 0 && pos.y == 0
            } else {
                false
            }
        });
        if already_at_origin {
            return (changed, false);
        }
        for path in remaining_active {
            if let Some(idx) = Self::source_mode_index(&path, &changed) {
                let mut source = changed[idx].source_mode();
                source.position = PointL { x: 0, y: 0 };
                changed[idx].set_source_mode(source);
                return (changed, true);
            }
        }
        (changed, false)
    }

    pub fn source_mode_index(path: &DisplayConfigPathInfo, modes: &[DisplayConfigModeInfo]) -> Option<usize> {
        let packed = path.source_info.mode_info_idx;
        let packed_src = (packed >> 16) & 0xFFFF;
        for idx in [packed_src, packed] {
            if idx == CcdConstants::DISPLAYCONFIG_PATH_SOURCE_MODE_IDX_INVALID || idx as usize >= modes.len() {
                continue;
            }
            if modes[idx as usize].info_type == CcdConstants::DISPLAYCONFIG_MODE_INFO_TYPE_SOURCE {
                return Some(idx as usize);
            }
        }
        None
    }

    pub fn active_targets(paths: &[DisplayConfigPathInfo]) -> Vec<(String, u32)> {
        let mut items: Vec<_> = paths
            .iter()
            .filter(|p| (p.flags & CcdConstants::DISPLAYCONFIG_PATH_ACTIVE) != 0)
            .map(|p| (p.target_info.adapter_id.to_hex(), p.target_info.id))
            .collect();
        items.sort();
        items
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TopologyBlob {
    pub version: i32,
    pub query_flags: u32,
    pub path_count: i32,
    pub mode_count: i32,
    pub path_b64: String,
    pub mode_b64: String,
    pub saved_at: String,
}

impl Default for TopologyBlob {
    fn default() -> Self {
        Self {
            version: 1,
            query_flags: CcdConstants::QUERY_FLAGS,
            path_count: 0,
            mode_count: 0,
            path_b64: String::new(),
            mode_b64: String::new(),
            saved_at: String::new(),
        }
    }
}

impl TopologyBlob {
    pub fn from_arrays(paths: &[DisplayConfigPathInfo], modes: &[DisplayConfigModeInfo]) -> Self {
        Self::from_arrays_flags(paths, modes, CcdConstants::QUERY_FLAGS)
    }

    pub fn from_arrays_flags(
        paths: &[DisplayConfigPathInfo],
        modes: &[DisplayConfigModeInfo],
        query_flags: u32,
    ) -> Self {
        Self {
            version: 1,
            query_flags,
            path_count: paths.len() as i32,
            mode_count: modes.len() as i32,
            path_b64: base64::engine::general_purpose::STANDARD.encode(struct_bytes(paths)),
            mode_b64: base64::engine::general_purpose::STANDARD.encode(struct_bytes(modes)),
            saved_at: chrono::Utc::now().to_rfc3339(),
        }
    }

    pub fn to_arrays(&self) -> Result<(Vec<DisplayConfigPathInfo>, Vec<DisplayConfigModeInfo>), String> {
        let path_raw = base64::engine::general_purpose::STANDARD
            .decode(&self.path_b64)
            .map_err(|e| e.to_string())?;
        let mode_raw = base64::engine::general_purpose::STANDARD
            .decode(&self.mode_b64)
            .map_err(|e| e.to_string())?;
        let expected_path = std::mem::size_of::<DisplayConfigPathInfo>() * self.path_count as usize;
        let expected_mode = std::mem::size_of::<DisplayConfigModeInfo>() * self.mode_count as usize;
        if path_raw.len() != expected_path || mode_raw.len() != expected_mode {
            return Err(format!(
                "topology size mismatch: paths {}!={}, modes {}!={}",
                path_raw.len(),
                expected_path,
                mode_raw.len(),
                expected_mode
            ));
        }
        Ok((
            from_bytes(&path_raw, self.path_count as usize),
            from_bytes(&mode_raw, self.mode_count as usize),
        ))
    }

    pub fn save(path: impl AsRef<Path>, paths: &[DisplayConfigPathInfo], modes: &[DisplayConfigModeInfo]) -> Result<(), String> {
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_string_pretty(&Self::from_arrays(paths, modes)).map_err(|e| e.to_string())?;
        std::fs::write(path, json).map_err(|e| e.to_string())
    }

    pub fn load(path: impl AsRef<Path>) -> Result<(Vec<DisplayConfigPathInfo>, Vec<DisplayConfigModeInfo>), String> {
        let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        let blob: TopologyBlob = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        blob.to_arrays()
    }

    pub fn fingerprint(paths: &[DisplayConfigPathInfo], modes: &[DisplayConfigModeInfo]) -> String {
        let mut bytes = struct_bytes(paths);
        bytes.extend_from_slice(&struct_bytes(modes));
        hex_lower(&Sha256::digest(bytes))
    }
}

pub fn struct_bytes<T: Copy>(items: &[T]) -> Vec<u8> {
    let size = std::mem::size_of::<T>();
    let mut dest = vec![0u8; size * items.len()];
    if !items.is_empty() {
        unsafe {
            std::ptr::copy_nonoverlapping(items.as_ptr() as *const u8, dest.as_mut_ptr(), dest.len());
        }
    }
    dest
}

fn from_bytes<T: Copy + Default>(raw: &[u8], count: usize) -> Vec<T> {
    let size = std::mem::size_of::<T>();
    let mut items = vec![T::default(); count];
    if count > 0 {
        unsafe {
            std::ptr::copy_nonoverlapping(raw.as_ptr(), items.as_mut_ptr() as *mut u8, size * count);
        }
    }
    items
}

fn hex_lower(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
