fn main() {
    println!("cargo:rerun-if-changed=app.manifest");
    let mut res = winresource::WindowsResource::new();
    res.set_manifest_file("app.manifest");
    stamp_version(&mut res);
    let _ = res.compile();
}

fn stamp_version(res: &mut winresource::WindowsResource) {
    let version = std::env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.0.0".into());
    let file_version = format!("{version}.0");
    res.set("FileVersion", &file_version);
    res.set("ProductVersion", &file_version);
    res.set("ProductName", "Veil");
    res.set("FileDescription", "Veil Recovery");
}
