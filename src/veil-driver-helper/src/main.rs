use sha2::{Digest, Sha256};
use std::ffi::{OsStr, OsString};
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::ptr;
use veil_engine::driver_policy::installation_result;
use veil_engine::{BundledVddSettings, CcdApi, CcdConstants, Win32CcdApi};
use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
    CM_Disable_DevNode, CM_Enable_DevNode, CM_Locate_DevNodeW, SetupDiDestroyDeviceInfoList,
    SetupDiEnumDeviceInfo, SetupDiGetClassDevsW, SetupDiGetDeviceInstanceIdW,
    SetupDiGetDeviceRegistryPropertyW, SPDRP_HARDWAREID, SP_DEVINFO_DATA,
};
mod retire;
use windows_sys::Win32::Foundation::{GetLastError, ERROR_NO_MORE_ITEMS, INVALID_HANDLE_VALUE};

const HARDWARE_ID: &str = CcdConstants::BUNDLED_HARDWARE_ID;
const PUBLISHER_THUMBPRINT: &str = "3CF8CF26D8BA266C3A483AB7D26D4A818E317D76";
const DISPLAY_CLASS: windows_sys::core::GUID = windows_sys::core::GUID {
    data1: 0x4d36e968,
    data2: 0xe325,
    data3: 0x11ce,
    data4: [0xbf, 0xc1, 0x08, 0x00, 0x2b, 0xe1, 0x03, 0x18],
};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let verb = args.first().map(String::as_str).unwrap_or("status");
    if verb == "--help" || verb == "-h" {
        eprintln!(
            "Veil.DriverHelper status|enable|disable|install-driver|uninstall-driver|sweep-sessions|retire-old"
        );
        std::process::exit(0);
    }
    let code = match verb {
        "restore-displays" => match veil_engine::maintenance::restore_in_user_session(
            &program_files_veil().join("Veil.App.exe"),
        ) {
            Ok(code) => code,
            Err(e) => {
                helper_log(&e);
                1
            }
        },
        "sweep-sessions" => sweep_sessions(),
        "retire-old" => {
            retire::stop_veil_apps();
            if sweep_sessions() != 0 {
                helper_log("sweep-sessions 未完成，继续卸旧版。");
            }
            match retire::retire_old_products() {
                Ok(()) => 0,
                Err(e) => {
                    helper_log(&e);
                    1
                }
            }
        }
        "begin-maintenance" => maintenance(true),
        "end-maintenance" => maintenance(false),
        "status" => status(),
        "enable" => enable_all(),
        "disable" => disable_all(),
        "install-driver" => install_driver(),
        "uninstall-driver" => uninstall_driver(),
        _ => {
            eprintln!("unknown verb {verb}");
            2
        }
    };
    std::process::exit(code);
}

fn status() -> i32 {
    let ids = match find_instance_ids() {
        Ok(ids) => ids,
        Err(e) => {
            helper_log(&e);
            return 2;
        }
    };
    println!(
        "{}",
        serde_json::json!({
            "hardwareId": HARDWARE_ID,
            "instances": ids,
            "installed": !ids.is_empty(),
        })
    );
    0
}

fn sweep_sessions() -> i32 {
    let mut roots = vec![veil_engine::SessionPaths::root()];
    match veil_engine::maintenance::interactive_user_veil_dir() {
        Ok(user) => {
            if !roots.iter().any(|root| root == &user) {
                roots.push(user);
            }
        }
        Err(e) => helper_log(&e),
    }
    for root in roots {
        if let Err(e) = veil_engine::OpenSessionRelease::sweep_concluded(&root) {
            helper_log(&e);
            return 1;
        }
    }
    0
}

fn maintenance(active: bool) -> i32 {
    match veil_engine::maintenance::set_active(active) {
        Ok(()) => 0,
        Err(e) => {
            helper_log(&e);
            2
        }
    }
}

