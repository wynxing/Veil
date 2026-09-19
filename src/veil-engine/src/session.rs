use crate::native::CcdConstants;
use crate::ScreenIdentity;
use serde::{Deserialize, Serialize};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

pub struct JsonUtil;

impl JsonUtil {
    pub fn write_atomic<T: Serialize>(path: impl AsRef<Path>, value: &T) -> Result<(), String> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let payload = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
        let tmp = PathBuf::from(format!("{}.tmp", path.display()));
        for attempt in 0.. {
            match write_then_rename(&tmp, path, &payload) {
                Ok(()) => return Ok(()),
                Err(e) if e.kind() == ErrorKind::PermissionDenied || e.kind() == ErrorKind::AlreadyExists || e.kind() == ErrorKind::WouldBlock => {
                    if attempt >= 7 {
                        return Err(e.to_string());
                    }
                    std::thread::sleep(std::time::Duration::from_millis(20));
                }
                Err(e) if attempt < 7 && e.kind() == ErrorKind::Other || e.raw_os_error() == Some(32) => {
                    std::thread::sleep(std::time::Duration::from_millis(20));
                }
                Err(e) => {
                    if attempt < 7 {
                        std::thread::sleep(std::time::Duration::from_millis(20));
                    } else {
                        return Err(e.to_string());
                    }
                }
            }
        }
        unreachable!()
    }

    pub fn read<T: for<'de> Deserialize<'de>>(path: impl AsRef<Path>) -> Result<T, String> {
        let text = std::fs::read_to_string(path.as_ref()).map_err(|e| e.to_string())?;
        serde_json::from_str(&text).map_err(|e| e.to_string())
    }

    pub fn try_read<T: for<'de> Deserialize<'de>>(path: impl AsRef<Path>) -> Option<T> {
        if !path.as_ref().exists() {
            return None;
        }
        Self::read(path).ok()
    }
}

