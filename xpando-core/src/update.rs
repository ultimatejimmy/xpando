//! The logic of the in-app updater, with no network or disk access of its own: version parsing
//! and ordering, picking the newest release that applies, choosing the installer for this
//! computer, looking for the signed manifest `latest.json`, and checking that a download address is really a
//! GitHub release of this project. The app (`xpando-app`) does the downloading and feeds the
//! answers through here. See `docs/xpando/RELEASING.md` for the version scheme.

use std::cmp::Ordering;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::minijson::{self, Json};

/// The releases of this project, newest first (GitHub's own order).
pub const RELEASES_API_URL: &str =
    "https://api.github.com/repos/ultimatejimmy/xpando/releases?per_page=30";
/// The release asset that Tauri's updater reads: the version, and for every platform the address
/// of the update package and its minisign signature.
pub const MANIFEST_FILE: &str = "latest.json";
/// The only User-Agent the updater sends: no name, version or machine details.
pub const USER_AGENT: &str = "xpando-updater";
/// How often the automatic check runs.
pub const CHECK_INTERVAL_SECS: u64 = 24 * 60 * 60;

const DOWNLOAD_PREFIX: &str = "https://github.com/ultimatejimmy/xpando/releases/download/";
/// Hosts GitHub may redirect a release download to (plus github.com itself).
const REDIRECT_HOSTS: [&str; 5] = [
    "github.com",
    "api.github.com",
    "objects.githubusercontent.com",
    "release-assets.githubusercontent.com",
    "github-releases.githubusercontent.com",
];

// ---------------------------------------------------------------------------------------
// Versions
// ---------------------------------------------------------------------------------------

/// A human version: `YYYY.M.D`, `YYYY.M.D.P` (P >= 1) or `YYYY.M.D-beta[N]` (N >= 2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Version {
    pub year: u32,
    pub month: u32,
    pub day: u32,
    /// 0 for the first release of a day.
    pub patch: u32,
    /// `Some(n)` for a beta; the first beta is 1 and is written `-beta`.
    pub beta: Option<u32>,
}

fn days_in_month(year: u32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ => {
            if (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400) {
                29
            } else {
                28
            }
        }
    }
}

/// A number without a sign, spaces or leading zeros.
fn plain_number(s: &str) -> Option<u32> {
    if s.is_empty() || s.len() > 9 || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if s.len() > 1 && s.starts_with('0') {
        return None;
    }
    s.parse().ok()
}

impl Version {
    /// Reads a version exactly as `scripts/version.mjs` accepts it (no `v`, no zero padding,
    /// a real date).
    pub fn parse(text: &str) -> Result<Version, String> {
        let bad = || {
            format!(
                "\"{text}\" is not a xpando version (expected something like 2026.11.23 or 2026.11.23-beta)"
            )
        };
        let (main, beta_text) = match text.split_once("-beta") {
            Some((m, b)) => (m, Some(b)),
            None => (text, None),
        };
        let parts: Vec<&str> = main.split('.').collect();
        if !(3..=4).contains(&parts.len()) {
            return Err(bad());
        }
        let year = plain_number(parts[0]).ok_or_else(bad)?;
        let month = plain_number(parts[1]).ok_or_else(bad)?;
        let day = plain_number(parts[2]).ok_or_else(bad)?;
        if !(1000..=9999).contains(&year)
            || parts[0].len() != 4
            || !(1..=12).contains(&month)
            || day < 1
            || day > days_in_month(year, month)
        {
            return Err(bad());
        }
        let patch = match parts.get(3) {
            Some(p) => {
                let n = plain_number(p).ok_or_else(bad)?;
                if n == 0 {
                    return Err(bad());
                }
                n
            }
            None => 0,
        };
        let beta = match beta_text {
            None => None,
            Some(_) if patch != 0 => return Err(bad()),
            Some("") => Some(1),
            Some(n) => {
                let n = plain_number(n).ok_or_else(bad)?;
                if n < 2 {
                    return Err(bad());
                }
                Some(n)
            }
        };
        Ok(Version {
            year,
            month,
            day,
            patch,
            beta,
        })
    }

    pub fn is_beta(&self) -> bool {
        self.beta.is_some()
    }

    /// `YYYY.<M*100+D>.<P>[-beta.<N>]`, the form Cargo, Tauri and npm need.
    pub fn semver(&self) -> String {
        let base = format!(
            "{}.{}.{}",
            self.year,
            self.month * 100 + self.day,
            self.patch
        );
        match self.beta {
            None => base,
            Some(n) => format!("{base}-beta.{n}"),
        }
    }

