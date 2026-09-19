pub mod capability;
pub mod coordinator;
pub mod fakes;
pub mod native;
pub mod planner;
pub mod process;
pub mod recovery;
pub mod screen_list;
pub mod session;
pub mod topology;

pub use capability::{
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
pub use planner::{DisplayPlanner, ValidatePlanResult};
pub use process::ProcessLaunch;
pub use recovery::{RecoveryOptions, RecoverySession};
pub use screen_list::{ScreenItem, ScreenListBuilder};
pub use session::{
    ArmFile, HeartbeatFile, IntentFile, JsonUtil, OpenSessionRelease, ReadyFile, ReleaseFile,
    ResultFile, ScreenIdentityDto, SessionLog, SessionPaths, VddRequestFile,
};
pub use topology::{struct_bytes, DeactivateResult, PathOps, TopologyBlob};

#[cfg(test)]
mod tests;
