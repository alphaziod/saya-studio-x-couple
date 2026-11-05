//! In-app update check: pure helpers (release parsing, version comparison, asset choice,
//! checksum verification, install command). No network and no process here; the shell does the I/O.
use crate::Result;
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::cmp::Ordering;
use std::path::Path;

pub const RELEASE_API: &str = "https://api.github.com/repos/alphaziod/saya-studio-x-couple/releases/latest";
pub const RELEASES_PAGE: &str = "https://github.com/alphaziod/saya-studio-x-couple/releases";
const URL_PREFIX: &str = "https://github.com/alphaziod/saya-studio-x-couple/";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    /// Version without the `studio-v` / `v` prefix.
    pub version: String,
    pub tag: String,
    pub notes_url: String,
    pub assets: Vec<Asset>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    UpToDate,
    Available,
    LocalBuild,
    Offline,
}

/// Only addresses of the project's own GitHub repository are ever downloaded or opened.
pub fn is_project_url(url: &str) -> bool {
    url.starts_with(URL_PREFIX) && !url.contains("..") && !url.chars().any(|c| c.is_control() || c == ' ')
}

pub fn strip_tag(tag: &str) -> &str {
    let tag = tag.trim();
    tag.strip_prefix("studio-v").or_else(|| tag.strip_prefix('v')).unwrap_or(tag)
}

pub fn parse_release(text: &str) -> Result<Release> {
    let json: Value = serde_json::from_str(text).map_err(|_| "Réponse GitHub illisible")?;
    let tag = json["tag_name"].as_str().filter(|t| !t.trim().is_empty()).ok_or("Version GitHub absente")?;
    let notes_url = json["html_url"].as_str().filter(|u| is_project_url(u)).unwrap_or(RELEASES_PAGE).to_owned();
    let assets = json["assets"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|a| {
                    let name = a["name"].as_str()?;
                    let url = a["browser_download_url"].as_str()?;
                    if !is_safe_file_name(name) || !is_project_url(url) {
                        return None;
                    }
                    Some(Asset { name: name.into(), url: url.into(), size: a["size"].as_u64().unwrap_or(0) })
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(Release { version: strip_tag(tag).to_owned(), tag: tag.to_owned(), notes_url, assets })
}

pub fn is_safe_file_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() < 200
        && name != "."
        && !name.contains("..")
        && !name.contains(['/', '\\', ':', '\0'])
        && !name.starts_with('-')
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    core: [u64; 3],
    pre: Vec<String>,
}

pub fn parse_version(text: &str) -> Option<Version> {
    let text = strip_tag(text).split('+').next()?;
    let (core, pre) = match text.split_once('-') {
        Some((core, pre)) => (core, Some(pre)),
        None => (text, None),
    };
    let mut numbers = core.split('.');
    let mut out = [0u64; 3];
    for slot in &mut out {
        *slot = numbers.next()?.parse().ok()?;
    }
    if numbers.next().is_some() {
        return None;
    }
    let pre = match pre {
        Some(p) if p.is_empty() => return None,
        Some(p) => p.split('.').map(str::to_owned).collect(),
        None => Vec::new(),
    };
    Some(Version { core: out, pre })
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        self.core.cmp(&other.core).then_with(|| match (self.pre.is_empty(), other.pre.is_empty()) {
            (true, true) => Ordering::Equal,
            (true, false) => Ordering::Greater,
            (false, true) => Ordering::Less,
            (false, false) => {
                for (a, b) in self.pre.iter().zip(&other.pre) {
                    let order = match (a.parse::<u64>(), b.parse::<u64>()) {
                        (Ok(x), Ok(y)) => x.cmp(&y),
                        (Ok(_), Err(_)) => Ordering::Less,
                        (Err(_), Ok(_)) => Ordering::Greater,
                        _ => a.cmp(b),
                    };
                    if order != Ordering::Equal {
                        return order;
                    }
                }
                self.pre.len().cmp(&other.pre.len())
            }
        })
    }
}
impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// A development or worktree build (`1.0.1-dev.3`, `…wt…`) or an unreadable version.
pub fn is_local_build(current: &str) -> bool {
    current.contains(".dev.") || current.contains("-dev") || current.contains("wt") || parse_version(current).is_none()
}

