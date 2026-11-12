//! Updates for the builds no store keeps current: the Windows installer and
//! the macOS disk image. (Flatpak updates through its repository, the
//! AppImage through the update information embedded in it, Android through
//! its store or Obtainium.)
//!
//! The latest release comes from the GitHub mirror's API, the same release
//! the Forgejo one is published with. An update is offered only once the
//! release has this platform's file, `SHA256SUMS` and its signature, and
//! the download is kept only if `SHA256SUMS` carries a good signature by
//! the release key (compiled in, `crate::openpgp`) and the file's checksum
//! matches its line there. A server that hands out other files, or other
//! checksums, cannot get past that without the release key.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64};

use serde::Deserialize;

use crate::Error;

/// Where releases are published for the updater to find.
pub const REPO: &str = "alvaroemtnez/HFPredict";
/// The version this build is.
pub const CURRENT: &str = env!("CARGO_PKG_VERSION");

/// A newer release, with this platform's file in it.
#[derive(Clone, Debug, PartialEq)]
pub struct Update {
    /// "0.3.0".
    pub version: String,
    /// The release's page: its notes, and every file by hand.
    pub page: String,
    /// The file's name, `HFPredict-<version>-<platform part>`.
    pub file: String,
    pub url: String,
    pub size: u64,
    /// The release's `SHA256SUMS`, and its signature `SHA256SUMS.asc`.
    pub sums_url: String,
    pub sig_url: String,
}

/// The release file for the platform this build runs on, after
/// `HFPredict-<version>-`; `None` where the app does not update itself.
pub fn platform_file() -> Option<&'static str> {
    if cfg!(all(windows, target_arch = "x86_64")) {
        Some("windows-x86_64-setup.exe")
    } else if cfg!(all(windows, target_arch = "aarch64")) {
        Some("windows-arm64-setup.exe")
    } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        Some("macos-arm64.dmg")
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        Some("macos-x86_64.dmg")
    } else {
        None
    }
}

/// "v0.3.0", "0.3.0" or "0.4.0-beta.2" as numbers, and whether it is a
/// pre-release (anything after a '-'); `None` if it is not a version.
fn version_parts(v: &str) -> Option<(Vec<u64>, bool)> {
    let v = v.trim().trim_start_matches('v');
    let (base, pre) = match v.split_once('-') {
        Some((base, pre)) => (base, Some(pre)),
        None => (v, None),
    };
    if base.is_empty() || pre.is_some_and(str::is_empty) {
        return None;
    }
    let numbers = base
        .split('.')
        .map(|p| p.parse().ok())
        .collect::<Option<Vec<u64>>>()?;
    Some((numbers, pre.is_some()))
}

/// Whether `candidate` is a later version than `current`. A pre-release is
/// never offered; a pre-release build (a beta, docs/releasing.md "Betas") is
/// offered the release it leads to, and anything after.
pub fn is_newer(candidate: &str, current: &str) -> bool {
    match (version_parts(candidate), version_parts(current)) {
        (Some((mut a, false)), Some((mut b, current_pre))) => {
            let n = a.len().max(b.len());
            a.resize(n, 0);
            b.resize(n, 0);
            a > b || (a == b && current_pre)
        }
        _ => false,
    }
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    size: u64,
}

/// The update a "latest release" answer offers over `current`, for the
/// release file ending in `platform`.
pub fn parse_latest(json: &str, current: &str, platform: &str) -> Option<Update> {
    let r: Release = serde_json::from_str(json).ok()?;
    if r.draft || r.prerelease || !is_newer(&r.tag_name, current) {
        return None;
    }
    let version = r.tag_name.trim_start_matches('v').to_string();
    let file = format!("HFPredict-{version}-{platform}");
    let asset = r.assets.iter().find(|a| a.name == file)?;
    // Still uploading, or a release without checksums: not yet.
    let sums = r.assets.iter().find(|a| a.name == "SHA256SUMS")?;
    // Unsigned releases are never offered.
    let sig = r.assets.iter().find(|a| a.name == "SHA256SUMS.asc")?;
    Some(Update {
        version,
        page: r.html_url,
        file,
        url: asset.browser_download_url.clone(),
        size: asset.size,
        sums_url: sums.browser_download_url.clone(),
        sig_url: sig.browser_download_url.clone(),
    })
}

/// The checksum `SHA256SUMS` lists for `file`, in lower-case hex.
pub fn checksum_for(sums: &str, file: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let (hash, name) = line.trim().split_once(char::is_whitespace)?;
        // sha256sum marks binary mode with '*'.
        let name = name.trim_start().trim_start_matches('*');
        (name == file && hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()))
            .then(|| hash.to_ascii_lowercase())
    })
}

