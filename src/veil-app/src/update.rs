use std::cmp::Ordering;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Deserialize;

const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
const RELEASE_API: &str = "/repos/wynxing/Veil/releases?per_page=20";
const RELEASE_HOST: &str = "api.github.com";
const MAX_BODY: usize = 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct UpdateOffer {
    pub version: String,
    pub url: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InformationalVersion {
    major: u32,
    minor: u32,
    patch: u32,
    preview: Option<u32>,
}

pub enum CacheRead {
    Fresh(Option<UpdateOffer>),
    Due,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct CacheFile {
    checked_at_unix: u64,
    offer: Option<UpdateOffer>,
}

#[derive(Deserialize)]
struct GithubRelease {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    html_url: String,
}

pub fn local_version() -> &'static str {
    env!("VEIL_INFORMATIONAL_VERSION")
}

pub fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub fn cache_is_fresh(checked_at_unix: u64, now: u64) -> bool {
    now.saturating_sub(checked_at_unix) < CHECK_INTERVAL.as_secs()
}

pub fn parse_version(text: &str) -> Option<InformationalVersion> {
    let text = text.trim().strip_prefix('v').unwrap_or(text.trim());
    let (numbers, preview) = if let Some((head, tail)) = text.split_once('-') {
        let number = tail.strip_prefix("preview.")?.parse().ok()?;
        (head, Some(number))
    } else {
        (text, None)
    };
    let mut parts = numbers.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some(InformationalVersion {
        major,
        minor,
        patch,
        preview,
    })
}

impl InformationalVersion {
    fn display(self) -> String {
        match self.preview {
            Some(preview) => format!(
                "{}.{}.{}-preview.{preview}",
                self.major, self.minor, self.patch
            ),
            None => format!("{}.{}.{}", self.major, self.minor, self.patch),
        }
    }

    fn rank(self) -> (u32, u32, u32, u8, u32) {
        match self.preview {
            Some(preview) => (self.major, self.minor, self.patch, 0, preview),
            None => (self.major, self.minor, self.patch, 1, 0),
        }
    }
}

impl PartialOrd for InformationalVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for InformationalVersion {
    fn cmp(&self, other: &Self) -> Ordering {
        self.rank().cmp(&other.rank())
    }
}

pub fn select_update(body: &str, local: &str) -> Result<Option<UpdateOffer>, ()> {
    let local = parse_version(local).ok_or(())?;
    let releases: Vec<GithubRelease> = serde_json::from_str(body).map_err(|_| ())?;
    let mut best: Option<(InformationalVersion, UpdateOffer)> = None;
    for release in releases {
        if release.draft {
            continue;
        }
        let Some(version) = parse_version(&release.tag_name) else {
            continue;
        };
        if version <= local {
            continue;
        }
        let expected = release_page_url(&release.tag_name);
        if release.html_url != expected {
            continue;
        }
        let offer = UpdateOffer {
            version: version.display(),
            url: release.html_url,
        };
        if best.as_ref().is_none_or(|(current, _)| version > *current) {
            best = Some((version, offer));
        }
    }
    Ok(best.map(|(_, offer)| offer))
}

fn release_page_url(tag: &str) -> String {
    format!("https://github.com/wynxing/Veil/releases/tag/{tag}")
}

pub fn fresh_cached_offer(now: u64) -> CacheRead {
    match read_cache() {
        Some(cache) if cache_is_fresh(cache.checked_at_unix, now) => {
            CacheRead::Fresh(retain_newer(cache.offer, local_version()))
        }
        _ => CacheRead::Due,
    }
}

fn retain_newer(offer: Option<UpdateOffer>, local: &str) -> Option<UpdateOffer> {
    let offer = offer?;
    let remote = parse_version(&offer.version)?;
    let local = parse_version(local)?;
    (remote > local).then_some(offer)
}

pub fn check_remote() -> Option<UpdateOffer> {
    let now = unix_now();
    let previous = retain_newer(read_cache().and_then(|cache| cache.offer), local_version());
    let offer = match http_get_releases(local_version()) {
        Ok(body) => match select_update(&body, local_version()) {
            Ok(found) => found,
            Err(()) => {
                crate::app_log("更新检查的发布列表无法解析，面板不提示。");
                previous
            }
        },
        Err(()) => {
            crate::app_log("更新检查失败，面板不提示。");
            previous
        }
    };
    if write_cache(now, &offer).is_err() {
        crate::app_log("更新检查结果没有写入本地记录。");
    }
    offer
}

pub fn open_release_page(url: &str) -> bool {
    let Some(tag) = url.strip_prefix("https://github.com/wynxing/Veil/releases/tag/") else {
        return false;
    };
    if parse_version(tag).is_none() || url != release_page_url(tag) {
        return false;
    }
    open_https_url(url)
}

fn cache_path() -> Option<PathBuf> {
    let root = std::env::var_os("LOCALAPPDATA")?;
    Some(PathBuf::from(root).join("Veil").join("update-check.json"))
}

fn read_cache() -> Option<CacheFile> {
    let path = cache_path()?;
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

fn write_cache(now: u64, offer: &Option<UpdateOffer>) -> Result<(), ()> {
    let path = cache_path().ok_or(())?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|_| ())?;
    }
    let body = serde_json::to_vec(&CacheFile {
        checked_at_unix: now,
        offer: offer.clone(),
    })
    .map_err(|_| ())?;
    std::fs::write(path, body).map_err(|_| ())
}