    /// The inverse of [`Version::semver`]. `None` for anything else, such as the `0.1.0`
    /// of a development build.
    pub fn from_semver(semver: &str) -> Option<Version> {
        let (core, beta) = match semver.split_once("-beta.") {
            Some((c, b)) => (c, Some(plain_number(b).filter(|n| *n >= 1)?)),
            None => (semver, None),
        };
        let parts: Vec<&str> = core.split('.').collect();
        if parts.len() != 3 || parts[0].len() != 4 {
            return None;
        }
        let year = plain_number(parts[0])?;
        let md = plain_number(parts[1])?;
        let patch = plain_number(parts[2])?;
        if beta.is_some() && patch != 0 {
            return None;
        }
        let v = Version {
            year,
            month: md / 100,
            day: md % 100,
            patch,
            beta,
        };
        // Round trip through the text form to validate the date and the canonical spelling.
        Version::parse(&v.to_string()).ok().filter(|p| p == &v)
    }

    /// Sort key: date first; within a day, betas (in order) come before the stable release,
    /// which comes before its patches.
    fn key(&self) -> (u32, u32, u32, u8, u32) {
        match self.beta {
            Some(n) => (self.year, self.month, self.day, 0, n),
            None => (self.year, self.month, self.day, 1, self.patch),
        }
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.year, self.month, self.day)?;
        match self.beta {
            None if self.patch > 0 => write!(f, ".{}", self.patch),
            None => Ok(()),
            Some(1) => write!(f, "-beta"),
            Some(n) => write!(f, "-beta{n}"),
        }
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        self.key().cmp(&other.key())
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

// ---------------------------------------------------------------------------------------
// Settings values
// ---------------------------------------------------------------------------------------

/// Which releases the updater offers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    /// Betas as well as stable releases.
    Beta,
    /// Stable releases only. Anything unrecognised in the settings file also means this.
    #[default]
    #[serde(other)]
    Stable,
}

/// Whether the daily check should run now. `last` is when the last successful check ended
/// (seconds since 1970). A date in the future (the clock was set back) counts as due.
pub fn is_check_due(auto_check: bool, last: Option<u64>, now: u64) -> bool {
    if !auto_check {
        return false;
    }
    match last {
        None => true,
        Some(l) => {
            l > now.saturating_add(CHECK_INTERVAL_SECS)
                || now.saturating_sub(l) >= CHECK_INTERVAL_SECS
        }
    }
}

/// A sentence for people on a beta who use the stable channel, or `None`.
pub fn downgrade_note(current: &Version, channel: Channel) -> Option<&'static str> {
    (current.is_beta() && channel == Channel::Stable).then_some(
        "You are using a beta. xpando will only offer a stable version that is newer than this one, and never switches you back to an older version.",
    )
}

// ---------------------------------------------------------------------------------------
// Releases
// ---------------------------------------------------------------------------------------

/// A file attached to a release.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub size: u64,
}

/// One release from the GitHub API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub tag: String,
    pub draft: bool,
    pub prerelease: bool,
    /// The release's page on github.com.
    pub page_url: String,
    pub assets: Vec<Asset>,
}

/// Reads the JSON list `GET /repos/.../releases` returns. Fields it does not need are ignored;
/// a release without a tag is skipped.
pub fn parse_releases(json: &str) -> Result<Vec<Release>, String> {
    let doc =
        minijson::parse(json).map_err(|e| format!("The release list could not be read ({e})."))?;
    let list = doc
        .as_array()
        .ok_or("The release list was not in the expected form.")?;
    let mut out = Vec::new();
    for item in list {
        let Some(tag) = item.get("tag_name").and_then(Json::as_str) else {
            continue;
        };
        let assets = item
            .get("assets")
            .and_then(Json::as_array)
            .unwrap_or_default()
            .iter()
            .filter_map(|a| {
                Some(Asset {
                    name: a.get("name")?.as_str()?.to_string(),
                    url: a.get("browser_download_url")?.as_str()?.to_string(),
                    size: a.get("size").and_then(Json::as_u64).unwrap_or(0),
                })
            })
            .collect();
        out.push(Release {
            tag: tag.to_string(),
            draft: item.get("draft").and_then(Json::as_bool).unwrap_or(false),
            prerelease: item
                .get("prerelease")
                .and_then(Json::as_bool)
                .unwrap_or(false),
            page_url: item
                .get("html_url")
                .and_then(Json::as_str)
                .unwrap_or_default()
                .to_string(),
            assets,
        });
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------
// Choosing the installer
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    Windows,
    MacOs,
    Linux,
}

impl Os {
    pub fn current() -> Os {
        if cfg!(windows) {
            Os::Windows
        } else if cfg!(target_os = "macos") {
            Os::MacOs
        } else {
            Os::Linux
        }
    }
}

/// Words that name the CPU in an installer's file name, best match first.
fn arch_words(os: Os, arch: &str) -> &'static [&'static str] {
    match (os, arch) {
        (Os::Windows, "aarch64") => &["arm64", "x64"], // x64 installers run under emulation
        (Os::Windows, _) => &["x64"],
        (Os::MacOs, "aarch64") => &["universal", "aarch64", "arm64"],
        (Os::MacOs, _) => &["universal", "x64", "x86_64"],
        (Os::Linux, "aarch64") => &["aarch64", "arm64"],
        (Os::Linux, _) => &["amd64", "x86_64", "x64"],
    }
}

