fn main() {
    println!("cargo:rerun-if-changed=app.manifest");
    let mut res = winresource::WindowsResource::new();
    res.set_manifest_file("app.manifest");
    let _ = res.compile();
}