fn http_get_releases(version: &str) -> Result<String, ()> {
    use std::ffi::c_void;
    use windows_sys::Win32::Networking::WinHttp::{
        WinHttpConnect, WinHttpOpen, WinHttpOpenRequest, WinHttpQueryDataAvailable,
        WinHttpQueryHeaders, WinHttpReadData, WinHttpReceiveResponse, WinHttpSendRequest,
        WinHttpSetTimeouts, WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_FLAG_SECURE,
        WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_STATUS_CODE,
    };

    let agent = wide(&format!("Veil/{version}"));
    let host = wide(RELEASE_HOST);
    let verb = wide("GET");
    let path = wide(RELEASE_API);
    let headers = wide("Accept: application/vnd.github+json\r\n");

    let session = InternetHandle(unsafe {
        WinHttpOpen(
            agent.as_ptr(),
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
            std::ptr::null(),
            std::ptr::null(),
            0,
        )
    });
    if session.0.is_null() {
        return Err(());
    }
    if unsafe { WinHttpSetTimeouts(session.0, 5_000, 5_000, 5_000, 5_000) } == 0 {
        return Err(());
    }
    let connect = InternetHandle(unsafe { WinHttpConnect(session.0, host.as_ptr(), 443, 0) });
    if connect.0.is_null() {
        return Err(());
    }
    let request = InternetHandle(unsafe {
        WinHttpOpenRequest(
            connect.0,
            verb.as_ptr(),
            path.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            WINHTTP_FLAG_SECURE,
        )
    });
    if request.0.is_null() {
        return Err(());
    }
    let sent = unsafe {
        WinHttpSendRequest(
            request.0,
            headers.as_ptr(),
            u32::MAX,
            std::ptr::null(),
            0,
            0,
            0,
        )
    };
    if sent == 0 || unsafe { WinHttpReceiveResponse(request.0, std::ptr::null_mut()) } == 0 {
        return Err(());
    }
    let mut status = 0u32;
    let mut status_len = std::mem::size_of::<u32>() as u32;
    let status_ok = unsafe {
        WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            std::ptr::null(),
            &mut status as *mut u32 as *mut c_void,
            &mut status_len,
            std::ptr::null_mut(),
        )
    };
    if status_ok == 0 || status != 200 {
        return Err(());
    }

    let mut body = Vec::new();
    loop {
        let mut available = 0u32;
        if unsafe { WinHttpQueryDataAvailable(request.0, &mut available) } == 0 {
            return Err(());
        }
        if available == 0 {
            break;
        }
        if body.len().saturating_add(available as usize) > MAX_BODY {
            return Err(());
        }
        let mut chunk = vec![0u8; available as usize];
        let mut read = 0u32;
        if unsafe {
            WinHttpReadData(
                request.0,
                chunk.as_mut_ptr() as *mut c_void,
                available,
                &mut read,
            )
        } == 0
        {
            return Err(());
        }
        if read == 0 {
            break;
        }
        chunk.truncate(read as usize);
        body.extend_from_slice(&chunk);
    }
    String::from_utf8(body).map_err(|_| ())
}

