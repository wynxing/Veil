pub fn installation_result(install: i32, disable: i32, count: usize) -> Result<(), String> {
    if install != 0 || disable != 0 || count == 0 {
        Err(format!(
            "安装未完成：install={install}, disable={disable}, devices={count}"
        ))
    } else {
        Ok(())
    }
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
}