/// The installer for this OS and CPU: the NSIS `-setup.exe` on Windows, the `.dmg` on macOS and
/// the `.AppImage` on Linux. `arch` is Rust's `std::env::consts::ARCH`.
pub fn choose_installer<'a>(assets: &'a [Asset], os: Os, arch: &str) -> Option<&'a Asset> {
    let suffix = match os {
        Os::Windows => "-setup.exe",
        Os::MacOs => ".dmg",
        Os::Linux => ".appimage",
    };
    let candidates: Vec<&Asset> = assets
        .iter()
        .filter(|a| a.name.to_ascii_lowercase().ends_with(suffix))
        .collect();
    for word in arch_words(os, arch) {
        if let Some(a) = candidates
            .iter()
            .find(|a| a.name.to_ascii_lowercase().contains(word))
        {
            return Some(a);
        }
    }
    None
}

/// An update that can be installed, with everything the install step needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Update {
    /// The human version, e.g. `2026.11.23-beta2`.
    pub version: String,
    pub tag: String,
    pub beta: bool,
    pub release_url: String,
    pub installer: Asset,
    /// The signed update manifest (`latest.json`) the plugin updater is pointed at.
    pub manifest: Asset,
}

/// The newest release that applies to this computer, or `None`.
///
/// A release applies when it is not a draft, its tag is a valid version, it is newer than
/// `current`, it is stable unless `channel` is `Beta` (a release marked pre-release on GitHub
/// or with a `-beta` tag counts as a beta), it is not `skipped`, and it carries both an
/// installer for this computer and the signed manifest `latest.json` at addresses `validate_download_url`
/// accepts. A beta user therefore also gets a newer stable release, and a stable user never
/// gets a beta or an older version.
pub fn pick_update(
    current: &Version,
    channel: Channel,
    skipped: Option<&Version>,
    releases: &[Release],
    os: Os,
    arch: &str,
) -> Option<Update> {
    let mut best: Option<(Version, Update)> = None;
    for r in releases {
        if r.draft {
            continue;
        }
        let Ok(v) = Version::parse(&r.tag) else {
            continue;
        };
        let beta = v.is_beta() || r.prerelease;
        if (beta && channel != Channel::Beta) || v <= *current || Some(&v) == skipped {
            continue;
        }
        if best.as_ref().is_some_and(|(b, _)| *b >= v) {
            continue;
        }
        let Some(installer) = choose_installer(&r.assets, os, arch) else {
            continue;
        };
        let Some(manifest) = r.assets.iter().find(|a| a.name == MANIFEST_FILE) else {
            continue;
        };
        if validate_download_url(&installer.url, &r.tag, &installer.name).is_err()
            || validate_download_url(&manifest.url, &r.tag, &manifest.name).is_err()
        {
            continue;
        }
        best = Some((
            v,
            Update {
                version: v.to_string(),
                tag: r.tag.clone(),
                beta,
                release_url: r.page_url.clone(),
                installer: installer.clone(),
                manifest: manifest.clone(),
            },
        ));
    }
    best.map(|(_, u)| u)
}

// ---------------------------------------------------------------------------------------
// Addresses and HTTP answers
// ---------------------------------------------------------------------------------------

fn safe_segment(s: &str) -> bool {
    !s.is_empty()
        && s != "."
        && s != ".."
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-' | b'+'))
}

/// Accepts only `https://github.com/ultimatejimmy/xpando/releases/download/<tag>/<file>` for
/// the given tag and file name: no other host, port, user name, query, fragment, encoded or
/// odd characters, or extra path parts.
pub fn validate_download_url(url: &str, tag: &str, file: &str) -> Result<(), String> {
    let bad = || format!("{url:?} is not a download address of this project, so it was not used.");
    let rest = url.strip_prefix(DOWNLOAD_PREFIX).ok_or_else(bad)?;
    let (t, f) = rest.split_once('/').ok_or_else(bad)?;
    if !safe_segment(t) || !safe_segment(f) || t != tag || f != file {
        return Err(bad());
    }
    Ok(())
}