pub fn compare(current: &str, latest_tag: &str) -> Result<Status> {
    if is_local_build(current) {
        return Ok(Status::LocalBuild);
    }
    let current = parse_version(current).ok_or("Version locale illisible")?;
    let latest = parse_version(latest_tag).ok_or("Version GitHub illisible")?;
    Ok(if latest > current { Status::Available } else { Status::UpToDate })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Pacman,
    Deb,
    RpmFedora,
    RpmSuse,
    AppImage,
    WindowsExe,
    MacDmg,
}

/// The package family wanted on Linux, from the content of `/etc/os-release`.
pub fn linux_format(os_release: &str) -> Format {
    let mut ids: Vec<String> = Vec::new();
    for line in os_release.lines() {
        if let Some((key, value)) = line.split_once('=') {
            if key == "ID" || key == "ID_LIKE" {
                ids.extend(value.trim().trim_matches(['"', '\'']).split_whitespace().map(str::to_lowercase));
            }
        }
    }
    let has = |names: &[&str]| ids.iter().any(|id| names.contains(&id.as_str()));
    if has(&["arch", "endeavouros", "cachyos", "manjaro", "garuda"]) {
        Format::Pacman
    } else if has(&["debian", "ubuntu", "linuxmint", "pop"]) {
        Format::Deb
    } else if has(&["opensuse", "opensuse-leap", "opensuse-tumbleweed", "suse", "sles"]) {
        Format::RpmSuse
    } else if has(&["fedora", "rhel", "centos"]) {
        Format::RpmFedora
    } else {
        Format::AppImage
    }
}

pub fn format_of(name: &str) -> Option<Format> {
    let lower = name.to_lowercase();
    if lower.ends_with(".pkg.tar.zst") {
        Some(Format::Pacman)
    } else if lower.ends_with(".deb") {
        Some(Format::Deb)
    } else if lower.ends_with(".opensuse.x86_64.rpm") {
        Some(Format::RpmSuse)
    } else if lower.ends_with(".rpm") {
        Some(Format::RpmFedora)
    } else if lower.ends_with(".appimage") {
        Some(Format::AppImage)
    } else if lower.ends_with(".exe") {
        Some(Format::WindowsExe)
    } else if lower.ends_with(".dmg") {
        Some(Format::MacDmg)
    } else {
        None
    }
}

/// `os` is `std::env::consts::OS`, `arch` is `std::env::consts::ARCH`.
pub fn choose_asset<'a>(assets: &'a [Asset], os: &str, arch: &str, os_release: &str) -> Option<&'a Asset> {
    let find = |matches: &dyn Fn(&str) -> bool| assets.iter().find(|a| matches(&a.name.to_lowercase()));
    match os {
        "windows" => find(&|n| n.ends_with("-windows-x64.exe")),
        "macos" => {
            let arm = arch == "aarch64" || arch == "arm64";
            find(&|n| {
                n.ends_with(".dmg")
                    && if arm { n.contains("arm64") || n.contains("aarch64") } else { n.contains("x64") || n.contains("x86_64") || n.contains("amd64") }
            })
        }
        "linux" => {
            let appimage = || find(&|n| n.ends_with(".appimage") && (n.contains("x86_64") || n.contains("amd64")))
                .or_else(|| find(&|n| n.ends_with(".appimage")));
            let native = match linux_format(os_release) {
                Format::Pacman => find(&|n| n.ends_with("-x86_64.pkg.tar.zst")),
                Format::Deb => find(&|n| n.ends_with("_amd64.deb")),
                Format::RpmFedora => find(&|n| n.ends_with(".fedora.x86_64.rpm")),
                Format::RpmSuse => find(&|n| n.ends_with(".opensuse.x86_64.rpm")),
                _ => None,
            };
            native.or_else(appimage)
        }
        _ => None,
    }
}

/// Finds `name` in a `SHA256SUMS` file (`<hex>  <name>` or `<hex> *<name>`).
pub fn parse_sums(text: &str, name: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let (hash, file) = line.trim().split_once(char::is_whitespace)?;
        let file = file.trim_start().trim_start_matches('*');
        (file == name && hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit())).then(|| hash.to_lowercase())
    })
}