/// How far a download is, for a progress bar: bytes so far, and set to
/// stop it.
#[derive(Default)]
pub struct Progress {
    pub received: AtomicU64,
    pub cancel: AtomicBool,
}

#[cfg(feature = "update")]
pub use fetch::{check, download};

#[cfg(not(feature = "update"))]
pub fn check(_platform: &str) -> Result<Option<Update>, Error> {
    Err(Error::Offline)
}

#[cfg(not(feature = "update"))]
pub fn download(_u: &Update, _dir: &Path, _p: &Progress) -> Result<PathBuf, Error> {
    Err(Error::Offline)
}

#[cfg(feature = "update")]
mod fetch {
    use std::fs;
    use std::io::{Read, Write};
    use std::sync::atomic::Ordering;
    use std::time::Duration;

    use sha2::{Digest, Sha256};

    use super::*;

    fn agent() -> ureq::Agent {
        ureq::Agent::config_builder()
            // A large file on a slow line: no limit on the whole, only on
            // each step.
            .timeout_connect(Some(Duration::from_secs(15)))
            .timeout_recv_response(Some(Duration::from_secs(30)))
            .timeout_per_read(Some(Duration::from_secs(30)))
            .user_agent(concat!(
                "HFPredict/",
                env!("CARGO_PKG_VERSION"),
                " (+https://code.alvaroedo.io/alvaroemtnez/HFPredict)"
            ))
            .build()
            .into()
    }

    fn net(e: impl std::fmt::Display) -> Error {
        Error::Network(e.to_string())
    }

    /// The latest release, if it is newer than this build and has the
    /// platform's file.
    pub fn check(platform: &str) -> Result<Option<Update>, Error> {
        let body = agent()
            .get(format!(
                "https://api.github.com/repos/{REPO}/releases/latest"
            ))
            .header("Accept", "application/vnd.github+json")
            .call()
            .map_err(net)?
            .body_mut()
            .read_to_string()
            .map_err(net)?;
        Ok(parse_latest(&body, CURRENT, platform))
    }

    /// Downloads the update into `dir` and checks it against the release's
    /// `SHA256SUMS`, once that is shown to be signed by the release key;
    /// the file's path once it matches.
    pub fn download(u: &Update, dir: &Path, progress: &Progress) -> Result<PathBuf, Error> {
        let sums = agent()
            .get(&u.sums_url)
            .call()
            .map_err(net)?
            .body_mut()
            .read_to_string()
            .map_err(net)?;
        let sig = agent()
            .get(&u.sig_url)
            .call()
            .map_err(net)?
            .body_mut()
            .read_to_string()
            .map_err(net)?;
        if !crate::openpgp::verify(crate::openpgp::RELEASE_KEY, sums.as_bytes(), &sig) {
            return Err(Error::Network(
                "SHA256SUMS is not signed by the HFPredict release key".into(),
            ));
        }
        let expected = checksum_for(&sums, &u.file)
            .ok_or_else(|| Error::Network(format!("SHA256SUMS does not list {}", u.file)))?;

        fs::create_dir_all(dir)?;
        let path = dir.join(&u.file);
        let partial = dir.join(format!("{}.part", u.file));
        let mut response = agent().get(&u.url).call().map_err(net)?;
        let mut reader = response
            .body_mut()
            .with_config()
            // Room over the advertised size, never unbounded.
            .limit(u.size.max(1 << 20) + (1 << 20))
            .reader();
        let mut out = fs::File::create(&partial)?;
        let mut hash = Sha256::new();
        let mut buf = vec![0u8; 64 * 1024];
        progress.received.store(0, Ordering::Relaxed);
        loop {
            if progress.cancel.load(Ordering::Relaxed) {
                drop(out);
                let _ = fs::remove_file(&partial);
                return Err(Error::Network("cancelled".into()));
            }
            let n = reader.read(&mut buf).map_err(net)?;
            if n == 0 {
                break;
            }
            hash.update(&buf[..n]);
            out.write_all(&buf[..n])?;
            progress.received.fetch_add(n as u64, Ordering::Relaxed);
        }
        out.sync_all()?;
        drop(out);
        let got: String = hash.finalize().iter().map(|b| format!("{b:02x}")).collect();
        if got != expected {
            let _ = fs::remove_file(&partial);
            return Err(Error::Network(format!(
                "{} does not match its checksum",
                u.file
            )));
        }
        fs::rename(&partial, &path)?;
        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RELEASE: &str = r#"{
        "tag_name": "v0.3.0", "html_url": "https://github.com/x/y/releases/tag/v0.3.0",
        "draft": false, "prerelease": false,
        "assets": [
            {"name": "HFPredict-0.3.0-windows-x86_64-setup.exe", "browser_download_url": "https://e/setup.exe", "size": 42},
            {"name": "SHA256SUMS", "browser_download_url": "https://e/SHA256SUMS", "size": 900},
            {"name": "SHA256SUMS.asc", "browser_download_url": "https://e/SHA256SUMS.asc", "size": 228}
        ]
    }"#;

