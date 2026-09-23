use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

pub const PUBLISHER_THUMBPRINT: &str = "3CF8CF26D8BA266C3A483AB7D26D4A818E317D76";
const REQUIRED_FILES: &[(&str, &str)] = &[
    ("vdd/mttvdd.cat", "mttvdd.cat"),
    ("vdd/MttVDD.dll", "MttVDD.dll"),
    ("vdd/MttVDD.inf", "MttVDD.inf"),
    ("nefcon/x64/nefconc.exe", "nefconc.exe"),
];

#[derive(Clone, Debug)]
pub struct ResolvedPayload {
    pub root: PathBuf,
    pub vdd_dir: PathBuf,
    pub nefcon: PathBuf,
    pub manifest: PathBuf,
}

pub fn program_files_veil() -> PathBuf {
    let program = std::env::var("ProgramFiles").unwrap_or_else(|_| r"C:\Program Files".into());
    PathBuf::from(program).join("Veil")
}

pub fn current_exe_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn default_search_roots() -> Vec<PathBuf> {
    let mut roots = roots_from_exe(&current_exe_dir());
    roots.push(program_files_veil());
    dedup_paths(roots)
}

pub fn roots_from_exe(exe_dir: &Path) -> Vec<PathBuf> {
    let mut out = vec![exe_dir.to_path_buf()];
    let mut cur = exe_dir.to_path_buf();
    for _ in 0..8 {
        let installer = cur.join("installer");
        if installer.join("payload.manifest.json").exists()
            || installer.join("payload").is_dir()
            || cur.join("payload.manifest.json").exists()
        {
            out.push(installer.join("payload"));
            out.push(installer);
            out.push(cur);
            break;
        }
        if !cur.pop() {
            break;
        }
    }
    dedup_paths(out)
}

pub fn payload_present() -> bool {
    payload_present_in(&default_search_roots())
}

pub fn payload_present_in(roots: &[PathBuf]) -> bool {
    resolve_in(roots).is_some()
}

pub fn resolve() -> Option<ResolvedPayload> {
    resolve_in(&default_search_roots())
}

pub fn resolve_in(roots: &[PathBuf]) -> Option<ResolvedPayload> {
    roots.iter().find_map(|root| try_resolve(root))
}

fn try_resolve(root: &Path) -> Option<ResolvedPayload> {
    for cand in [root.to_path_buf(), root.join("payload")] {
        let vdd = cand.join("vdd");
        let nefcon = cand.join("nefcon").join("x64").join("nefconc.exe");
        if !vdd.is_dir() || !nefcon.exists() {
            continue;
        }
        let Some(manifest) = find_manifest(&cand) else {
            continue;
        };
        if validate_files(&cand, &nefcon, &manifest).is_ok() {
            return Some(ResolvedPayload {
                root: cand,
                vdd_dir: vdd,
                nefcon,
                manifest,
            });
        }
    }
    None
}

fn find_manifest(root: &Path) -> Option<PathBuf> {
    let candidates = [
        root.join("payload.manifest.json"),
        root.parent()
            .map(|p| p.join("payload.manifest.json"))
            .unwrap_or_default(),
    ];
    candidates.into_iter().find(|p| p.exists())
}

pub fn validate(payload: &ResolvedPayload) -> Result<(), String> {
    validate_files(&payload.root, &payload.nefcon, &payload.manifest)
}

fn validate_files(root: &Path, nefcon: &Path, manifest: &Path) -> Result<(), String> {
    let doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(manifest).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let thumb = doc
        .get("publisherThumbprint")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if !thumb.eq_ignore_ascii_case(PUBLISHER_THUMBPRINT) {
        return Err("publisherThumbprint 与锁定指纹不符".into());
    }
    let files = doc.get("files").ok_or("manifest 缺少 files")?;
    for (key, name) in REQUIRED_FILES {
        let path = if *name == "nefconc.exe" {
            nefcon.to_path_buf()
        } else {
            root.join("vdd").join(name)
        };
        if !path.exists() {
            return Err(format!("缺少 {key}"));
        }
        let expected = files.get(*key).and_then(|v| v.as_str()).unwrap_or("");
        let actual = sha256_file(&path);
        if !actual.eq_ignore_ascii_case(expected) {
            return Err(format!("哈希不符：{key}"));
        }
    }
    Ok(())
}

fn sha256_file(path: &Path) -> String {
    let bytes = std::fs::read(path).unwrap_or_default();
    hex_upper(&Sha256::digest(bytes))
}

fn hex_upper(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect()
}

fn dedup_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for path in paths {
        if path.as_os_str().is_empty() {
            continue;
        }
        if !out.iter().any(|existing: &PathBuf| existing == &path) {
            out.push(path);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    fn hash_of(bytes: &[u8]) -> String {
        hex_upper(&Sha256::digest(bytes))
    }

    fn write_valid_payload(root: &Path) {
        let files = [
            ("vdd/mttvdd.cat", b"cat".as_slice()),
            ("vdd/MttVDD.dll", b"dll".as_slice()),
            ("vdd/MttVDD.inf", b"inf".as_slice()),
            ("nefcon/x64/nefconc.exe", b"nef".as_slice()),
        ];
        for (rel, bytes) in files {
            let path = root.join(std::path::Path::new(rel));
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, bytes).unwrap();
        }
        let manifest = serde_json::json!({
            "publisherThumbprint": PUBLISHER_THUMBPRINT,
            "files": {
                "vdd/mttvdd.cat": hash_of(b"cat"),
                "vdd/MttVDD.dll": hash_of(b"dll"),
                "vdd/MttVDD.inf": hash_of(b"inf"),
                "nefcon/x64/nefconc.exe": hash_of(b"nef"),
            }
        });
        std::fs::write(
            root.join("payload.manifest.json"),
            serde_json::to_string_pretty(&manifest).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn payload_present_in_accepts_hashed_files() {
        let root = std::env::temp_dir().join(format!("veil-payload-ok-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        write_valid_payload(&root);
        assert!(payload_present_in(&[root.clone()]));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn payload_present_in_rejects_hash_mismatch() {
        let root = std::env::temp_dir().join(format!("veil-payload-bad-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        write_valid_payload(&root);
        std::fs::write(root.join("vdd").join("MttVDD.inf"), b"changed").unwrap();
        assert!(!payload_present_in(&[root.clone()]));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn payload_present_in_finds_exe_adjacent_when_program_files_empty() {
        let root = std::env::temp_dir().join(format!("veil-payload-exe-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        write_valid_payload(&root);
        let missing = root.join("missing-program-files");
        assert!(payload_present_in(&[missing, root.clone()]));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn roots_from_exe_walks_up_to_installer_payload() {
        let root = std::env::temp_dir().join(format!("veil-payload-walk-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let installer = root.join("installer");
        write_valid_payload(&installer.join("payload"));
        std::fs::copy(
            installer.join("payload").join("payload.manifest.json"),
            installer.join("payload.manifest.json"),
        )
        .unwrap();
        let exe_dir = root.join("src").join("target").join("debug");
        std::fs::create_dir_all(&exe_dir).unwrap();
        let roots = roots_from_exe(&exe_dir);
        assert!(payload_present_in(&roots), "roots={roots:?}");
        let _ = std::fs::remove_dir_all(&root);
    }
}