pub fn file_sha256(path: &Path) -> Result<String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1 << 20];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

pub fn verify_sha256(path: &Path, expected: &str) -> Result<()> {
    if file_sha256(path)? == expected.trim().to_lowercase() {
        Ok(())
    } else {
        Err("Somme de contrôle incorrecte : fichier refusé".into())
    }
}

/// What to do with a downloaded file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Plan {
    /// Run through `pkexec` (the first element is the real program).
    Privileged(Vec<String>),
    /// Hand the file to the system (installer `.exe` / `.dmg`).
    Open,
    /// Nothing to run: the file is only saved.
    SavedOnly,
}

pub fn install_plan(format: Format, file: &str) -> Plan {
    let args = |parts: &[&str]| Plan::Privileged(parts.iter().map(|s| s.to_string()).chain([file.to_owned()]).collect());
    match format {
        Format::Pacman => args(&["pacman", "-U", "--noconfirm"]),
        Format::Deb => args(&["apt-get", "install", "-y"]),
        Format::RpmFedora => args(&["dnf", "install", "-y"]),
        Format::RpmSuse => args(&["zypper", "--non-interactive", "install"]),
        Format::AppImage => Plan::SavedOnly,
        Format::WindowsExe | Format::MacDmg => Plan::Open,
    }
}

fn quote(arg: &str) -> String {
    if !arg.is_empty() && arg.chars().all(|c| c.is_ascii_alphanumeric() || "/._-+:@%=".contains(c)) {
        arg.to_owned()
    } else {
        format!("'{}'", arg.replace('\'', "'\\''"))
    }
}

