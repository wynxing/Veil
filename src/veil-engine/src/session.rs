use crate::native::CcdConstants;
use crate::ScreenIdentity;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub struct JsonUtil;

impl JsonUtil {
    pub fn write_atomic<T: Serialize>(path: impl AsRef<Path>, value: &T) -> Result<(), String> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let payload = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
        let tmp = path.with_extension(format!("{}.tmp", unique_session_id()));
        let result = (|| {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&tmp)
                .map_err(|e| e.to_string())?;
            file.write_all(payload.as_bytes())
                .map_err(|e| e.to_string())?;
            file.sync_all().map_err(|e| e.to_string())?;
            drop(file);
            let mut last = String::new();
            for attempt in 0..8 {
                match replace_file(&tmp, path) {
                    Ok(()) => return Ok(()),
                    Err(e) => last = e.to_string(),
                }
                if attempt < 7 {
                    std::thread::sleep(Duration::from_millis(20));
                }
            }
            Err(last)
        })();
        let _ = std::fs::remove_file(&tmp);
        result
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

fn replace_file(source: &Path, dest: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };
    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let dest: Vec<u16> = dest.as_os_str().encode_wide().chain(Some(0)).collect();
    if unsafe {
        MoveFileExW(
            source.as_ptr(),
            dest.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    } == 0
    {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

pub const PROTOCOL_VERSION: u32 = 2;
pub fn request_id() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static LAST: AtomicU64 = AtomicU64::new(0);
    let now = (unix_seconds() * 1_000_000.0) as u64;
    LAST.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |old| {
        Some(now.max(old + 1))
    })
    .unwrap()
    .max(now - 1)
        + 1
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RestoreState {
    #[default]
    Unknown,
    Complete,
    Partial,
    NotNeeded,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RecoveryState {
    #[default]
    Waiting,
    Holding,
    Restoring,
    RestoreFailed,
    Finished,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMetadata {
    pub protocol_version: u32,
    pub physical_targets: Vec<ScreenIdentityDto>,
    pub vdd_owned: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RestoreOutcome {
    pub state: RestoreState,
    pub message: String,
}
impl RestoreOutcome {
    pub fn complete(message: impl Into<String>) -> Self {
        Self {
            state: RestoreState::Complete,
            message: message.into(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RestoreError {
    Failed(String),
    Protocol(String),
    Timeout(String),
}
impl RestoreError {
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::Failed(_) => 1,
            _ => 2,
        }
    }
}
impl std::fmt::Display for RestoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Failed(s) | Self::Protocol(s) | Self::Timeout(s) => write!(f, "{s}"),
        }
    }
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
    pub request_id: u64,
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
    #[serde(default)]
    pub request_id: u64,
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
    #[serde(default)]
    pub kind: String,
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
    pub state: RecoveryState,
    #[serde(default)]
    pub processed_request_id: u64,
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
    pub protocol_version: u32,
    #[serde(default)]
    pub restore_state: RestoreState,
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
        #[cfg(not(test))]
        let root = Self::root();
        #[cfg(test)]
        let root = std::env::temp_dir().join("veil-coordinator-tests");
        let dir = root.join(format!("session-{}", unique_session_id()));
        std::fs::create_dir_all(&dir).ok();
        dir
    }

    pub fn metadata(dir: impl AsRef<Path>) -> PathBuf {
        dir.as_ref().join("session.json")
    }
    pub fn baseline(dir: impl AsRef<Path>) -> PathBuf {
        dir.as_ref().join("baseline.json")
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
    pub fn request_all_and_wait(timeout: Duration) -> Result<RestoreOutcome, RestoreError> {
        Self::wait_after_release(&SessionPaths::root(), timeout)
    }
    pub fn wait_after_release(
        root: impl AsRef<Path>,
        timeout: Duration,
    ) -> Result<RestoreOutcome, RestoreError> {
        let mut pending = Vec::new();
        if !root.as_ref().exists() {
            return Ok(RestoreOutcome::complete("没有待恢复会话。"));
        }
        let entries = std::fs::read_dir(root).map_err(|e| RestoreError::Protocol(e.to_string()))?;
        for entry in entries {
            let path = entry
                .map_err(|e| RestoreError::Protocol(e.to_string()))?
                .path();
            if !path.is_dir()
                || !path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .starts_with("session-")
            {
                continue;
            }
            if SessionPaths::result(&path).exists() {
                JsonUtil::read::<ResultFile>(SessionPaths::result(&path))
                    .map_err(RestoreError::Protocol)?
                    .restoration_outcome()?;
                continue;
            }
            let meta = JsonUtil::read::<SessionMetadata>(SessionPaths::metadata(&path))
                .map_err(RestoreError::Protocol)?;
            if meta.protocol_version != PROTOCOL_VERSION {
                return Err(RestoreError::Protocol(
                    "未知会话协议，不能确认恢复。".into(),
                ));
            }
            let request_id = request_id();
            JsonUtil::write_atomic(
                SessionPaths::release(&path),
                &ReleaseFile {
                    at: unix_seconds(),
                    request_id,
                },
            )
            .map_err(RestoreError::Protocol)?;
            pending.push((path, request_id));
        }
        let deadline = Instant::now() + timeout;
        loop {
            let mut done = true;
            for (dir, request_id) in &pending {
                if SessionPaths::result(dir).exists() {
                    JsonUtil::read::<ResultFile>(SessionPaths::result(dir))
                        .map_err(RestoreError::Protocol)?
                        .restoration_outcome()?;
                } else {
                    done = false;
                    if let Some(hb) =
                        JsonUtil::try_read::<HeartbeatFile>(SessionPaths::heartbeat(dir))
                    {
                        if hb.state == RecoveryState::RestoreFailed
                            && hb.processed_request_id >= *request_id
                        {
                            return Err(RestoreError::Failed(
                                hb.detail.unwrap_or_else(|| "恢复失败。".into()),
                            ));
                        }
                    }
                }
            }
            if done {
                return Ok(RestoreOutcome::complete("全部会话已恢复。"));
            }
            if Instant::now() >= deadline {
                return Err(RestoreError::Timeout("等待恢复超时，未移除驱动。".into()));
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}
impl ResultFile {
    pub fn restoration_outcome(&self) -> Result<RestoreOutcome, RestoreError> {
        if self.protocol_version != PROTOCOL_VERSION {
            return Err(RestoreError::Protocol("未知恢复结果协议。".into()));
        }
        match self.restore_state {
            RestoreState::Complete | RestoreState::NotNeeded => Ok(RestoreOutcome {
                state: self.restore_state,
                message: if self.restore_state == RestoreState::NotNeeded {
                    "无需恢复，未执行关屏。".into()
                } else if self.restored_topology {
                    "已恢复全部。".into()
                } else {
                    "物理输出已恢复，显示布局可能变化。".into()
                },
            }),
            _ => Err(RestoreError::Failed(
                self.error
                    .clone()
                    .unwrap_or_else(|| "恢复未完成；请重试恢复全部。".into()),
            )),
        }
    }
}

pub fn unix_seconds() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}
