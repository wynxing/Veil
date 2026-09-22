use crate::capability::{DisplaySnapshot, PathRow, Roles};
use std::ffi::c_void;
use std::mem::{offset_of, size_of};
use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, HANDLE, HWND, WPARAM};
use windows_sys::Win32::System::Power::{
    PowerRegisterSuspendResumeNotification, PowerSettingRegisterNotification,
    PowerSettingUnregisterNotification, PowerUnregisterSuspendResumeNotification,
    DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS, HPOWERNOTIFY, POWERBROADCAST_SETTING,
};
use windows_sys::Win32::System::Threading::{
    GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{RegisterHotKey, UnregisterHotKey};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, PeekMessageW, DEVICE_NOTIFY_CALLBACK, MSG, PBT_APMRESUMEAUTOMATIC,
    PBT_APMRESUMESUSPEND, PBT_APMSUSPEND, PBT_POWERSETTINGCHANGE, PM_REMOVE, WM_HOTKEY,
};

pub struct CcdConstants;

impl CcdConstants {
    pub const ERROR_SUCCESS: i32 = 0;
    pub const ERROR_INSUFFICIENT_BUFFER: i32 = 122;
    pub const QDC_ONLY_ACTIVE_PATHS: u32 = 0x00000002;
    pub const QDC_VIRTUAL_MODE_AWARE: u32 = 0x00000010;
    pub const QDC_VIRTUAL_REFRESH_RATE_AWARE: u32 = 0x00000040;
    pub const SDC_TOPOLOGY_INTERNAL: u32 = 0x00000001;
    pub const SDC_TOPOLOGY_CLONE: u32 = 0x00000002;
    pub const SDC_USE_SUPPLIED_DISPLAY_CONFIG: u32 = 0x00000020;
    pub const SDC_VALIDATE: u32 = 0x00000040;
    pub const SDC_APPLY: u32 = 0x00000080;
    pub const SDC_SAVE_TO_DATABASE: u32 = 0x00000200;
    pub const SDC_ALLOW_CHANGES: u32 = 0x00000400;
    pub const SDC_VIRTUAL_MODE_AWARE: u32 = 0x00008000;
    pub const SDC_VIRTUAL_REFRESH_RATE_AWARE: u32 = 0x00020000;
    pub const DISPLAYCONFIG_PATH_ACTIVE: u32 = 0x00000001;
    pub const DISPLAYCONFIG_MODE_INFO_TYPE_SOURCE: u32 = 1;
    pub const DISPLAYCONFIG_MODE_INFO_TYPE_TARGET: u32 = 2;
    pub const DISPLAYCONFIG_PATH_SOURCE_MODE_IDX_INVALID: u32 = 0xFFFF;
    pub const DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME: u32 = 1;
    pub const DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME: u32 = 2;
    pub const DISPLAYCONFIG_DEVICE_INFO_GET_ADAPTER_NAME: u32 = 4;
    pub const OUTPUT_TECHNOLOGY_INTERNAL: u32 = 0x80000000;
    pub const OUTPUT_TECHNOLOGY_DISPLAY_PORT_EMBEDDED: u32 = 11;
    pub const OUTPUT_TECHNOLOGY_UDI_EMBEDDED: u32 = 13;
    pub const SM_CMONITORS: i32 = 80;
    pub const QUERY_FLAGS: u32 = Self::QDC_ONLY_ACTIVE_PATHS
        | Self::QDC_VIRTUAL_MODE_AWARE
        | Self::QDC_VIRTUAL_REFRESH_RATE_AWARE;
    pub const SET_BASE_FLAGS: u32 = Self::SDC_USE_SUPPLIED_DISPLAY_CONFIG
        | Self::SDC_ALLOW_CHANGES
        | Self::SDC_VIRTUAL_MODE_AWARE
        | Self::SDC_VIRTUAL_REFRESH_RATE_AWARE;
    pub const VALIDATE_FLAGS: u32 = Self::SET_BASE_FLAGS | Self::SDC_VALIDATE;
    pub const APPLY_FLAGS: u32 = Self::SET_BASE_FLAGS | Self::SDC_APPLY;
    pub const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;
    pub const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x01000000;
    pub const CREATE_NO_WINDOW: u32 = 0x08000000;
    pub const HOTKEY_MODIFIERS: u32 = 0x4007;
    pub const VK_F10: u32 = 0x79;
    pub const HOTKEY_ID: i32 = 1;
    pub const HOTKEY_TEXT: &'static str = "Ctrl+Alt+Shift+F10";
    pub const BUNDLED_HARDWARE_ID: &'static str = r"Root\MttVDD";
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Luid {
    pub low_part: u32,
    pub high_part: i32,
}

impl Luid {
    pub fn to_hex(self) -> String {
        format!("{:08x}{:08x}", self.high_part as u32, self.low_part)
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DisplayConfigRational {
    pub numerator: u32,
    pub denominator: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DisplayConfig2DRegion {
    pub cx: u32,
    pub cy: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DisplayConfigPathSourceInfo {
    pub adapter_id: Luid,
    pub id: u32,
    pub mode_info_idx: u32,
    pub status_flags: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DisplayConfigPathTargetInfo {
    pub adapter_id: Luid,
    pub id: u32,
    pub mode_info_idx: u32,
    pub output_technology: u32,
    pub rotation: u32,
    pub scaling: u32,
    pub refresh_rate: DisplayConfigRational,
    pub scan_line_ordering: u32,
    pub target_available: i32,
    pub status_flags: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DisplayConfigPathInfo {
    pub source_info: DisplayConfigPathSourceInfo,
    pub target_info: DisplayConfigPathTargetInfo,
    pub flags: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DisplayConfigVideoSignalInfo {
    pub pixel_rate: u64,
    pub h_sync_freq: DisplayConfigRational,
    pub v_sync_freq: DisplayConfigRational,
    pub active_size: DisplayConfig2DRegion,
    pub total_size: DisplayConfig2DRegion,
    pub video_standard: u32,
    pub scan_line_ordering: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DisplayConfigTargetMode {
    pub target_video_signal_info: DisplayConfigVideoSignalInfo,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PointL {
    pub x: i32,
    pub y: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DisplayConfigSourceMode {
    pub width: u32,
    pub height: u32,
    pub pixel_format: u32,
    pub position: PointL,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct RectL {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DisplayConfigDesktopImageInfo {
    pub path_source_size: PointL,
    pub desktop_image_region: RectL,
    pub desktop_image_clip: RectL,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union DisplayConfigModeUnion {
    pub target_mode: DisplayConfigTargetMode,
    pub source_mode: DisplayConfigSourceMode,
    pub desktop_image_info: DisplayConfigDesktopImageInfo,
}

impl Default for DisplayConfigModeUnion {
    fn default() -> Self {
        Self {
            target_mode: DisplayConfigTargetMode::default(),
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct DisplayConfigModeInfo {
    pub info_type: u32,
    pub id: u32,
    pub adapter_id: Luid,
    pub union: DisplayConfigModeUnion,
}

impl DisplayConfigModeInfo {
    pub fn source_mode(&self) -> DisplayConfigSourceMode {
        unsafe { self.union.source_mode }
    }

    pub fn set_source_mode(&mut self, mode: DisplayConfigSourceMode) {
        self.union.source_mode = mode;
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct DisplayConfigDeviceInfoHeader {
    pub info_type: u32,
    pub size: u32,
    pub adapter_id: Luid,
    pub id: u32,
}

#[repr(C)]
pub struct DisplayConfigTargetDeviceName {
    pub header: DisplayConfigDeviceInfoHeader,
    pub flags: u32,
    pub output_technology: u32,
    pub edid_manufacture_id: u16,
    pub edid_product_code_id: u16,
    pub connector_instance: u32,
    pub monitor_friendly_device_name: [u16; 64],
    pub monitor_device_path: [u16; 128],
}

#[repr(C)]
pub struct DisplayConfigAdapterName {
    pub header: DisplayConfigDeviceInfoHeader,
    pub adapter_device_path: [u16; 128],
}

#[repr(C)]
pub struct DisplayConfigSourceDeviceName {
    pub header: DisplayConfigDeviceInfoHeader,
    pub view_gdi_device_name: [u16; 32],
}

pub struct CcdAbi;

impl CcdAbi {
    pub const MODE_UNION_OFFSET: usize = offset_of!(DisplayConfigModeInfo, union);

    pub fn ensure_expected_layout() -> Result<(), String> {
        let expected = [
            ("LUID", size_of::<Luid>(), 8),
            (
                "DISPLAYCONFIG_PATH_SOURCE_INFO",
                size_of::<DisplayConfigPathSourceInfo>(),
                20,
            ),
            (
                "DISPLAYCONFIG_PATH_TARGET_INFO",
                size_of::<DisplayConfigPathTargetInfo>(),
                48,
            ),
            (
                "DISPLAYCONFIG_PATH_INFO",
                size_of::<DisplayConfigPathInfo>(),
                72,
            ),
            (
                "DISPLAYCONFIG_VIDEO_SIGNAL_INFO",
                size_of::<DisplayConfigVideoSignalInfo>(),
                48,
            ),
            (
                "DISPLAYCONFIG_MODE_INFO",
                size_of::<DisplayConfigModeInfo>(),
                64,
            ),
        ];
        for (name, actual, want) in expected {
            if actual != want {
                return Err(format!(
                    "unexpected Windows ABI layout for {name}: {actual} != {want}"
                ));
            }
        }
        if size_of::<usize>() != 8 || Self::MODE_UNION_OFFSET != 16 {
            return Err(format!(
                "unexpected Windows ABI layout: pointer={}, modeOffset={}",
                size_of::<usize>(),
                Self::MODE_UNION_OFFSET
            ));
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct CcdFrame {
    pub paths: Vec<DisplayConfigPathInfo>,
    pub modes: Vec<DisplayConfigModeInfo>,
    pub snapshot: DisplaySnapshot,
}

pub trait CcdApi {
    fn connected_physical(&self) -> Result<Vec<crate::ScreenIdentity>, String> {
        let frame = self.capture(
            1 | CcdConstants::QDC_VIRTUAL_MODE_AWARE | CcdConstants::QDC_VIRTUAL_REFRESH_RATE_AWARE,
        )?;
        let mut connected = Vec::new();
        for (path, row) in frame.paths.iter().zip(frame.snapshot.paths.iter()) {
            if path.target_info.target_available != 0 && row.is_physical() {
                if row.monitor_path.is_empty() {
                    return Err("无法确认物理屏身份。".into());
                }
                connected.push(row.identity());
            }
        }
        Ok(connected)
    }
    fn query_raw(
        &self,
        flags: u32,
    ) -> Result<(Vec<DisplayConfigPathInfo>, Vec<DisplayConfigModeInfo>), String>;
    fn capture(&self, flags: u32) -> Result<CcdFrame, String>;
    fn set(
        &self,
        paths: &[DisplayConfigPathInfo],
        modes: &[DisplayConfigModeInfo],
        flags: u32,
    ) -> Result<i32, String>;
    fn set_topology(&self, topology_flags: u32) -> Result<i32, String>;
    fn query_snapshot(&self, flags: u32) -> Result<DisplaySnapshot, String> {
        Ok(self.capture(flags)?.snapshot)
    }
    fn last_error_message(&self, code: i32) -> String {
        win32_message(code)
    }
}

pub trait Hotkey {
    fn try_register(&mut self) -> bool;
    fn unregister(&mut self);
    fn was_pressed(&mut self) -> bool;
}

pub trait MonotonicClock {
    fn seconds(&self) -> f64;
}

pub trait ParentWatcher {
    fn is_alive(&self, pid: i32) -> Result<bool, String>;
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PowerEvent {
    #[default]
    None,
    Suspending,
    Resumed,
    DisplayState(u32),
}

pub trait PowerObserver {
    fn poll(&mut self) -> PowerEvent;
}

const GUID_CONSOLE_DISPLAY_STATE: windows_sys::core::GUID =
    windows_sys::core::GUID::from_u128(0x6fe69556_704a_47a0_8f24_c28d936fda47);

struct PowerNotifyState {
    suspending: AtomicBool,
    resumed: AtomicBool,
    display_state: AtomicU32,
}

impl Default for PowerNotifyState {
    fn default() -> Self {
        Self {
            suspending: AtomicBool::new(false),
            resumed: AtomicBool::new(false),
            display_state: AtomicU32::new(u32::MAX),
        }
    }
}

pub struct Win32PowerObserver {
    state: Box<PowerNotifyState>,
    _suspend_params: DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS,
    _display_params: DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS,
    suspend_handle: HPOWERNOTIFY,
    display_handle: HPOWERNOTIFY,
}

impl Win32PowerObserver {
    pub fn new() -> Self {
        let state = Box::new(PowerNotifyState::default());
        let context = (&*state) as *const PowerNotifyState as *mut c_void;
        let mut suspend_params = DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS {
            Callback: Some(power_notify_callback),
            Context: context,
        };
        let mut display_params = DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS {
            Callback: Some(power_notify_callback),
            Context: context,
        };
        let mut suspend_raw: *mut c_void = ptr::null_mut();
        let suspend_rc = unsafe {
            PowerRegisterSuspendResumeNotification(
                DEVICE_NOTIFY_CALLBACK,
                &mut suspend_params as *mut DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS as HANDLE,
                &mut suspend_raw,
            )
        };
        let mut display_raw: *mut c_void = ptr::null_mut();
        let display_rc = unsafe {
            PowerSettingRegisterNotification(
                &GUID_CONSOLE_DISPLAY_STATE,
                DEVICE_NOTIFY_CALLBACK,
                &mut display_params as *mut DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS as HANDLE,
                &mut display_raw,
            )
        };
        Self {
            state,
            _suspend_params: suspend_params,
            _display_params: display_params,
            suspend_handle: if suspend_rc == 0 {
                suspend_raw as HPOWERNOTIFY
            } else {
                0
            },
            display_handle: if display_rc == 0 {
                display_raw as HPOWERNOTIFY
            } else {
                0
            },
        }
    }
}

impl Default for Win32PowerObserver {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Win32PowerObserver {
    fn drop(&mut self) {
        if self.suspend_handle != 0 {
            unsafe {
                PowerUnregisterSuspendResumeNotification(self.suspend_handle);
            }
            self.suspend_handle = 0;
        }
        if self.display_handle != 0 {
            unsafe {
                PowerSettingUnregisterNotification(self.display_handle);
            }
            self.display_handle = 0;
        }
    }
}

impl PowerObserver for Win32PowerObserver {
    fn poll(&mut self) -> PowerEvent {
        if self.state.suspending.swap(false, Ordering::SeqCst) {
            return PowerEvent::Suspending;
        }
        if self.state.resumed.swap(false, Ordering::SeqCst) {
            return PowerEvent::Resumed;
        }
        match self.state.display_state.swap(u32::MAX, Ordering::SeqCst) {
            value @ 0..=2 => PowerEvent::DisplayState(value),
            _ => PowerEvent::None,
        }
    }
}

unsafe extern "system" fn power_notify_callback(
    context: *const c_void,
    notify_type: u32,
    setting: *const c_void,
) -> u32 {
    if context.is_null() {
        return 0;
    }
    let state = unsafe { &*(context as *const PowerNotifyState) };
    match notify_type {
        PBT_APMSUSPEND => state.suspending.store(true, Ordering::SeqCst),
        PBT_APMRESUMESUSPEND | PBT_APMRESUMEAUTOMATIC => {
            state.resumed.store(true, Ordering::SeqCst);
        }
        PBT_POWERSETTINGCHANGE if !setting.is_null() => {
            let broadcast = unsafe { &*(setting as *const POWERBROADCAST_SETTING) };
            if guid_eq(&broadcast.PowerSetting, &GUID_CONSOLE_DISPLAY_STATE)
                && broadcast.DataLength >= 4
            {
                let value = unsafe { ptr::read_unaligned(broadcast.Data.as_ptr() as *const u32) };
                if value <= 2 {
                    state.display_state.store(value, Ordering::SeqCst);
                }
            }
        }
        _ => {}
    }
    0
}

fn guid_eq(a: &windows_sys::core::GUID, b: &windows_sys::core::GUID) -> bool {
    a.data1 == b.data1 && a.data2 == b.data2 && a.data3 == b.data3 && a.data4 == b.data4
}

#[cfg(test)]
mod power_tests {
    use super::*;

    #[test]
    fn ordinary_display_off_on_is_not_a_system_resume() {
        #[repr(C)]
        struct Setting {
            guid: windows_sys::core::GUID,
            len: u32,
            value: u32,
        }
        let state = PowerNotifyState::default();
        let context = &state as *const _ as *const c_void;
        let mut setting = Setting {
            guid: GUID_CONSOLE_DISPLAY_STATE,
            len: 4,
            value: 0,
        };
        unsafe {
            power_notify_callback(
                context,
                PBT_POWERSETTINGCHANGE,
                &setting as *const _ as *const c_void,
            );
        }
        std::thread::sleep(std::time::Duration::from_millis(2100));
        setting.value = 1;
        unsafe {
            power_notify_callback(
                context,
                PBT_POWERSETTINGCHANGE,
                &setting as *const _ as *const c_void,
            );
        }
        assert!(!state.resumed.load(Ordering::SeqCst));
        unsafe {
            power_notify_callback(context, PBT_APMRESUMEAUTOMATIC, ptr::null());
        }
        assert!(state.resumed.load(Ordering::SeqCst));
    }
}

#[derive(Default)]
pub struct SystemMonotonicClock;

impl MonotonicClock for SystemMonotonicClock {
    fn seconds(&self) -> f64 {
        TickClock.seconds()
    }
}

pub struct TickClock;

impl MonotonicClock for TickClock {
    fn seconds(&self) -> f64 {
        unsafe { windows_sys::Win32::System::SystemInformation::GetTickCount64() as f64 / 1000.0 }
    }
}

pub struct Win32CcdApi;

impl Win32CcdApi {
    pub fn describe(path: DisplayConfigPathInfo, index: i32) -> PathRow {
        let target = target_name(path);
        let adapter_path = adapter_name(path);
        let source_name = source_name(path);
        let monitor_path = target.path;
        let placeholder = monitor_path
            .to_ascii_uppercase()
            .contains("DEFAULT_MONITOR");
        let internal_tech = Roles::is_internal_technology(path.target_info.output_technology);
        let role = Roles::classify(
            placeholder,
            internal_tech,
            &adapter_path,
            &monitor_path,
            &target.name,
            &source_name,
        );
        PathRow {
            index,
            active: (path.flags & CcdConstants::DISPLAYCONFIG_PATH_ACTIVE) != 0,
            flags: path.flags,
            source_id: path.source_info.id,
            target_id: path.target_info.id,
            adapter_luid: path.target_info.adapter_id.to_hex(),
            output_technology: path.target_info.output_technology,
            internal: internal_tech,
            source_name,
            adapter_path,
            monitor_name: target.name,
            monitor_path,
            placeholder,
            role,
            edid_manufacture_id: target.edid_manufacture_id,
            edid_product_code_id: target.edid_product_code_id,
        }
    }
}

impl CcdApi for Win32CcdApi {
    fn query_raw(
        &self,
        flags: u32,
    ) -> Result<(Vec<DisplayConfigPathInfo>, Vec<DisplayConfigModeInfo>), String> {
        for _ in 0..8 {
            let mut path_count = 0u32;
            let mut mode_count = 0u32;
            let rc =
                unsafe { GetDisplayConfigBufferSizes(flags, &mut path_count, &mut mode_count) };
            if rc != CcdConstants::ERROR_SUCCESS {
                return Err(format!(
                    "GetDisplayConfigBufferSizes failed: {}",
                    win32_message(rc)
                ));
            }
            let mut paths = vec![DisplayConfigPathInfo::default(); path_count as usize];
            let mut modes = vec![DisplayConfigModeInfo::default(); mode_count as usize];
            let rc = unsafe {
                QueryDisplayConfig(
                    flags,
                    &mut path_count,
                    paths.as_mut_ptr(),
                    &mut mode_count,
                    modes.as_mut_ptr(),
                    ptr::null_mut(),
                )
            };
            if rc == CcdConstants::ERROR_INSUFFICIENT_BUFFER {
                continue;
            }
            if rc != CcdConstants::ERROR_SUCCESS {
                return Err(format!("QueryDisplayConfig failed: {}", win32_message(rc)));
            }
            paths.truncate(path_count as usize);
            modes.truncate(mode_count as usize);
            return Ok((paths, modes));
        }
        Err("QueryDisplayConfig buffer retry exhausted".into())
    }

    fn set(
        &self,
        paths: &[DisplayConfigPathInfo],
        modes: &[DisplayConfigModeInfo],
        flags: u32,
    ) -> Result<i32, String> {
        if flags & CcdConstants::SDC_SAVE_TO_DATABASE != 0 {
            return Err("SDC_SAVE_TO_DATABASE is forbidden".into());
        }
        Ok(unsafe {
            SetDisplayConfig(
                paths.len() as u32,
                if paths.is_empty() {
                    ptr::null()
                } else {
                    paths.as_ptr()
                },
                modes.len() as u32,
                if modes.is_empty() {
                    ptr::null()
                } else {
                    modes.as_ptr()
                },
                flags,
            )
        })
    }

    fn set_topology(&self, topology_flags: u32) -> Result<i32, String> {
        if topology_flags & CcdConstants::SDC_SAVE_TO_DATABASE != 0 {
            return Err("SDC_SAVE_TO_DATABASE is forbidden".into());
        }
        Ok(unsafe { SetDisplayConfig(0, ptr::null(), 0, ptr::null(), topology_flags) })
    }

    fn capture(&self, flags: u32) -> Result<CcdFrame, String> {
        let (paths, modes) = self.query_raw(flags)?;
        let rows = paths
            .iter()
            .enumerate()
            .map(|(i, p)| Self::describe(*p, i as i32))
            .collect();
        let gdi = unsafe { GetSystemMetrics(CcdConstants::SM_CMONITORS) };
        Ok(CcdFrame {
            paths,
            modes,
            snapshot: DisplaySnapshot::new(rows, gdi),
        })
    }
}

fn target_name(path: DisplayConfigPathInfo) -> TargetInfo {
    let mut info = unsafe { std::mem::zeroed::<DisplayConfigTargetDeviceName>() };
    info.header = DisplayConfigDeviceInfoHeader {
        info_type: CcdConstants::DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
        size: size_of::<DisplayConfigTargetDeviceName>() as u32,
        adapter_id: path.target_info.adapter_id,
        id: path.target_info.id,
    };
    let rc = unsafe { DisplayConfigGetDeviceInfo(ptr::addr_of_mut!(info) as *mut _) };
    if rc != 0 {
        return TargetInfo::default();
    }
    TargetInfo {
        name: utf16_z(&info.monitor_friendly_device_name),
        path: utf16_z(&info.monitor_device_path),
        edid_manufacture_id: info.edid_manufacture_id,
        edid_product_code_id: info.edid_product_code_id,
    }
}

fn adapter_name(path: DisplayConfigPathInfo) -> String {
    let mut info = unsafe { std::mem::zeroed::<DisplayConfigAdapterName>() };
    info.header = DisplayConfigDeviceInfoHeader {
        info_type: CcdConstants::DISPLAYCONFIG_DEVICE_INFO_GET_ADAPTER_NAME,
        size: size_of::<DisplayConfigAdapterName>() as u32,
        adapter_id: path.target_info.adapter_id,
        id: path.target_info.id,
    };
    let rc = unsafe { DisplayConfigGetDeviceInfo(ptr::addr_of_mut!(info) as *mut _) };
    if rc == 0 {
        utf16_z(&info.adapter_device_path)
    } else {
        String::new()
    }
}

fn source_name(path: DisplayConfigPathInfo) -> String {
    let mut info = unsafe { std::mem::zeroed::<DisplayConfigSourceDeviceName>() };
    info.header = DisplayConfigDeviceInfoHeader {
        info_type: CcdConstants::DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
        size: size_of::<DisplayConfigSourceDeviceName>() as u32,
        adapter_id: path.source_info.adapter_id,
        id: path.source_info.id,
    };
    let rc = unsafe { DisplayConfigGetDeviceInfo(ptr::addr_of_mut!(info) as *mut _) };
    if rc == 0 {
        utf16_z(&info.view_gdi_device_name)
    } else {
        String::new()
    }
}

#[derive(Default)]
struct TargetInfo {
    name: String,
    path: String,
    edid_manufacture_id: u16,
    edid_product_code_id: u16,
}

fn utf16_z(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

pub fn win32_message(code: i32) -> String {
    use windows_sys::Win32::System::Diagnostics::Debug::{
        FormatMessageW, FORMAT_MESSAGE_FROM_SYSTEM, FORMAT_MESSAGE_IGNORE_INSERTS,
    };
    let mut buffer = [0u16; 1024];
    let n = unsafe {
        FormatMessageW(
            FORMAT_MESSAGE_FROM_SYSTEM | FORMAT_MESSAGE_IGNORE_INSERTS,
            ptr::null(),
            code as u32,
            0,
            buffer.as_mut_ptr(),
            buffer.len() as u32,
            ptr::null_mut(),
        )
    };
    if n == 0 {
        return code.to_string();
    }
    let text = String::from_utf16_lossy(&buffer[..n as usize])
        .trim()
        .to_string();
    if text.is_empty() {
        code.to_string()
    } else {
        format!("{code} {text}")
    }
}

#[link(name = "user32")]
extern "system" {
    fn GetDisplayConfigBufferSizes(
        flags: u32,
        num_path_array_elements: *mut u32,
        num_mode_info_array_elements: *mut u32,
    ) -> i32;
    fn QueryDisplayConfig(
        flags: u32,
        num_path_array_elements: *mut u32,
        path_array: *mut DisplayConfigPathInfo,
        num_mode_info_array_elements: *mut u32,
        mode_info_array: *mut DisplayConfigModeInfo,
        current_topology_id: *mut u32,
    ) -> i32;
    fn SetDisplayConfig(
        num_path_array_elements: u32,
        path_array: *const DisplayConfigPathInfo,
        num_mode_info_array_elements: u32,
        mode_info_array: *const DisplayConfigModeInfo,
        flags: u32,
    ) -> i32;
    fn DisplayConfigGetDeviceInfo(request_packet: *mut DisplayConfigDeviceInfoHeader) -> i32;
}

pub struct Win32Hotkey {
    registered: bool,
}

impl Default for Win32Hotkey {
    fn default() -> Self {
        Self { registered: false }
    }
}

impl Hotkey for Win32Hotkey {
    fn try_register(&mut self) -> bool {
        self.registered = unsafe {
            RegisterHotKey(
                0 as HWND,
                CcdConstants::HOTKEY_ID,
                CcdConstants::HOTKEY_MODIFIERS,
                CcdConstants::VK_F10,
            )
        } != 0;
        self.registered
    }

    fn unregister(&mut self) {
        if self.registered {
            unsafe { UnregisterHotKey(0 as HWND, CcdConstants::HOTKEY_ID) };
            self.registered = false;
        }
    }

    fn was_pressed(&mut self) -> bool {
        let mut found = false;
        let mut msg = unsafe { std::mem::zeroed::<MSG>() };
        while unsafe { PeekMessageW(&mut msg, 0 as HWND, WM_HOTKEY, WM_HOTKEY, PM_REMOVE) } != 0 {
            found |= msg.wParam == CcdConstants::HOTKEY_ID as WPARAM;
        }
        found
    }
}

pub struct Win32ParentWatcher;

impl ParentWatcher for Win32ParentWatcher {
    fn is_alive(&self, pid: i32) -> Result<bool, String> {
        if pid <= 0 {
            return Ok(false);
        }
        let handle: HANDLE =
            unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid as u32) };
        if handle.is_null() {
            return Ok(false);
        }
        let mut code = 0u32;
        let ok = unsafe { GetExitCodeProcess(handle, &mut code) };
        unsafe { CloseHandle(handle) };
        if ok == 0 {
            let _ = unsafe { GetLastError() };
            return Ok(false);
        }
        Ok(code == 259)
    }
}

impl MonotonicClock for fn() -> f64 {
    fn seconds(&self) -> f64 {
        self()
    }
}
