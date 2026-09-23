fn main() {
    println!("cargo:rerun-if-changed=app.manifest");
    let icon =
        std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"))
            .join("../../assets/icon/veil.ico");
    println!("cargo:rerun-if-changed={}", icon.display());
    let informational = informational_version();
    println!("cargo:rustc-env=VEIL_INFORMATIONAL_VERSION={informational}");

    let mut res = winresource::WindowsResource::new();
    res.set_manifest_file("app.manifest");
    res.set_icon(icon.to_str().expect("icon path"));
    stamp_version(&mut res);
    let _ = res.compile();
}

fn stamp_version(res: &mut winresource::WindowsResource) {
    let version = std::env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.0.0".into());
    let file_version = format!("{version}.0");
    res.set("FileVersion", &file_version);
    res.set("ProductVersion", &file_version);
    res.set("ProductName", "Veil");
    res.set("FileDescription", "Veil");
}

fn informational_version() -> String {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let path = std::path::Path::new(&manifest_dir).join("../version.props");
    println!("cargo:rerun-if-changed={}", path.display());
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()));
    let version =
        xml_value(&text, "Version").unwrap_or_else(|| panic!("version.props is missing <Version>"));
    if !is_numeric_version(&version) {
        panic!("version.props Version must be Major.Minor.Build, got {version}");
    }
    match xml_value(&text, "VersionSuffix") {
        Some(suffix) if !suffix.is_empty() => {
            if suffix
                .chars()
                .any(|c| c.is_whitespace() || c == '"' || c == '\\')
            {
                panic!("version.props VersionSuffix contains unsupported characters");
            }
            format!("{version}-{suffix}")
        }
        _ => version,
    }
}

fn xml_value(text: &str, tag: &str) -> Option<String> {
    let start = format!("<{tag}>");
    let end = format!("</{tag}>");
    let from = text.find(&start)? + start.len();
    let to = text[from..].find(&end)? + from;
    Some(text[from..to].trim().to_string())
}

fn is_numeric_version(value: &str) -> bool {
    let mut parts = value.split('.');
    let three = [parts.next(), parts.next(), parts.next()];
    parts.next().is_none()
        && three.iter().all(|part| {
            part.is_some_and(|text| !text.is_empty() && text.chars().all(|c| c.is_ascii_digit()))
        })
}