struct InternetHandle(*mut std::ffi::c_void);

impl Drop for InternetHandle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                windows_sys::Win32::Networking::WinHttp::WinHttpCloseHandle(self.0);
            }
        }
    }
}

fn open_https_url(url: &str) -> bool {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let verb = wide("open");
    let target = wide(url);
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            target.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    result as usize > 32
}

fn wide(text: &str) -> Vec<u16> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    OsStr::new(text)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn offer(version: &str) -> UpdateOffer {
        let tag = format!("v{version}");
        UpdateOffer {
            version: version.to_string(),
            url: release_page_url(&tag),
        }
    }

    fn release(tag: &str, draft: bool, url: Option<&str>) -> String {
        let url = url
            .map(str::to_string)
            .unwrap_or_else(|| release_page_url(tag));
        format!(r#"{{"tag_name":"{tag}","draft":{draft},"html_url":"{url}"}}"#)
    }

    #[test]
    fn embedded_version_parses() {
        assert!(parse_version(local_version()).is_some());
    }

    #[test]
    fn preview_orders_below_the_same_release() {
        let preview = parse_version("0.1.12-preview.2").unwrap();
        let later_preview = parse_version("v0.1.12-preview.9").unwrap();
        let release = parse_version("0.1.12").unwrap();
        let next = parse_version("0.1.13-preview.1").unwrap();
        assert!(preview < later_preview);
        assert!(later_preview < release);
        assert!(release < next);
        assert!(parse_version("1.2").is_none());
        assert!(parse_version("0.1.12-beta.1").is_none());
    }

    #[test]
    fn select_update_picks_the_newest_real_release_page() {
        let body = format!(
            "[{},{},{},{},{}]",
            release("v0.1.12-preview.1", false, None),
            release("v0.1.12-preview.2", false, None),
            release("v0.1.13-preview.1", true, None),
            release("v0.1.11", false, None),
            release(
                "v0.2.0",
                false,
                Some("https://example.invalid/VeilSetup.exe")
            ),
        );
        let selected = select_update(&body, "0.1.12-preview.1").unwrap();
        assert_eq!(selected, Some(offer("0.1.12-preview.2")));
    }

    #[test]
    fn stable_release_beats_a_preview_of_the_same_number() {
        let body = format!(
            "[{},{}]",
            release("v0.1.12-preview.4", false, None),
            release("v0.1.12", false, None),
        );
        let selected = select_update(&body, "0.1.12-preview.1").unwrap();
        assert_eq!(selected, Some(offer("0.1.12")));
    }

    #[test]
    fn current_or_older_versions_are_not_offered() {
        let body = format!(
            "[{},{}]",
            release("v0.1.12-preview.1", false, None),
            release("v0.1.12-preview.9", false, None),
        );
        assert_eq!(select_update(&body, "0.1.12").unwrap(), None);
    }

    #[test]
    fn broken_release_json_is_an_error() {
        assert!(select_update("not json", "0.1.12-preview.1").is_err());
    }

    #[test]
    fn cached_offer_is_hidden_once_it_is_no_longer_newer() {
        let stale = Some(offer("0.1.12-preview.1"));
        assert_eq!(retain_newer(stale, "0.1.12-preview.1"), None);
        assert_eq!(
            retain_newer(Some(offer("0.1.13-preview.1")), "0.1.12-preview.1")
                .as_ref()
                .map(|item| item.version.as_str()),
            Some("0.1.13-preview.1")
        );
    }

    #[test]
    fn cache_stays_fresh_until_the_interval_elapses() {
        assert!(cache_is_fresh(1_000, 1_000));
        assert!(cache_is_fresh(1_000, 1_000 + CHECK_INTERVAL.as_secs() - 1));
        assert!(!cache_is_fresh(1_000, 1_000 + CHECK_INTERVAL.as_secs()));
        assert!(cache_is_fresh(2_000, 1_000));
    }

    #[test]
    fn open_release_page_rejects_other_urls() {
        assert!(!open_release_page("https://example.invalid/setup.exe"));
        assert!(!open_release_page(
            "https://github.com/wynxing/Veil/releases/download/v0.1.12/VeilSetup.exe"
        ));
    }
}
