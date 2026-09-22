use crate::driver_policy::{bundled_instance_ids, DisplayDevice};
use std::cell::RefCell;
use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::ptr;
use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
    SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInfo, SetupDiGetClassDevsW,
    SetupDiGetDeviceInstanceIdW, SetupDiGetDeviceRegistryPropertyW, SPDRP_HARDWAREID,
    SP_DEVINFO_DATA,
};
use windows_sys::Win32::Foundation::{GetLastError, ERROR_NO_MORE_ITEMS, INVALID_HANDLE_VALUE};

const DISPLAY_CLASS: windows_sys::core::GUID = windows_sys::core::GUID {
    data1: 0x4d36e968,
    data2: 0xe325,
    data3: 0x11ce,
    data4: [0xbf, 0xc1, 0x08, 0x00, 0x2b, 0xe1, 0x03, 0x18],
};

thread_local! {
    static OVERRIDE: RefCell<Option<Vec<String>>> = const { RefCell::new(None) };
}

pub struct BundledInstanceOverride;

impl Drop for BundledInstanceOverride {
    fn drop(&mut self) {
        OVERRIDE.with(|slot| *slot.borrow_mut() = None);
    }
}

pub fn override_bundled_instances(ids: Vec<String>) -> BundledInstanceOverride {
    OVERRIDE.with(|slot| *slot.borrow_mut() = Some(ids));
    BundledInstanceOverride
}

pub fn known_bundled_instance_ids() -> Vec<String> {
    if let Some(ids) = OVERRIDE.with(|slot| slot.borrow().clone()) {
        return ids;
    }
    enumerate_bundled_instances().unwrap_or_default()
}

pub fn enumerate_bundled_instances() -> Result<Vec<String>, String> {
    Ok(bundled_instance_ids(&enumerate_display_devices()?))
}

fn enumerate_display_devices() -> Result<Vec<DisplayDevice>, String> {
    let mut found = Vec::new();
    let mut guid = DISPLAY_CLASS;
    let set = unsafe { SetupDiGetClassDevsW(&mut guid, ptr::null(), ptr::null_mut(), 0) };
    if set == INVALID_HANDLE_VALUE as isize {
        return Err("无法枚举显示设备。".into());
    }
    let mut data: SP_DEVINFO_DATA = unsafe { std::mem::zeroed() };
    data.cbSize = std::mem::size_of::<SP_DEVINFO_DATA>() as u32;
    let mut index = 0u32;
    while unsafe { SetupDiEnumDeviceInfo(set, index, &mut data) } != 0 {
        index += 1;
        let Some(instance_id) = instance_id(set, &mut data) else {
            continue;
        };
        found.push(DisplayDevice {
            instance_id,
            hardware_ids: hardware_ids(set, &mut data),
        });
    }
    let enumeration_error = unsafe { GetLastError() };
    unsafe { SetupDiDestroyDeviceInfoList(set) };
    if enumeration_error != ERROR_NO_MORE_ITEMS {
        return Err(format!("设备枚举未完成：{enumeration_error}"));
    }
    Ok(found)
}

fn hardware_ids(set: isize, data: &mut SP_DEVINFO_DATA) -> Vec<String> {
    let mut size = 0u32;
    unsafe {
        SetupDiGetDeviceRegistryPropertyW(
            set,
            data,
            SPDRP_HARDWAREID,
            ptr::null_mut(),
            ptr::null_mut(),
            0,
            &mut size,
        );
    }
    if size == 0 {
        return vec![];
    }
    let mut buf = vec![0u8; size as usize];
    let ok = unsafe {
        SetupDiGetDeviceRegistryPropertyW(
            set,
            data,
            SPDRP_HARDWAREID,
            ptr::null_mut(),
            buf.as_mut_ptr(),
            size,
            ptr::null_mut(),
        )
    };
    if ok == 0 {
        return vec![];
    }
    let units: Vec<u16> = buf
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .collect();
    split_multi_sz(&units)
}

fn instance_id(set: isize, data: &mut SP_DEVINFO_DATA) -> Option<String> {
    let mut buf = vec![0u16; 1024];
    let ok = unsafe {
        SetupDiGetDeviceInstanceIdW(
            set,
            data,
            buf.as_mut_ptr(),
            buf.len() as u32,
            ptr::null_mut(),
        )
    };
    if ok == 0 {
        return None;
    }
    Some(from_wide_z(&buf))
}

fn split_multi_sz(buf: &[u16]) -> Vec<String> {
    let mut out = Vec::new();
    let mut start = 0;
    for (index, &unit) in buf.iter().enumerate() {
        if unit == 0 {
            if index > start {
                out.push(String::from_utf16_lossy(&buf[start..index]));
            }
            start = index + 1;
            if index + 1 < buf.len() && buf[index + 1] == 0 {
                break;
            }
        }
    }
    out.into_iter().filter(|item| !item.is_empty()).collect()
}

fn from_wide_z(buf: &[u16]) -> String {
    let end = buf.iter().position(|c| *c == 0).unwrap_or(buf.len());
    OsString::from_wide(&buf[..end])
        .to_string_lossy()
        .into_owned()
}