/// The command shown to the owner (and copyable): `pkexec pacman -U --noconfirm <file>`.
pub fn display_command(plan: &Plan, file: &str) -> String {
    match plan {
        Plan::Privileged(argv) => format!("pkexec {}", argv.iter().map(|a| quote(a)).collect::<Vec<_>>().join(" ")),
        Plan::Open => format!("{} {}", if cfg!(target_os = "macos") { "open" } else { "start" }, quote(file)),
        Plan::SavedOnly => format!("chmod +x {0} && {0}", quote(file)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RELEASE: &str = r#"{"tag_name":"studio-v1.0.1","html_url":"https://github.com/alphaziod/saya-studio-x-couple/releases/tag/studio-v1.0.1",
      "assets":[
       {"name":"saya-studio-1.0.1-1-x86_64.pkg.tar.zst","browser_download_url":"https://github.com/alphaziod/saya-studio-x-couple/releases/download/studio-v1.0.1/saya-studio-1.0.1-1-x86_64.pkg.tar.zst","size":42},
       {"name":"saya-studio_1.0.1_amd64.deb","browser_download_url":"https://github.com/alphaziod/saya-studio-x-couple/releases/download/studio-v1.0.1/saya-studio_1.0.1_amd64.deb","size":43},
       {"name":"saya-studio-1.0.1-1.fedora.x86_64.rpm","browser_download_url":"https://github.com/alphaziod/saya-studio-x-couple/releases/download/studio-v1.0.1/f.rpm","size":44},
       {"name":"saya-studio-1.0.1-1.opensuse.x86_64.rpm","browser_download_url":"https://github.com/alphaziod/saya-studio-x-couple/releases/download/studio-v1.0.1/o.rpm","size":45},
       {"name":"Saya-Studio-1.0.1-x86_64.AppImage","browser_download_url":"https://github.com/alphaziod/saya-studio-x-couple/releases/download/studio-v1.0.1/a.AppImage","size":46},
       {"name":"Saya-Studio-1.0.1-windows-x64.exe","browser_download_url":"https://github.com/alphaziod/saya-studio-x-couple/releases/download/studio-v1.0.1/w.exe","size":47},
       {"name":"Saya-Studio-1.0.1-arm64.dmg","browser_download_url":"https://github.com/alphaziod/saya-studio-x-couple/releases/download/studio-v1.0.1/m1.dmg","size":48},
       {"name":"Saya-Studio-1.0.1-x64.dmg","browser_download_url":"https://github.com/alphaziod/saya-studio-x-couple/releases/download/studio-v1.0.1/m2.dmg","size":49},
       {"name":"SHA256SUMS","browser_download_url":"https://github.com/alphaziod/saya-studio-x-couple/releases/download/studio-v1.0.1/SHA256SUMS","size":5},
       {"name":"evil","browser_download_url":"https://evil.example/x","size":1}]}"#;

    #[test]
    fn parses_release_and_drops_foreign_assets() {
        let r = parse_release(RELEASE).unwrap();
        assert_eq!(r.version, "1.0.1");
        assert_eq!(r.tag, "studio-v1.0.1");
        assert_eq!(r.assets.len(), 9);
        assert_eq!(r.assets[0].size, 42);
        assert!(r.notes_url.ends_with("studio-v1.0.1"));
        assert!(parse_release("{}").is_err());
        assert!(parse_release("nope").is_err());
        let foreign = parse_release(r#"{"tag_name":"v2.0.0","html_url":"https://evil.example/"}"#).unwrap();
        assert_eq!(foreign.notes_url, RELEASES_PAGE);
        assert!(foreign.assets.is_empty());
    }

    #[test]
    fn compares_versions() {
        assert_eq!(compare("1.0.0", "studio-v1.0.1").unwrap(), Status::Available);
        assert_eq!(compare("1.0.1", "studio-v1.0.1").unwrap(), Status::UpToDate);
        assert_eq!(compare("1.0.2", "studio-v1.0.1").unwrap(), Status::UpToDate);
        assert_eq!(compare("1.0.0-beta.2", "studio-v1.0.0").unwrap(), Status::Available);
        assert_eq!(compare("1.0.0-beta.2", "studio-v1.0.0-beta.10").unwrap(), Status::Available);
        assert_eq!(compare("1.0.0-beta.2", "studio-v1.0.0-beta.2").unwrap(), Status::UpToDate);
        assert_eq!(compare("1.0.0", "studio-v1.0.0-beta.2").unwrap(), Status::UpToDate);
        assert_eq!(compare("1.9.0", "studio-v1.10.0").unwrap(), Status::Available);
        assert_eq!(compare("1.0.1-dev.3", "studio-v9.0.0").unwrap(), Status::LocalBuild);
        assert_eq!(compare("1.0.1.dev.3", "studio-v9.0.0").unwrap(), Status::LocalBuild);
        assert_eq!(compare("1.0.1-wt2", "studio-v9.0.0").unwrap(), Status::LocalBuild);
        assert_eq!(compare("1.0.1-alpha.wt", "studio-v9.0.0").unwrap(), Status::LocalBuild);
        assert!(compare("1.0.0", "garbage").is_err());
        assert!(parse_version("1.0").is_none());
        assert!(parse_version("1.0.0.0").is_none());
    }

    #[test]
    fn picks_the_asset_for_the_system() {
        let assets = parse_release(RELEASE).unwrap().assets;
        let pick = |os: &str, arch: &str, rel: &str| choose_asset(&assets, os, arch, rel).map(|a| a.name.as_str());
        assert_eq!(pick("linux", "x86_64", "ID=endeavouros\nID_LIKE=arch\n"), Some("saya-studio-1.0.1-1-x86_64.pkg.tar.zst"));
        assert_eq!(pick("linux", "x86_64", "ID=cachyos\nID_LIKE=\"arch\"\n"), Some("saya-studio-1.0.1-1-x86_64.pkg.tar.zst"));
        assert_eq!(pick("linux", "x86_64", "ID=ubuntu\nID_LIKE=debian\n"), Some("saya-studio_1.0.1_amd64.deb"));
        assert_eq!(pick("linux", "x86_64", "ID=fedora\n"), Some("saya-studio-1.0.1-1.fedora.x86_64.rpm"));
        assert_eq!(pick("linux", "x86_64", "ID=\"opensuse-tumbleweed\"\nID_LIKE=\"opensuse suse\"\n"), Some("saya-studio-1.0.1-1.opensuse.x86_64.rpm"));
        assert_eq!(pick("linux", "x86_64", "ID=gentoo\n"), Some("Saya-Studio-1.0.1-x86_64.AppImage"));
        assert_eq!(pick("linux", "x86_64", ""), Some("Saya-Studio-1.0.1-x86_64.AppImage"));
        assert_eq!(pick("windows", "x86_64", ""), Some("Saya-Studio-1.0.1-windows-x64.exe"));
        assert_eq!(pick("macos", "aarch64", ""), Some("Saya-Studio-1.0.1-arm64.dmg"));
        assert_eq!(pick("macos", "x86_64", ""), Some("Saya-Studio-1.0.1-x64.dmg"));
        assert_eq!(pick("freebsd", "x86_64", ""), None);
        // Missing native package: fall back to the AppImage.
        let only_appimage: Vec<Asset> = assets.iter().filter(|a| a.name.ends_with("AppImage")).cloned().collect();
        assert_eq!(choose_asset(&only_appimage, "linux", "x86_64", "ID=arch").map(|a| a.name.as_str()), Some("Saya-Studio-1.0.1-x86_64.AppImage"));
    }

    #[test]
    fn sums_and_verification() {
        let hash = "a".repeat(64);
        let sums = format!("{hash}  one.deb\n{}  *two.rpm\nshort  bad\n", "B".repeat(64));
        assert_eq!(parse_sums(&sums, "one.deb"), Some(hash));
        assert_eq!(parse_sums(&sums, "two.rpm"), Some("b".repeat(64)));
        assert_eq!(parse_sums(&sums, "bad"), None);
        assert_eq!(parse_sums(&sums, "missing"), None);
        let dir = std::env::temp_dir().join(format!("saya-updates-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("f.bin");
        std::fs::write(&file, b"abc").unwrap();
        let abc = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert_eq!(file_sha256(&file).unwrap(), abc);
        verify_sha256(&file, abc).unwrap();
        verify_sha256(&file, &abc.to_uppercase()).unwrap();
        assert!(verify_sha256(&file, &"0".repeat(64)).is_err());
        assert!(verify_sha256(&dir.join("absent"), abc).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn builds_install_commands() {
        let f = "/home/u/.local/share/x/updates/saya studio.pkg.tar.zst";
        assert_eq!(display_command(&install_plan(Format::Pacman, "/u/a.zst"), "/u/a.zst"), "pkexec pacman -U --noconfirm /u/a.zst");
        assert_eq!(display_command(&install_plan(Format::Deb, "/u/a.deb"), "/u/a.deb"), "pkexec apt-get install -y /u/a.deb");
        assert_eq!(display_command(&install_plan(Format::RpmFedora, "/u/a.rpm"), "/u/a.rpm"), "pkexec dnf install -y /u/a.rpm");
        assert_eq!(display_command(&install_plan(Format::RpmSuse, "/u/a.rpm"), "/u/a.rpm"), "pkexec zypper --non-interactive install /u/a.rpm");
        assert!(display_command(&install_plan(Format::Pacman, f), f).ends_with("'/home/u/.local/share/x/updates/saya studio.pkg.tar.zst'"));
        assert_eq!(install_plan(Format::AppImage, "/u/a"), Plan::SavedOnly);
        assert_eq!(install_plan(Format::WindowsExe, "/u/a.exe"), Plan::Open);
        assert_eq!(install_plan(Format::MacDmg, "/u/a.dmg"), Plan::Open);
        assert_eq!(format_of("x.pkg.tar.zst"), Some(Format::Pacman));
        assert_eq!(format_of("x.opensuse.x86_64.rpm"), Some(Format::RpmSuse));
        assert_eq!(format_of("x.fedora.x86_64.rpm"), Some(Format::RpmFedora));
        assert_eq!(format_of("SHA256SUMS"), None);
    }

    #[test]
    fn url_and_name_guards() {
        assert!(is_project_url("https://github.com/alphaziod/saya-studio-x-couple/releases/download/a/b"));
        assert!(!is_project_url("https://github.com/other/repo/releases"));
        assert!(!is_project_url("http://github.com/alphaziod/saya-studio-x-couple/"));
        assert!(!is_project_url("https://github.com/alphaziod/saya-studio-x-couple/../x"));
        assert!(is_safe_file_name("a.deb"));
        assert!(!is_safe_file_name("../a.deb"));
        assert!(!is_safe_file_name("a/b"));
        assert!(!is_safe_file_name("-rf"));
    }
}