fn ownership_path() -> PathBuf {
    program_files_veil().join("owned-devices.json")
}
fn owned_ids() -> Result<Vec<String>, String> {
    let path = ownership_path();
    if !path.exists() {
        return Ok(vec![]);
    }
    veil_engine::JsonUtil::read(path)
}
fn checked_owned_ids() -> Result<Vec<String>, String> {
    let owned = owned_ids()?;
    let current = find_instance_ids()?;
    Ok(owned
        .into_iter()
        .filter(|id| current.iter().any(|c| c.eq_ignore_ascii_case(id)))
        .collect())
}
fn install_driver() -> i32 {
    match install_driver_inner() {
        Ok(()) => 0,
        Err(e) => {
            helper_log(&e);
            2
        }
    }
}
fn install_driver_inner() -> Result<(), String> {
    let before = find_instance_ids()?;
    if !before.is_empty() {
        return Err("已有 MTT VDD 设备，无法证明由本次安装创建；保留原设备并停止安装。".into());
    }
    let payload = resolve_payload().unwrap_or_else(|| {
        let program = program_files_veil();
        (
            program.join("vdd"),
            program.join("nefcon").join("x64").join("nefconc.exe"),
        )
    });
    validate_payload(&payload.0, &payload.1)?;
    let created = BundledVddSettings::write_xml(&[
        &payload.0.to_string_lossy(),
        BundledVddSettings::DRIVER_READS_DIRECTORY,
    ])?;
    let rc = run(
        &payload.1,
        &format!(
            "install \"{}\" {HARDWARE_ID} --no-duplicates",
            payload.0.join("MttVDD.inf").display()
        ),
    );
    std::thread::sleep(std::time::Duration::from_secs(2));
    // Store exact instance IDs even after partial installation so rollback never matches all MTT devices.
    let ids = find_instance_ids()?;
    veil_engine::JsonUtil::write_atomic(ownership_path(), &ids)?;
    let disable_rc = ids.iter().fold(0, |rc, id| rc | change_state(id, false));
    if let Err(error) = installation_result(rc, disable_rc, ids.len()) {
        let cleanup = remove_instances(&ids);
        if cleanup == 0 {
            let _ = std::fs::remove_file(ownership_path());
            BundledVddSettings::rollback_created(&created);
        }
        return Err(error);
    }
    Ok(())
}
fn remove_instances(ids: &[String]) -> i32 {
    let pnputil = PathBuf::from(std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".into()))
        .join("System32")
        .join("pnputil.exe");
    ids.iter().fold(0, |rc, id| {
        rc | run(&pnputil, &format!("/remove-device \"{id}\""))
    })
}
fn uninstall_driver() -> i32 {
    if !ownership_path().exists() {
        return 0;
    }
    let ids = match checked_owned_ids() {
        Ok(ids) => ids,
        Err(e) => {
            helper_log(&e);
            return 2;
        }
    };
    let rc = ids.iter().fold(0, |rc, id| rc | change_state(id, false));
    if rc != 0 {
        return rc;
    }
    let rc = remove_instances(&ids);
    if rc == 0 {
        let _ = std::fs::remove_file(ownership_path());
        BundledVddSettings::try_remove_owned_file(BundledVddSettings::DRIVER_READS_DIRECTORY);
    }
    rc
}
fn enable_all() -> i32 {
    let _guard = match veil_engine::maintenance::OperationLock::for_keep_off() {
        Ok(g) => g,
        Err(e) => {
            helper_log(&e);
            return 2;
        }
    };
    if !payload_present() {
        helper_log("自带 VDD 文件缺失或哈希不符，拒绝启用。");
        return 2;
    }
    let ids = match checked_owned_ids() {
        Ok(ids) => ids,
        Err(e) => {
            helper_log(&e);
            return 2;
        }
    };
    if ids.is_empty() {
        helper_log("没有可证明属于 Veil 的设备，拒绝启用。");
        return 2;
    }
    ids.iter().fold(0, |rc, id| rc | change_state(id, true))
}
fn disable_all() -> i32 {
    // Recheck after the UAC round trip, immediately before changing device state.
    match Win32CcdApi.query_snapshot(CcdConstants::QUERY_FLAGS) {
        Ok(snapshot) if snapshot.active_physical().next().is_some() => {}
        _ => {
            helper_log("未确认活动物理输出，保留辅助 VDD。");
            return 2;
        }
    }
    let ids = match checked_owned_ids() {
        Ok(ids) => ids,
        Err(e) => {
            helper_log(&e);
            return 2;
        }
    };
    ids.iter().fold(0, |rc, id| rc | change_state(id, false))
}

fn change_state(instance_id: &str, enable: bool) -> i32 {
    let wide = to_wide(instance_id);
    let mut dev_inst = 0u32;
    let locate = unsafe { CM_Locate_DevNodeW(&mut dev_inst, wide.as_ptr(), 0) };
    if locate != 0 {
        eprintln!("CM_Locate_DevNode {instance_id} -> {locate}");
        return 1;
    }
    let rc = if enable {
        unsafe { CM_Enable_DevNode(dev_inst, 0) }
    } else {
        unsafe { CM_Disable_DevNode(dev_inst, 0) }
    };
    if rc != 0 {
        eprintln!(
            "{} {instance_id} -> {rc}",
            if enable { "enable" } else { "disable" }
        );
        return 1;
    }
    0
}

fn find_instance_ids() -> Result<Vec<String>, String> {
    let mut found = Vec::new();
    let mut guid = DISPLAY_CLASS;
    let set = unsafe { SetupDiGetClassDevsW(&mut guid, ptr::null(), ptr::null_mut(), 0) };
    if set == INVALID_HANDLE_VALUE as isize {
        return Err("无法枚举显示设备。".into());
    }
    let mut data: SP_DEVINFO_DATA = unsafe { std::mem::zeroed() };
    data.cbSize = std::mem::size_of::<SP_DEVINFO_DATA>() as u32;
    let mut i = 0u32;
    while unsafe { SetupDiEnumDeviceInfo(set, i, &mut data) } != 0 {
        i += 1;
        let ids = hardware_ids(set, &mut data);
        if !ids.iter().any(|id| id.eq_ignore_ascii_case(HARDWARE_ID)) {
            continue;
        }
        if let Some(instance) = instance_id(set, &mut data) {
            found.push(instance);
        }
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
    let u16s: Vec<u16> = buf
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    split_multi_sz(&u16s)
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
    for (i, &c) in buf.iter().enumerate() {
        if c == 0 {
            if i > start {
                out.push(String::from_utf16_lossy(&buf[start..i]));
            }
            start = i + 1;
            if i + 1 < buf.len() && buf[i + 1] == 0 {
                break;
            }
        }
    }
    out.into_iter().filter(|s| !s.is_empty()).collect()
}

fn payload_present() -> bool {
    let program = program_files_veil();
    let vdd = program.join("vdd");
    let dll = vdd.join("MttVDD.dll");
    let inf = vdd.join("MttVDD.inf");
    let cat = vdd.join("mttvdd.cat");
    if !dll.exists() || !inf.exists() || !cat.exists() {
        return false;
    }
    let mut manifest = program.join("payload.manifest.json");
    if !manifest.exists() {
        manifest = exe_dir().join("payload.manifest.json");
    }
    if !manifest.exists() {
        return false;
    }
    let Ok(text) = std::fs::read_to_string(&manifest) else {
        return false;
    };
    let Ok(doc) = serde_json::from_str::<serde_json::Value>(&text) else {
        return false;
    };
    let Some(files) = doc.get("files") else {
        return false;
    };
    let map = [
        ("vdd/MttVDD.dll", dll),
        ("vdd/MttVDD.inf", inf),
        ("vdd/mttvdd.cat", cat),
    ];
    for (key, path) in map {
        let Some(expected) = files.get(key).and_then(|v| v.as_str()) else {
            return false;
        };
        if sha256_file(&path) != expected.to_ascii_uppercase()
            && sha256_file(&path) != expected.to_ascii_lowercase()
        {
            let actual = sha256_file(&path);
            if !actual.eq_ignore_ascii_case(expected) {
                return false;
            }
        }
    }
    true
}

fn validate_payload(vdd_dir: &Path, nefcon: &Path) -> Result<(), String> {
    let manifest =
        find_manifest().ok_or_else(|| "缺少 payload.manifest.json，拒绝安装驱动。".to_string())?;
    let doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&manifest).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let thumb = doc
        .get("publisherThumbprint")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if thumb.is_empty() {
        return Err("manifest 缺少 publisherThumbprint".into());
    }
    if !thumb.eq_ignore_ascii_case(PUBLISHER_THUMBPRINT) {
        return Err("publisherThumbprint 与锁定指纹不符".into());
    }
    let files = doc.get("files").ok_or("manifest 缺少 files")?;
    let map = [
        ("vdd/mttvdd.cat", vdd_dir.join("mttvdd.cat")),
        ("vdd/MttVDD.dll", vdd_dir.join("MttVDD.dll")),
        ("vdd/MttVDD.inf", vdd_dir.join("MttVDD.inf")),
        ("nefcon/x64/nefconc.exe", nefcon.to_path_buf()),
    ];
    for (key, path) in map {
        if !path.exists() {
            return Err(format!("缺少 {key}"));
        }
        let expected = files.get(key).and_then(|v| v.as_str()).unwrap_or("");
        let actual = sha256_file(&path);
        if !actual.eq_ignore_ascii_case(expected) {
            return Err(format!("哈希不符：{key}"));
        }
    }
    Ok(())
}

fn find_manifest() -> Option<PathBuf> {
    let candidates = [
        exe_dir().join("payload.manifest.json"),
        program_files_veil().join("payload.manifest.json"),
        exe_dir()
            .join("..")
            .join("..")
            .join("..")
            .join("..")
            .join("..")
            .join("installer")
            .join("payload.manifest.json"),
    ];
    candidates.into_iter().find(|p| p.exists())
}

fn resolve_payload() -> Option<(PathBuf, PathBuf)> {
    let roots = [
        program_files_veil(),
        exe_dir()
            .join("..")
            .join("..")
            .join("..")
            .join("..")
            .join("..")
            .join("installer"),
        exe_dir(),
    ];
    for root in roots {
        let vdd = root.join("vdd");
        let nefcon = root.join("nefcon").join("x64").join("nefconc.exe");
        if vdd.is_dir() && nefcon.exists() {
            return Some((vdd, nefcon));
        }
    }
    None
}

fn run(file: &Path, args: &str) -> i32 {
    let status = std::process::Command::new(file)
        .args(split_args(args))
        .status();
    match status {
        Ok(s) => s.code().unwrap_or(1),
        Err(_) => 1,
    }
}

fn split_args(args: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_quote = false;
    for c in args.chars() {
        match c {
            '"' => in_quote = !in_quote,
            ' ' if !in_quote => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            _ => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn sha256_file(path: &Path) -> String {
    let bytes = std::fs::read(path).unwrap_or_default();
    hex_upper(&Sha256::digest(bytes))
}

fn hex_upper(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect()
}

fn helper_log(line: &str) {
    let paths = [
        std::env::temp_dir().join("Veil-driver-helper.log"),
        PathBuf::from(r"C:\ProgramData\Veil\driver-helper.log"),
    ];
    for path in paths {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            use std::io::Write;
            let _ = writeln!(f, "{line}");
        }
    }
    eprintln!("{line}");
}

fn program_files_veil() -> PathBuf {
    let program = std::env::var("ProgramFiles").unwrap_or_else(|_| r"C:\Program Files".into());
    PathBuf::from(program).join("Veil")
}

fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn to_wide(s: &str) -> Vec<u16> {
    OsStr::new(s)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

fn from_wide_z(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    OsString::from_wide(&buf[..end])
        .to_string_lossy()
        .into_owned()
}
