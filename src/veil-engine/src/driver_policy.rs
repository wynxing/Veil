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
}
