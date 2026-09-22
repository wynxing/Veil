use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use veil_engine::driver_policy::{
    installation_result, plan_enable_driver, plan_install_driver, EnableDriverPlan,
    InstallDriverPlan,
};
use veil_engine::{BundledVddSettings, CcdApi, CcdConstants, JsonUtil, Win32CcdApi};
use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
    CM_Disable_DevNode, CM_Enable_DevNode, CM_Locate_DevNodeW,
};
mod retire;

const HARDWARE_ID: &str = CcdConstants::BUNDLED_HARDWARE_ID;

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
    let payload = veil_engine::resolve_payload()
        .ok_or_else(|| "缺少已校验的辅助虚拟输出驱动包，拒绝安装。".to_string())?;
    veil_engine::validate_payload(&payload)?;
    let created = BundledVddSettings::write_xml(&[
        &payload.vdd_dir.to_string_lossy(),
        BundledVddSettings::DRIVER_READS_DIRECTORY,
    ])?;
    match plan_install_driver(&before) {
        InstallDriverPlan::Adopt(ids) => {
            JsonUtil::write_atomic(ownership_path(), &ids)?;
            Ok(())
        }
        InstallDriverPlan::CreateDevice => {
            let rc = run(
                &payload.nefcon,
                &format!(
                    "install \"{}\" {HARDWARE_ID} --no-duplicates",
                    payload.vdd_dir.join("MttVDD.inf").display()
                ),
            );
            std::thread::sleep(std::time::Duration::from_secs(2));
            let ids = find_instance_ids()?;
            JsonUtil::write_atomic(ownership_path(), &ids)?;
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
    }
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
    let owned = match owned_ids() {
        Ok(ids) => ids,
        Err(e) => {
            helper_log(&e);
            return 2;
        }
    };
    let current = match find_instance_ids() {
        Ok(ids) => ids,
        Err(e) => {
            helper_log(&e);
            return 2;
        }
    };
    let ids = match plan_enable_driver(&owned, &current) {
        EnableDriverPlan::Enable(ids) => ids,
        EnableDriverPlan::AdoptThenEnable(ids) => {
            if let Err(e) = JsonUtil::write_atomic(ownership_path(), &ids) {
                helper_log(&e);
                return 2;
            }
            ids
        }
        EnableDriverPlan::Nothing => {
            helper_log("没有可启用的辅助虚拟输出设备。");
            return 2;
        }
    };
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
    helper_log(&format!(
        "device-change-start instance={instance_id} enable={enable}"
    ));
    let wide = to_wide(instance_id);
    let mut dev_inst = 0u32;
    let locate = unsafe { CM_Locate_DevNodeW(&mut dev_inst, wide.as_ptr(), 0) };
    if locate != 0 {
        helper_log(&format!(
            "device-change-end instance={instance_id} locateRc={locate}"
        ));
        eprintln!("CM_Locate_DevNode {instance_id} -> {locate}");
        return 1;
    }
    let rc = if enable {
        unsafe { CM_Enable_DevNode(dev_inst, 0) }
    } else {
        unsafe { CM_Disable_DevNode(dev_inst, 0) }
    };
    helper_log(&format!(
        "device-change-end instance={instance_id} enable={enable} configRet={rc}"
    ));
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
    veil_engine::enumerate_bundled_instances()
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
    veil_engine::payload::program_files_veil()
}

fn to_wide(s: &str) -> Vec<u16> {
    OsStr::new(s)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}