fn write_then_rename(tmp: &Path, dest: &Path, payload: &str) -> std::io::Result<()> {
    {
        let mut f = std::fs::File::create(tmp)?;
        f.write_all(payload.as_bytes())?;
        f.flush()?;
    }
    std::fs::rename(tmp, dest).or_else(|_| {
        std::fs::copy(tmp, dest)?;
        std::fs::remove_file(tmp)?;
        Ok(())
    })
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadyFile {
    pub pid: i32,
    pub hotkey_registered: bool,
    #[serde(default = "default_hotkey")]
    pub hotkey: String,
}

fn default_hotkey() -> String {
    CcdConstants::HOTKEY_TEXT.into()
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArmFile {
    pub pid: i32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntentFile {
    #[serde(default)]
    pub keep_off: Vec<ScreenIdentityDto>,
    #[serde(default)]
    pub vdd_assist: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenIdentityDto {
    #[serde(default)]
    pub adapter_luid: String,
    #[serde(default)]
    pub target_id: u32,
    #[serde(default)]
    pub monitor_path: String,
}

impl ScreenIdentityDto {
    pub fn to_identity(&self) -> ScreenIdentity {
        ScreenIdentity::new(&self.adapter_luid, self.target_id, &self.monitor_path)
    }

    pub fn from_identity(id: &ScreenIdentity) -> Self {
        Self {
            adapter_luid: id.adapter_luid.clone(),
            target_id: id.target_id,
            monitor_path: id.monitor_path.clone(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseFile {
    pub at: f64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeartbeatScreen {
    #[serde(default)]
    pub adapter_luid: String,
    #[serde(default)]
    pub target_id: u32,
    #[serde(default)]
    pub monitor_path: String,
    #[serde(default)]
    pub name: String,
    #[serde(default = "default_wanted")]
    pub wanted: String,
    #[serde(default = "default_confirmed")]
    pub confirmed: String,
    #[serde(default)]
    pub detail: String,
}

fn default_wanted() -> String {
    "开启".into()
}
fn default_confirmed() -> String {
    "未知".into()
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeartbeatFile {
    #[serde(default)]
    pub hotkey_registered: bool,
    #[serde(default)]
    pub armed: bool,
    #[serde(default)]
    pub screens: Vec<HeartbeatScreen>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultFile {
    #[serde(default)]
    pub ok: bool,
    #[serde(default = "default_reason")]
    pub reason: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub apply_rc: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restore_rc: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_rc: Option<i32>,
    #[serde(default)]
    pub restored_topology: bool,
    #[serde(default)]
    pub restored_targets: bool,
    #[serde(default)]
    pub reapply_attempted: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adjusted_origin: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adjusted_clone: Option<bool>,
}

fn default_reason() -> String {
    "error".into()
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VddRequestFile {
    pub at: f64,
    #[serde(default = "default_vdd_reason")]
    pub reason: String,
}

fn default_vdd_reason() -> String {
    "reapply".into()
}

pub struct SessionPaths;

impl SessionPaths {
    pub fn root() -> PathBuf {
        let local = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| {
            let home = std::env::var("USERPROFILE").unwrap_or_else(|_| ".".into());
            format!("{home}\\AppData\\Local")
        });
        PathBuf::from(local).join("Veil")
    }

    pub fn new_session_directory() -> PathBuf {
        let dir = Self::root().join(format!("session-{}", unique_session_id()));
        std::fs::create_dir_all(&dir).ok();
        dir
    }

    pub fn topology(dir: impl AsRef<Path>) -> PathBuf {
        dir.as_ref().join("topology.json")
    }
    pub fn ready(dir: impl AsRef<Path>) -> PathBuf {
        dir.as_ref().join("ready.json")
    }
    pub fn arm(dir: impl AsRef<Path>) -> PathBuf {
        dir.as_ref().join("arm.json")
    }
    pub fn intent(dir: impl AsRef<Path>) -> PathBuf {
        dir.as_ref().join("intent.json")
    }
    pub fn release(dir: impl AsRef<Path>) -> PathBuf {
        dir.as_ref().join("release.json")
    }
    pub fn heartbeat(dir: impl AsRef<Path>) -> PathBuf {
        dir.as_ref().join("heartbeat.json")
    }
    pub fn result(dir: impl AsRef<Path>) -> PathBuf {
        dir.as_ref().join("result.json")
    }
    pub fn events(dir: impl AsRef<Path>) -> PathBuf {
        dir.as_ref().join("events.jsonl")
    }
    pub fn vdd_request(dir: impl AsRef<Path>) -> PathBuf {
        dir.as_ref().join("vdd-request.json")
    }
}

fn unique_session_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let tid = format!("{:?}", std::thread::current().id());
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let raw = format!("{pid:x}{n:x}{tid}{nanos:x}");
    let digest = format!("{:x}", {
        let mut h: u64 = 0xcbf29ce484222325;
        for b in raw.bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
        h
    });
    digest.chars().take(8).collect()
}

pub struct SessionLog;

impl SessionLog {
    pub fn append(
        directory: impl AsRef<Path>,
        event_type: &str,
        detail: Option<&str>,
        reason: Option<&str>,
        reapply: Option<bool>,
        apply_rc: Option<i32>,
    ) {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct SessionEvent<'a> {
            ts: String,
            #[serde(rename = "type")]
            event_type: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            detail: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            reason: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            reapply: Option<bool>,
            #[serde(skip_serializing_if = "Option::is_none")]
            apply_rc: Option<i32>,
        }
        let ev = SessionEvent {
            ts: chrono::Utc::now().to_rfc3339(),
            event_type,
            detail,
            reason,
            reapply,
            apply_rc,
        };
        let dir = directory.as_ref();
        let _ = std::fs::create_dir_all(dir);
        if let Ok(line) = serde_json::to_string(&ev) {
            let mut file = match std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(SessionPaths::events(dir))
            {
                Ok(f) => f,
                Err(_) => return,
            };
            let _ = writeln!(file, "{line}");
        }
    }
}

pub struct OpenSessionRelease;

impl OpenSessionRelease {
    pub fn request_all() {
        let root = SessionPaths::root();
        if !root.exists() {
            return;
        }
        let Ok(entries) = std::fs::read_dir(root) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if !name.starts_with("session-") {
                continue;
            }
            if SessionPaths::result(&path).exists() {
                continue;
            }
            let at = unix_seconds();
            let _ = JsonUtil::write_atomic(SessionPaths::release(&path), &ReleaseFile { at });
        }
    }
}

pub fn unix_seconds() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}
