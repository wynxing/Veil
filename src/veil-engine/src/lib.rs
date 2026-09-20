pub mod capability;
pub mod coordinator;
pub mod driver_policy;
pub mod fakes;
pub mod maintenance;
pub mod native;
pub mod payload;
pub mod planner;
pub mod process;
pub mod recovery;
pub mod screen_list;
pub mod session;
pub mod topology;

pub use capability::{
    looks_like_device_path, resolved_screen_name, short_monitor_id, BundledVddAvailability,
    BundledVddSettings, DisplaySnapshot, Gate, KeepOffAction, KeepOffPlan, PathRole, PathRow,
    Roles, ScreenIdentity,
};
pub use coordinator::{DriverStatus, RecoveryCoordinator, RecoveryCoordinatorHooks};
pub use native::{
    CcdAbi, CcdApi, CcdConstants, CcdFrame, DisplayConfigModeInfo, DisplayConfigPathInfo,
    DisplayConfigPathSourceInfo, DisplayConfigPathTargetInfo, DisplayConfigSourceMode,
    DisplayConfigVideoSignalInfo, Hotkey, Luid, MonotonicClock, ParentWatcher, PointL,
    SystemMonotonicClock, Win32CcdApi, Win32Hotkey, Win32ParentWatcher,
};
pub use payload::{
    payload_present, payload_present_in, resolve as resolve_payload, validate as validate_payload,
    ResolvedPayload,
};
pub use planner::{DisplayPlanner, ValidatePlanResult};
pub use process::ProcessLaunch;
pub use recovery::{RecoveryOptions, RecoverySession};
pub use screen_list::{AuxiliaryInstallItem, ScreenItem, ScreenListBuilder};
pub use session::{
    ArmFile, HeartbeatFile, IntentFile, JsonUtil, OpenSessionRelease, ReadyFile, ReleaseFile,
    ResultFile, ScreenIdentityDto, SessionLog, SessionPaths, VddRequestFile,
};
pub use topology::{struct_bytes, DeactivateResult, PathOps, TopologyBlob};

#[cfg(test)]
mod tests;