/// Accepts the address GitHub redirects a download to: https, one of GitHub's own hosts, no
/// user name or port, no control characters or spaces.
pub fn validate_redirect_url(url: &str) -> Result<(), String> {
    let bad = || "GitHub sent the download somewhere unexpected, so it was stopped.".to_string();
    let rest = url.strip_prefix("https://").ok_or_else(bad)?;
    if rest.bytes().any(|b| b <= b' ' || b == 0x7f || b == b'\\') {
        return Err(bad());
    }
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let host = &rest[..end];
    if host.contains(['@', ':']) || !REDIRECT_HOSTS.contains(&host.to_ascii_lowercase().as_str()) {
        return Err(bad());
    }
    Ok(())
}

/// Reads the status code and `Location` header of the last response in a header dump (as
/// written by `curl -D`, which may hold several responses when it follows a proxy or `100`).
pub fn parse_response_head(head: &str) -> Option<(u16, Option<String>)> {
    let mut status = None;
    let mut location = None;
    for line in head.lines() {
        let line = line.trim_end_matches('\r');
        if line.starts_with("HTTP/") {
            status = line.split_whitespace().nth(1).and_then(|c| c.parse().ok());
            location = None; // a new response starts
        } else if let Some((name, value)) = line.split_once(':') {
            if name.eq_ignore_ascii_case("location") {
                location = Some(value.trim().to_string());
            }
        }
    }
    status.map(|s| (s, location))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> Version {
        Version::parse(s).unwrap()
    }

    // ---- versions ----

    #[test]
    fn parses_and_prints_every_form() {
        for s in [
            "2026.11.23",
            "2026.11.23.1",
            "2026.11.23.12",
            "2026.11.23-beta",
            "2026.11.23-beta2",
            "2026.1.5",
            "2024.2.29",
        ] {
            assert_eq!(v(s).to_string(), s);
        }
        assert_eq!(v("2026.11.23-beta").beta, Some(1));
        assert_eq!(v("2026.11.23-beta3").beta, Some(3));
        assert_eq!(v("2026.11.23.4").patch, 4);
    }

    #[test]
    fn rejects_what_the_version_script_rejects() {
        for bad in [
            "",
            "v2026.11.23",
            "2026.11",
            "2026.11.23.0",
            "2026.11.23-beta1",
            "2026.11.23-beta0",
            "2026.11.23.1-beta",
            "2026.11.23.1-beta2",
            "2026.11.23-rc1",
            "2026.11.23-betax",
            "2026.01.05",
            "2026.11.05",
            "2026.13.1",
            "2026.0.1",
            "2026.2.30",
            "2023.2.29",
            "26.11.23",
            "2026.11.23.1.2",
            " 2026.11.23",
            "2026.11.23\n",
            "2026.-1.23",
            "2026.+1.23",
        ] {
            assert!(Version::parse(bad).is_err(), "{bad:?} should be rejected");
        }
    }

    #[test]
    fn ordering_matches_the_release_docs() {
        let ordered = [
            "2026.11.23-beta",
            "2026.11.23-beta2",
            "2026.11.23-beta10",
            "2026.11.23",
            "2026.11.23.1",
            "2026.11.23.2",
            "2026.11.23.10",
            "2026.11.24-beta",
            "2026.11.24",
            "2026.12.1",
            "2027.1.1",
        ];
        for pair in ordered.windows(2) {
            assert!(v(pair[0]) < v(pair[1]), "{} < {}", pair[0], pair[1]);
            assert!(v(pair[1]) > v(pair[0]));
        }
        assert_eq!(v("2026.11.23").cmp(&v("2026.11.23")), Ordering::Equal);
        // A later date wins even against a stable patch of an earlier day.
        assert!(v("2026.11.24-beta") > v("2026.11.23.9"));
        // Stable beats the beta it followed, even when its date is the same.
        assert!(v("2026.11.23") > v("2026.11.23-beta3"));
    }

    #[test]
    fn semver_mapping_both_ways_matches_the_script() {
        let table = [
            ("2026.11.23", "2026.1123.0"),
            ("2026.11.23.1", "2026.1123.1"),
            ("2026.11.23.2", "2026.1123.2"),
            ("2026.11.23-beta", "2026.1123.0-beta.1"),
            ("2026.11.23-beta2", "2026.1123.0-beta.2"),
            ("2026.1.5", "2026.105.0"),
        ];
        for (human, semver) in table {
            assert_eq!(v(human).semver(), semver);
            assert_eq!(Version::from_semver(semver), Some(v(human)), "{semver}");
        }
    }

    #[test]
    fn semver_that_is_not_ours_maps_to_none() {
        for s in [
            "0.1.0",
            "1.2.3",
            "2026.1123",
            "2026.1123.0-beta.0",
            "2026.1123.1-beta.1",
            "2026.1123.0-rc.1",
            "2026.1332.0",
            "2026.230.0",
            "2026.1123.0-beta.2.3",
            "",
        ] {
            assert_eq!(Version::from_semver(s), None, "{s:?}");
        }
    }

    #[test]
    fn derived_semver_sorts_like_the_human_version() {
        let human = ["2026.11.23-beta", "2026.11.23", "2026.11.23.1", "2026.12.2"];
        for pair in human.windows(2) {
            assert!(v(pair[0]) < v(pair[1]));
        }
    }

    // ---- settings values ----

    #[test]
    fn channel_reads_and_writes_lowercase_and_defaults_to_stable() {
        assert_eq!(Channel::default(), Channel::Stable);
        assert_eq!(
            serde_norway::to_string(&Channel::Beta).unwrap().trim(),
            "beta"
        );
        assert_eq!(
            serde_norway::from_str::<Channel>("beta").unwrap(),
            Channel::Beta
        );
        assert_eq!(
            serde_norway::from_str::<Channel>("stable").unwrap(),
            Channel::Stable
        );
        assert_eq!(
            serde_norway::from_str::<Channel>("nightly").unwrap(),
            Channel::Stable
        );
    }

    #[test]
    fn daily_check_timing() {
        let day = CHECK_INTERVAL_SECS;
        assert!(is_check_due(true, None, 1000));
        assert!(!is_check_due(true, Some(1000), 1000 + day - 1));
        assert!(is_check_due(true, Some(1000), 1000 + day));
        assert!(!is_check_due(false, None, 1000));
        assert!(!is_check_due(false, Some(0), 10 * day));
        // The clock went back a lot: do not wait for it to catch up.
        assert!(is_check_due(true, Some(10 * day), 1000));
        // A little skew is not "due".
        assert!(!is_check_due(true, Some(1010), 1000));
    }

    #[test]
    fn beta_user_on_stable_gets_a_note() {
        assert!(downgrade_note(&v("2026.11.23-beta"), Channel::Stable).is_some());
        assert!(downgrade_note(&v("2026.11.23-beta"), Channel::Beta).is_none());
        assert!(downgrade_note(&v("2026.11.23"), Channel::Stable).is_none());
    }

    // ---- release list ----

    fn asset(tag: &str, name: &str) -> Asset {
        Asset {
            name: name.into(),
            url: format!("{DOWNLOAD_PREFIX}{tag}/{name}"),
            size: 1000,
        }
    }

    fn release(tag: &str, prerelease: bool) -> Release {
        Release {
            tag: tag.into(),
            draft: false,
            prerelease,
            page_url: format!("https://github.com/ultimatejimmy/xpando/releases/tag/{tag}"),
            assets: vec![
                asset(tag, &format!("xpando_{tag}_x64-setup.exe")),
                asset(tag, &format!("xpando_{tag}_universal.dmg")),
                asset(tag, &format!("xpando_{tag}_amd64.AppImage")),
                asset(tag, &format!("xpando_{tag}_amd64.deb")),
                asset(tag, MANIFEST_FILE),
                asset(tag, &format!("xpando_{tag}_x64-setup.exe.sig")),
            ],
        }
    }

    const API_SAMPLE: &str = r#"[
      {"url":"https://api.github.com/x","html_url":"https://github.com/ultimatejimmy/xpando/releases/tag/2026.11.24-beta",
       "tag_name":"2026.11.24-beta","draft":false,"prerelease":true,"body":"Notes — \"quoted\"\r\n",
       "assets":[
         {"name":"xpando_2026.11.24-beta_x64-setup.exe","size":12345,"browser_download_url":"https://github.com/ultimatejimmy/xpando/releases/download/2026.11.24-beta/xpando_2026.11.24-beta_x64-setup.exe","id":1},
         {"name":"latest.json","size":300,"browser_download_url":"https://github.com/ultimatejimmy/xpando/releases/download/2026.11.24-beta/latest.json"}]},
      {"tag_name":"2026.11.23","draft":true,"prerelease":false,"assets":[]},
      {"html_url":"x","draft":false},
      {"tag_name":"2026.11.20","assets":null}
    ]"#;

    #[test]
    fn parses_the_github_response() {
        let r = parse_releases(API_SAMPLE).unwrap();
        assert_eq!(r.len(), 3); // the one without a tag is skipped
        assert_eq!(r[0].tag, "2026.11.24-beta");
        assert!(r[0].prerelease && !r[0].draft);
        assert_eq!(r[0].assets.len(), 2);
        assert_eq!(r[0].assets[0].size, 12345);
        assert!(r[0].page_url.ends_with("2026.11.24-beta"));
        assert!(r[1].draft);
        assert!(r[2].assets.is_empty());
        assert!(parse_releases("{}").is_err());
        assert!(parse_releases("not json").is_err());
        assert_eq!(parse_releases("[]").unwrap(), vec![]);
    }

    // ---- picking ----

    fn pick(current: &str, channel: Channel, releases: &[Release]) -> Option<String> {
        pick_update(&v(current), channel, None, releases, Os::Windows, "x86_64").map(|u| u.version)
    }

    #[test]
    fn stable_user_gets_newest_stable_never_a_beta() {
        let rs = [
            release("2026.11.25-beta", true),
            release("2026.11.24", false),
            release("2026.11.23", false),
        ];
        assert_eq!(
            pick("2026.11.20", Channel::Stable, &rs).as_deref(),
            Some("2026.11.24")
        );
        // Only a beta is newer: nothing for the stable channel.
        assert_eq!(pick("2026.11.24", Channel::Stable, &rs), None);
        // A release flagged pre-release on GitHub counts as a beta even with a plain tag.
        let flagged = [release("2026.11.26", true)];
        assert_eq!(pick("2026.11.20", Channel::Stable, &flagged), None);
        assert_eq!(
            pick("2026.11.20", Channel::Beta, &flagged).as_deref(),
            Some("2026.11.26")
        );
    }

    #[test]
    fn beta_user_gets_betas_and_newer_stable() {
        let rs = [
            release("2026.11.25-beta2", true),
            release("2026.11.25-beta", true),
            release("2026.11.24", false),
        ];
        assert_eq!(
            pick("2026.11.24", Channel::Beta, &rs).as_deref(),
            Some("2026.11.25-beta2")
        );
        assert_eq!(
            pick("2026.11.25-beta", Channel::Beta, &rs).as_deref(),
            Some("2026.11.25-beta2")
        );
        assert_eq!(pick("2026.11.25-beta2", Channel::Beta, &rs), None);
        // The stable that follows the betas is newer than all of them.
        let rs2 = [
            release("2026.11.26", false),
            release("2026.11.25-beta2", true),
        ];
        assert_eq!(
            pick("2026.11.25-beta2", Channel::Beta, &rs2).as_deref(),
            Some("2026.11.26")
        );
        assert_eq!(
            pick("2026.11.25-beta2", Channel::Stable, &rs2).as_deref(),
            Some("2026.11.26")
        );
    }

    #[test]
    fn same_day_stable_replaces_its_beta_and_patches_follow() {
        let rs = [release("2026.11.23.1", false), release("2026.11.23", false)];
        assert_eq!(
            pick("2026.11.23-beta3", Channel::Stable, &rs).as_deref(),
            Some("2026.11.23.1")
        );
        assert_eq!(
            pick("2026.11.23", Channel::Stable, &rs).as_deref(),
            Some("2026.11.23.1")
        );
        assert_eq!(pick("2026.11.23.1", Channel::Stable, &rs), None);
    }

    #[test]
    fn a_beta_user_on_stable_is_never_moved_to_an_older_release() {
        let rs = [release("2026.11.23", false)];
        assert_eq!(pick("2026.11.24-beta", Channel::Stable, &rs), None);
        assert_eq!(pick("2026.11.24-beta", Channel::Beta, &rs), None);
    }

    #[test]
    fn order_of_the_list_does_not_matter() {
        let rs = [
            release("2026.11.21", false),
            release("2026.11.24", false),
            release("2026.11.22", false),
        ];
        assert_eq!(
            pick("2026.11.1", Channel::Stable, &rs).as_deref(),
            Some("2026.11.24")
        );
    }

    #[test]
    fn drafts_odd_tags_and_incomplete_releases_are_skipped() {
        let mut draft = release("2026.11.30", false);
        draft.draft = true;
        let odd = release("v2026.11.29", false);
        let mut no_manifest = release("2026.11.28", false);
        no_manifest.assets.retain(|a| a.name != MANIFEST_FILE);
        let mut no_installer = release("2026.11.27", false);
        no_installer
            .assets
            .retain(|a| !a.name.ends_with("-setup.exe"));
        let mut off_site = release("2026.11.26", false);
        off_site.assets[0].url = "https://evil.example/x/xpando_2026.11.26_x64-setup.exe".into();
        let good = release("2026.11.25", false);
        let rs = [draft, odd, no_manifest, no_installer, off_site, good];
        // The newest *usable* release wins.
        assert_eq!(
            pick("2026.11.1", Channel::Stable, &rs).as_deref(),
            Some("2026.11.25")
        );
    }

    #[test]
    fn skipped_version_is_not_offered_but_a_newer_one_is() {
        let rs = [release("2026.11.24", false), release("2026.11.23", false)];
        let skip = v("2026.11.24");
        let got = pick_update(
            &v("2026.11.1"),
            Channel::Stable,
            Some(&skip),
            &rs,
            Os::Windows,
            "x86_64",
        );
        assert_eq!(got.unwrap().version, "2026.11.23");
        let rs = [release("2026.11.25", false), release("2026.11.24", false)];
        let got = pick_update(
            &v("2026.11.1"),
            Channel::Stable,
            Some(&skip),
            &rs,
            Os::Windows,
            "x86_64",
        );
        assert_eq!(got.unwrap().version, "2026.11.25");
    }

    #[test]
    fn update_carries_the_right_files() {
        let rs = [release("2026.11.24-beta", true)];
        let u = pick_update(
            &v("2026.11.1"),
            Channel::Beta,
            None,
            &rs,
            Os::Windows,
            "x86_64",
        )
        .unwrap();
        assert!(u.beta);
        assert_eq!(u.tag, "2026.11.24-beta");
        assert_eq!(u.installer.name, "xpando_2026.11.24-beta_x64-setup.exe");
        assert_eq!(u.manifest.name, MANIFEST_FILE);
        assert!(u.release_url.ends_with("/tag/2026.11.24-beta"));
        let stable = [release("2026.11.24", false)];
        let u = pick_update(
            &v("2026.11.1"),
            Channel::Stable,
            None,
            &stable,
            Os::Linux,
            "x86_64",
        )
        .unwrap();
        assert!(!u.beta);
        assert!(u.installer.name.ends_with("amd64.AppImage"));
    }

    // ---- assets ----

    #[test]
    fn chooses_the_installer_for_each_os_and_cpu() {
        let r = release("2026.11.23", false);
        let name = |os, arch| choose_installer(&r.assets, os, arch).map(|a| a.name.as_str());
        assert_eq!(
            name(Os::Windows, "x86_64"),
            Some("xpando_2026.11.23_x64-setup.exe")
        );
        // No ARM build: the x64 installer runs under emulation.
        assert_eq!(
            name(Os::Windows, "aarch64"),
            Some("xpando_2026.11.23_x64-setup.exe")
        );
        assert_eq!(
            name(Os::MacOs, "aarch64"),
            Some("xpando_2026.11.23_universal.dmg")
        );
        assert_eq!(
            name(Os::MacOs, "x86_64"),
            Some("xpando_2026.11.23_universal.dmg")
        );
        assert_eq!(
            name(Os::Linux, "x86_64"),
            Some("xpando_2026.11.23_amd64.AppImage")
        );
        // The deb is never picked, and there is no ARM Linux build to offer an x86 one.
        assert_eq!(name(Os::Linux, "aarch64"), None);
    }

    #[test]
    fn prefers_a_matching_cpu_and_ignores_other_files() {
        let a = |n: &str| asset("t", n);
        let assets = vec![
            a("xpando_1_x64-setup.exe"),
            a("xpando_1_arm64-setup.exe"),
            a("xpando_1_x64_en-US.msi"),
            a("xpando_1_aarch64.AppImage"),
            a("xpando_1_amd64.AppImage"),
            a("latest.json"),
        ];
        let n = |os, arch| choose_installer(&assets, os, arch).map(|x| x.name.clone());
        assert_eq!(
            n(Os::Windows, "aarch64").as_deref(),
            Some("xpando_1_arm64-setup.exe")
        );
        assert_eq!(
            n(Os::Windows, "x86_64").as_deref(),
            Some("xpando_1_x64-setup.exe")
        );
        assert_eq!(
            n(Os::Linux, "aarch64").as_deref(),
            Some("xpando_1_aarch64.AppImage")
        );
        assert_eq!(
            n(Os::Linux, "x86_64").as_deref(),
            Some("xpando_1_amd64.AppImage")
        );
        assert_eq!(n(Os::MacOs, "aarch64"), None);
        assert!(choose_installer(&[], Os::Windows, "x86_64").is_none());
    }

    // ---- addresses ----

    #[test]
    fn download_urls_must_be_this_projects_release_files() {
        let ok = "https://github.com/ultimatejimmy/xpando/releases/download/2026.11.23-beta2/xpando_2026.11.23-beta2_x64-setup.exe";
        assert!(validate_download_url(
            ok,
            "2026.11.23-beta2",
            "xpando_2026.11.23-beta2_x64-setup.exe"
        )
        .is_ok());
        let t = "2026.11.23";
        let f = "a.exe";
        for bad in [
            "http://github.com/ultimatejimmy/xpando/releases/download/2026.11.23/a.exe",
            "https://github.com.evil.example/ultimatejimmy/xpando/releases/download/2026.11.23/a.exe",
            "https://evil.example/ultimatejimmy/xpando/releases/download/2026.11.23/a.exe",
            "https://github.com@evil.example/ultimatejimmy/xpando/releases/download/2026.11.23/a.exe",
            "https://github.com:444/ultimatejimmy/xpando/releases/download/2026.11.23/a.exe",
            "https://github.com/someoneelse/xpando/releases/download/2026.11.23/a.exe",
            "https://github.com/ultimatejimmy/other/releases/download/2026.11.23/a.exe",
            "https://github.com/ultimatejimmy/xpando/releases/download/2026.11.22/a.exe",
            "https://github.com/ultimatejimmy/xpando/releases/download/2026.11.23/b.exe",
            "https://github.com/ultimatejimmy/xpando/releases/download/2026.11.23/a.exe?x=1",
            "https://github.com/ultimatejimmy/xpando/releases/download/2026.11.23/a.exe#x",
            "https://github.com/ultimatejimmy/xpando/releases/download/2026.11.23/../a.exe",
            "https://github.com/ultimatejimmy/xpando/releases/download/2026.11.23/sub/a.exe",
            "https://github.com/ultimatejimmy/xpando/releases/download/2026.11.23//a.exe",
            "https://github.com/ultimatejimmy/xpando/releases/download/2026.11.23/a.exe/",
            "https://github.com/ultimatejimmy/xpando/releases/download/2026.11.23/%2e%2e",
            "https://github.com/ultimatejimmy/xpando/releases/download/2026.11.23/a.exe ",
            "https://github.com/ultimatejimmy/xpando/releases/download/2026.11.23\\a.exe",
            "https://github.com/ultimatejimmy/xpando/releases/download/2026.11.23/",
            "https://github.com/ultimatejimmy/xpando/releases/download/",
            "https://GITHUB.com/ultimatejimmy/xpando/releases/download/2026.11.23/a.exe",
            "",
        ] {
            assert!(validate_download_url(bad, t, f).is_err(), "{bad:?} should be refused");
        }
        // Asking for dot segments by name is refused too.
        assert!(validate_download_url(
            "https://github.com/ultimatejimmy/xpando/releases/download/../a.exe",
            "..",
            "a.exe"
        )
        .is_err());
    }

    #[test]
    fn redirects_may_only_go_to_githubs_own_hosts_over_https() {
        for ok in [
            "https://objects.githubusercontent.com/github-production-release-asset/1?X-Amz-Signature=abc&x=y",
            "https://release-assets.githubusercontent.com/github-production-release-asset/2?sp=r",
            "https://github-releases.githubusercontent.com/123/abc?x=1",
            "https://github.com/ultimatejimmy/xpando/releases/download/2026.11.23/a.exe",
            "https://api.github.com/repositories/1/releases?per_page=30",
        ] {
            assert!(validate_redirect_url(ok).is_ok(), "{ok}");
        }
        for bad in [
            "http://objects.githubusercontent.com/x",
            "https://evil.example/x",
            "https://objects.githubusercontent.com.evil.example/x",
            "https://evilgithubusercontent.com/x",
            "https://user@objects.githubusercontent.com/x",
            "https://objects.githubusercontent.com@evil.example/x",
            "https://objects.githubusercontent.com:8443/x",
            "https://objects.githubusercontent.com\\@evil.example/x",
            "https://objects.githubusercontent.com/x y",
            "https://objects.githubusercontent.com/x\r\nHost: evil",
            "https://raw.githubusercontent.com/x",
            "//objects.githubusercontent.com/x",
            "/relative/path",
            "ftp://github.com/x",
            "https://",
            "",
        ] {
            assert!(
                validate_redirect_url(bad).is_err(),
                "{bad:?} should be refused"
            );
        }
    }

    #[test]
    fn reads_the_last_response_of_a_header_dump() {
        let dump = "HTTP/2 302 \r\nlocation: https://objects.githubusercontent.com/x?y=1\r\ncontent-length: 0\r\n\r\n";
        assert_eq!(
            parse_response_head(dump),
            Some((
                302,
                Some("https://objects.githubusercontent.com/x?y=1".to_string())
            ))
        );
        let two = "HTTP/1.1 100 Continue\r\n\r\nHTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\n";
        assert_eq!(parse_response_head(two), Some((200, None)));
        // A Location from an earlier response must not leak into a later one.
        let stale = "HTTP/1.1 302 Found\r\nLocation: https://a/\r\n\r\nHTTP/1.1 200 OK\r\n\r\n";
        assert_eq!(parse_response_head(stale), Some((200, None)));
        let caps = "HTTP/2 301\nLOCATION: https://github.com/x\n";
        assert_eq!(
            parse_response_head(caps),
            Some((301, Some("https://github.com/x".into())))
        );
        assert_eq!(parse_response_head(""), None);
        assert_eq!(parse_response_head("garbage"), None);
    }
}
