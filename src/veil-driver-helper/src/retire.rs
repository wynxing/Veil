//! 旧版 MSI 的 RestoreDisplays 会在升级时失败。这里复制已缓存的 MSI，
//! 关掉该动作后再按产品代码卸载，让新包可以按首次安装继续。
use std::ffi::OsString;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::Command;
use windows_sys::Win32::Foundation::ERROR_NO_MORE_ITEMS;
use windows_sys::Win32::System::ApplicationInstallationAndServicing::{
    MsiCloseHandle, MsiDatabaseCommit, MsiDatabaseOpenViewW, MsiEnumRelatedProductsW,
    MsiGetProductInfoW, MsiOpenDatabaseW, MsiViewClose, MsiViewExecute,
    INSTALLPROPERTY_LOCALPACKAGE, MSIDBOPEN_TRANSACT,
};

const UPGRADE_CODE: &str = "{9E2C1A0B-4D7E-4C3A-9B11-7F0D2A91E001}";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn retire_old_products() -> Result<(), String> {
    let mut last = None;
    for product in related_products()? {
        match retire_product(&product) {
            Ok(()) => {}
            Err(e) => last = Some(format!("{product}: {e}")),
        }
    }
    match last {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

fn related_products() -> Result<Vec<String>, String> {
    let upgrade = wide(UPGRADE_CODE);
    let mut products = Vec::new();
    for index in 0..32 {
        let mut buf = [0u16; 40];
        let rc = unsafe { MsiEnumRelatedProductsW(upgrade.as_ptr(), 0, index, buf.as_mut_ptr()) };
        if rc == ERROR_NO_MORE_ITEMS {
            break;
        }
        if rc != 0 {
            // 干净机器或枚举失败都不应挡住新包安装。
            return Ok(products);
        }
        products.push(from_wide_z(&buf));
    }
    Ok(products)
}

fn retire_product(product: &str) -> Result<(), String> {
    let local = product_info(product, INSTALLPROPERTY_LOCALPACKAGE)?;
    if local.is_empty() || !Path::new(&local).exists() {
        return Err("找不到已缓存的旧安装包。".into());
    }
    disable_restore_displays(Path::new(&local))?;
    let log = std::env::temp_dir().join("Veil-retire-old.msi.log");
    let status = Command::new("msiexec")
        .args([
            "/x",
            product,
            "/qn",
            "REBOOT=ReallySuppress",
            "/l*v",
            &log.to_string_lossy(),
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map_err(|e| format!("无法启动 msiexec：{e}"))?;
    if !status.success() {
        return Err(format!(
            "卸载旧版失败：{}，日志 {}",
            status.code().unwrap_or(-1),
            log.display()
        ));
    }
    Ok(())
}

fn disable_restore_displays(path: &Path) -> Result<(), String> {
    let wide_path = wide(&path.to_string_lossy());
    let mut db = 0;
    let rc = unsafe { MsiOpenDatabaseW(wide_path.as_ptr(), MSIDBOPEN_TRANSACT, &mut db) };
    if rc != 0 {
        return Err(format!("无法打开旧安装包：{rc}"));
    }
    let result = (|| {
        let sql = wide(
            "UPDATE `InstallExecuteSequence` SET `Condition`='0' WHERE `Action`='RestoreDisplays'",
        );
        let mut view = 0;
        let rc = unsafe { MsiDatabaseOpenViewW(db, sql.as_ptr(), &mut view) };
        if rc != 0 {
            return Err(format!("无法改旧卸载序列：{rc}"));
        }
        let exec = unsafe { MsiViewExecute(view, 0) };
        unsafe {
            MsiViewClose(view);
            MsiCloseHandle(view);
        }
        if exec != 0 {
            return Err(format!("无法关闭旧 RestoreDisplays：{exec}"));
        }
        let rc = unsafe { MsiDatabaseCommit(db) };
        if rc != 0 {
            return Err(format!("无法保存旧安装包：{rc}"));
        }
        Ok(())
    })();
    unsafe {
        MsiCloseHandle(db);
    }
    result
}

fn product_info(product: &str, property: windows_sys::core::PCWSTR) -> Result<String, String> {
    let product = wide(product);
    let mut chars = 0u32;
    let probe =
        unsafe { MsiGetProductInfoW(product.as_ptr(), property, std::ptr::null_mut(), &mut chars) };
    if probe != 0 && probe != 234 {
        return Err(format!("无法读取旧产品信息：{probe}"));
    }
    let mut buf = vec![0u16; (chars as usize).saturating_add(2)];
    chars = buf.len() as u32;
    let rc =
        unsafe { MsiGetProductInfoW(product.as_ptr(), property, buf.as_mut_ptr(), &mut chars) };
    if rc != 0 {
        return Err(format!("无法读取旧产品路径：{rc}"));
    }
    Ok(from_wide_z(&buf))
}

fn wide(s: &str) -> Vec<u16> {
    std::ffi::OsStr::new(s)
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

pub fn stop_veil_apps() {
    for name in [
        "Veil.App.exe",
        "Veil.Recovery.exe",
        "veil_app.exe",
        "veil_recovery.exe",
    ] {
        let _ = Command::new("taskkill")
            .args(["/IM", name, "/F"])
            .creation_flags(CREATE_NO_WINDOW)
            .status();
    }
}
