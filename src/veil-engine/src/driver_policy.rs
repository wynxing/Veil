pub fn installation_result(install: i32, disable: i32, count: usize) -> Result<(), String> {
    if install != 0 || disable != 0 || count == 0 {
        Err(format!(
            "安装未完成：install={install}, disable={disable}, devices={count}"
        ))
    } else {
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InstallDriverPlan {
    Adopt(Vec<String>),
    CreateDevice,
}

pub fn plan_install_driver(existing_instance_ids: &[String]) -> InstallDriverPlan {
    if existing_instance_ids.is_empty() {
        InstallDriverPlan::CreateDevice
    } else {
        InstallDriverPlan::Adopt(existing_instance_ids.to_vec())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EnableDriverPlan {
    Enable(Vec<String>),
    AdoptThenEnable(Vec<String>),
    Nothing,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DisplayDevice {
    pub instance_id: String,
    pub hardware_ids: Vec<String>,
}

pub fn bundled_instance_ids(devices: &[DisplayDevice]) -> Vec<String> {
    let hardware = crate::CcdConstants::BUNDLED_HARDWARE_ID;
    devices
        .iter()
        .filter(|device| {
            device
                .hardware_ids
                .iter()
                .any(|id| id.eq_ignore_ascii_case(hardware))
        })
        .map(|device| device.instance_id.clone())
        .collect()
}

pub fn adapter_matches_bundled_instance(adapter_path: &str, instance_id: &str) -> bool {
    let token = normalize_device_path(instance_id);
    if token.is_empty() {
        return false;
    }
    let haystack = normalize_device_path(adapter_path);
    let Some(at) = haystack.find(&token) else {
        return false;
    };
    haystack[at + token.len()..]
        .chars()
        .next()
        .map(|c| !c.is_ascii_alphanumeric())
        .unwrap_or(true)
}

fn normalize_device_path(value: &str) -> String {
    value.trim().replace('\\', "#").to_lowercase()
}

pub fn plan_enable_driver(owned: &[String], current: &[String]) -> EnableDriverPlan {
    let matched: Vec<String> = owned
        .iter()
        .filter(|id| current.iter().any(|c| c.eq_ignore_ascii_case(id)))
        .cloned()
        .collect();
    if !matched.is_empty() {
        return EnableDriverPlan::Enable(matched);
    }
    if !current.is_empty() {
        return EnableDriverPlan::AdoptThenEnable(current.to_vec());
    }
    EnableDriverPlan::Nothing
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn install_requires_disabled_device() {
        assert!(installation_result(0, 1, 1).is_err());
        assert!(installation_result(0, 0, 0).is_err());
        assert!(installation_result(1, 0, 1).is_err());
        assert!(installation_result(0, 0, 1).is_ok());
    }

    #[test]
    fn existing_instances_are_adopted_without_creating() {
        assert_eq!(
            plan_install_driver(&["ROOT\\MttVDD\\0000".into()]),
            InstallDriverPlan::Adopt(vec!["ROOT\\MttVDD\\0000".into()])
        );
        assert_eq!(plan_install_driver(&[]), InstallDriverPlan::CreateDevice);
    }

    #[test]
    fn enable_adopts_unowned_present_devices() {
        assert_eq!(
            plan_enable_driver(&[], &["ROOT\\MttVDD\\0000".into()]),
            EnableDriverPlan::AdoptThenEnable(vec!["ROOT\\MttVDD\\0000".into()])
        );
        assert_eq!(
            plan_enable_driver(
                &["ROOT\\MttVDD\\0000".into()],
                &["ROOT\\MttVDD\\0000".into()]
            ),
            EnableDriverPlan::Enable(vec!["ROOT\\MttVDD\\0000".into()])
        );
        assert_eq!(
            plan_enable_driver(&["gone".into()], &[]),
            EnableDriverPlan::Nothing
        );
        assert_eq!(plan_enable_driver(&[], &[]), EnableDriverPlan::Nothing);
    }

    #[test]
    fn display_instance_with_mtt_hardware_id_counts_as_installed() {
        let devices = [
            DisplayDevice {
                instance_id: r"ROOT\DISPLAY\0002".into(),
                hardware_ids: vec![r"Root\MttVDD".into()],
            },
            DisplayDevice {
                instance_id: r"ROOT\DISPLAY\0000".into(),
                hardware_ids: vec!["Root\\GameViewer".into()],
            },
        ];
        assert_eq!(
            bundled_instance_ids(&devices),
            vec![r"ROOT\DISPLAY\0002".to_string()]
        );
    }

    #[test]
    fn idd_adapter_path_matches_display_instance_not_other_root_display() {
        assert!(adapter_matches_bundled_instance(
            r"\\?\ROOT#DISPLAY#0002#{e6f07b5f-ee97-4a90-b076-33f57bf4eaa7}",
            r"ROOT\DISPLAY\0002",
        ));
        assert!(!adapter_matches_bundled_instance(
            r"ROOT\DISPLAY\0000",
            r"ROOT\DISPLAY\0002",
        ));
        assert!(!adapter_matches_bundled_instance(
            r"\\?\ROOT#DISPLAY#00021#{guid}",
            r"ROOT\DISPLAY\0002",
        ));
    }
}