    #[test]
    fn versions_compare_as_numbers() {
        assert!(is_newer("v0.10.0", "0.9.9"));
        assert!(is_newer("0.3.0", "0.2.1"));
        assert!(is_newer("1.0", "0.99.0"));
        assert!(!is_newer("v0.2.1", "0.2.1"));
        assert!(!is_newer("0.2.0", "0.2.1"));
        assert!(!is_newer("0.3.0-rc1", "0.2.1"));
        assert!(!is_newer("nonsense", "0.2.1"));
    }

    #[test]
    fn a_beta_build_is_offered_its_release_and_later_but_never_a_beta() {
        // The case that matters: without it a beta tester is stuck on the beta.
        assert!(is_newer("v0.4.0", "0.4.0-beta.2"));
        assert!(is_newer("0.4.1", "0.4.0-beta.1"));
        // The release before it is older, not an update.
        assert!(!is_newer("v0.3.2", "0.4.0-beta.1"));
        // Pre-releases are never offered, to a beta or to a release.
        assert!(!is_newer("v0.4.0-beta.2", "0.4.0-beta.1"));
        assert!(!is_newer("v0.5.0-beta.1", "0.4.0"));
        // Malformed current or candidate: nothing.
        assert!(!is_newer("0.4.0", "0.4.0-"));
        assert!(!is_newer("0.4.0", "-beta.1"));
        assert!(!is_newer("0.4.0-", "0.3.0"));
    }

    #[test]
    fn offers_the_platform_file_of_a_newer_release() {
        let u = parse_latest(RELEASE, "0.2.1", "windows-x86_64-setup.exe").unwrap();
        assert_eq!(u.version, "0.3.0");
        assert_eq!(u.file, "HFPredict-0.3.0-windows-x86_64-setup.exe");
        assert_eq!(u.url, "https://e/setup.exe");
        assert_eq!(u.size, 42);
        assert_eq!(u.sums_url, "https://e/SHA256SUMS");
        assert_eq!(u.sig_url, "https://e/SHA256SUMS.asc");
    }

    #[test]
    fn a_beta_build_is_offered_the_release() {
        let u = parse_latest(RELEASE, "0.3.0-beta.2", "windows-x86_64-setup.exe").unwrap();
        assert_eq!(u.file, "HFPredict-0.3.0-windows-x86_64-setup.exe");
    }

    #[test]
    fn nothing_when_current_missing_or_unchecked() {
        assert_eq!(
            parse_latest(RELEASE, "0.3.0", "windows-x86_64-setup.exe"),
            None
        );
        // The ARM64 installer is not in this release (yet).
        assert_eq!(
            parse_latest(RELEASE, "0.2.1", "windows-arm64-setup.exe"),
            None
        );
        let no_sig = RELEASE.replace("\"SHA256SUMS.asc\"", "\"README\"");
        assert_eq!(
            parse_latest(&no_sig, "0.2.1", "windows-x86_64-setup.exe"),
            None
        );
        let no_sums = RELEASE.replace("\"SHA256SUMS\"", "\"README\"");
        assert_eq!(
            parse_latest(&no_sums, "0.2.1", "windows-x86_64-setup.exe"),
            None
        );
        let pre = RELEASE.replace("\"prerelease\": false", "\"prerelease\": true");
        assert_eq!(
            parse_latest(&pre, "0.2.1", "windows-x86_64-setup.exe"),
            None
        );
        assert_eq!(
            parse_latest("not json", "0.2.1", "windows-x86_64-setup.exe"),
            None
        );
    }

    #[test]
    fn finds_the_file_checksum() {
        let h = "a".repeat(64);
        let sums = format!(
            "{}  HFPredict-0.3.0-macos-arm64.dmg\n{h} *HFPredict-0.3.0-windows-x86_64-setup.exe\n",
            "b".repeat(64)
        );
        assert_eq!(
            checksum_for(&sums, "HFPredict-0.3.0-windows-x86_64-setup.exe"),
            Some(h)
        );
        assert_eq!(checksum_for(&sums, "HFPredict-0.3.0-x86_64.AppImage"), None);
        assert_eq!(checksum_for("short  HFPredict", "HFPredict"), None);
    }
}
