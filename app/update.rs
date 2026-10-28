//! Explicit, signed GitHub-release updates for the installed Windows app.

use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

const API: &str = "https://api.github.com/repos/amxcodes/little-world-pluto/releases/latest";
const RELEASE_BASE: &str = "https://github.com/amxcodes/little-world-pluto/releases/download/";

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct State {
    pub installed_version: String,
    pub latest_version: Option<String>,
    pub checking: bool,
    pub downloading: bool,
    pub available: bool,
    pub can_install: bool,
    pub notice: String,
}

#[derive(Clone)]
struct Release {
    version: String,
    installer_url: String,
    checksums_url: String,
}

enum Event {
    Checked(Result<Option<Release>, String>),
    Downloaded(Result<PathBuf, String>),
}

pub struct Updater {
    pub state: State,
    release: Option<Release>,
    sender: Sender<Event>,
    receiver: Receiver<Event>,
}

impl Default for Updater {
    fn default() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            state: State {
                installed_version: env!("CARGO_PKG_VERSION").into(),
                latest_version: None,
                checking: false,
                downloading: false,
                available: false,
                can_install: option_env!("PLUTO_SIGNING_PUBLISHER").is_some()
                    && option_env!("PLUTO_SIGNING_CERT_THUMBPRINT").is_some(),
                notice: "Check for a signed Pluto release.".into(),
            },
            release: None,
            sender,
            receiver,
        }
    }
}

impl Updater {
    pub fn check(&mut self) {
        if self.state.checking || self.state.downloading {
            return;
        }
        self.state.checking = true;
        self.state.notice = "Checking GitHub releases…".into();
        let sender = self.sender.clone();
        thread::spawn(move || {
            let _ = sender.send(Event::Checked(fetch_latest()));
        });
    }

    pub fn install(&mut self) {
        if self.state.checking || self.state.downloading {
            return;
        }
        let Some(release) = self.release.clone() else {
            return;
        };
        let Some(publisher) = option_env!("PLUTO_SIGNING_PUBLISHER") else {
            self.state.notice = "Install updates from the signed release build.".into();
            return;
        };
        let Some(thumbprint) = option_env!("PLUTO_SIGNING_CERT_THUMBPRINT") else {
            self.state.notice = "Install updates from the signed release build.".into();
            return;
        };
        self.state.downloading = true;
        self.state.notice = "Downloading and verifying Setup…".into();
        let sender = self.sender.clone();
        let publisher = publisher.to_owned();
        let thumbprint = thumbprint.to_owned();
        thread::spawn(move || {
            let _ = sender.send(Event::Downloaded(download_verified(
                &release,
                &publisher,
                &thumbprint,
            )));
        });
    }

    /// Returns the verified installer only after an explicit Install click.
    pub fn poll(&mut self) -> Option<PathBuf> {
        let mut ready = None;
        while let Ok(event) = self.receiver.try_recv() {
            match event {
                Event::Checked(Ok(Some(release))) => {
                    self.state.checking = false;
                    self.state.latest_version = Some(release.version.clone());
                    self.state.available = true;
                    self.state.notice = if self.state.can_install {
                        format!("Pluto {} is ready to install.", release.version)
                    } else {
                        format!(
                            "Pluto {} is available. Install it from the signed GitHub release.",
                            release.version
                        )
                    };
                    self.release = Some(release);
                }
                Event::Checked(Ok(None)) => {
                    self.state.checking = false;
                    self.state.available = false;
                    self.release = None;
                    self.state.notice = "Pluto is up to date.".into();
                }
                Event::Checked(Err(error)) => {
                    self.state.checking = false;
                    self.state.notice = error;
                }
                Event::Downloaded(Ok(path)) => {
                    self.state.downloading = false;
                    self.state.notice = "Opening verified Setup…".into();
                    ready = Some(path);
                }
                Event::Downloaded(Err(error)) => {
                    self.state.downloading = false;
                    self.state.notice = error;
                }
            }
        }
        ready
    }
}

fn version_parts(value: &str) -> Option<[u32; 3]> {
    let parts: Vec<_> = value
        .split('.')
        .map(str::parse::<u32>)
        .collect::<Result<_, _>>()
        .ok()?;
    if parts.len() == 3 {
        Some([parts[0], parts[1], parts[2]])
    } else {
        None
    }
}

fn fetch_latest() -> Result<Option<Release>, String> {
    let output = curl(API, None)?;
    let json: serde_json::Value = serde_json::from_slice(&output)
        .map_err(|_| "Could not read the release response.".to_owned())?;
    if json["draft"].as_bool() == Some(true) || json["prerelease"].as_bool() == Some(true) {
        return Err("The latest release is not stable.".into());
    }
    let tag = json["tag_name"]
        .as_str()
        .ok_or("Release has no version tag.")?;
    let version = tag.strip_prefix('v').ok_or("Release tag is invalid.")?;
    let latest = version_parts(version).ok_or("Release version is invalid.")?;
    let current =
        version_parts(env!("CARGO_PKG_VERSION")).ok_or("Installed version is invalid.")?;
    if latest <= current {
        return Ok(None);
    }
    let prefix = format!("{RELEASE_BASE}{tag}/");
    let installer_name = format!("Pluto-Setup-{tag}-windows-x64.exe");
    let assets = json["assets"].as_array().ok_or("Release has no assets.")?;
    let find = |name: &str| -> Result<String, String> {
        let url = assets
            .iter()
            .find(|asset| asset["name"] == name)
            .and_then(|asset| asset["browser_download_url"].as_str())
            .ok_or_else(|| format!("Release is missing {name}."))?;
        if url != format!("{prefix}{name}") {
            return Err("Release asset URL is unexpected.".into());
        }
        Ok(url.to_owned())
    };
    Ok(Some(Release {
        version: version.into(),
        installer_url: find(&installer_name)?,
        checksums_url: find("SHA256SUMS.txt")?,
    }))
}

fn curl(url: &str, path: Option<&Path>) -> Result<Vec<u8>, String> {
    use std::os::windows::process::CommandExt;
    let mut command = Command::new("curl.exe");
    command.creation_flags(0x0800_0000).args([
        "--silent",
        "--show-error",
        "--fail",
        "--location",
        "--max-redirs",
        "5",
        "--connect-timeout",
        "8",
        "--max-time",
        "120",
        "--max-filesize",
        "250000000",
        "--user-agent",
        "Pluto-Desktop-Updater",
        url,
    ]);
    if let Some(path) = path {
        command.arg("--output").arg(path);
    }
    let output = command
        .output()
        .map_err(|_| "Could not start the release download.".to_owned())?;
    if !output.status.success() {
        return Err("Could not download the GitHub release. Try again later.".into());
    }
    Ok(output.stdout)
}

fn download_verified(
    release: &Release,
    publisher: &str,
    thumbprint: &str,
) -> Result<PathBuf, String> {
    let dir = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .ok_or("Local app data is unavailable.")?
        .join("Pluto")
        .join("updates");
    std::fs::create_dir_all(&dir).map_err(|_| "Could not create the update folder.".to_owned())?;
    let name = format!("Pluto-Setup-v{}-windows-x64.exe", release.version);
    let partial = dir.join(format!("{name}.partial.exe"));
    let final_path = dir.join(name.clone());
    let result = (|| {
        let sums = curl(&release.checksums_url, None)?;
        if sums.len() > 1024 * 1024 {
            return Err("Release checksum file is too large.".into());
        }
        let sums =
            String::from_utf8(sums).map_err(|_| "Release checksum file is invalid.".to_owned())?;
        let expected = sums
            .lines()
            .filter_map(|line| line.split_once("  "))
            .find(|(_, file)| *file == name)
            .map(|(hash, _)| hash.to_ascii_lowercase())
            .ok_or("Installer checksum is missing.")?;
        if expected.len() != 64 || !expected.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err("Installer checksum is invalid.".into());
        }
        curl(&release.installer_url, Some(&partial))?;
        verify_file(&partial, publisher, thumbprint, &expected)?;
        if final_path.exists() {
            std::fs::remove_file(&final_path)
                .map_err(|_| "Could not replace the previous update download.".to_owned())?;
        }
        std::fs::rename(&partial, &final_path)
            .map_err(|_| "Could not finalize the installer.".to_owned())?;
        Ok(final_path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&partial);
    }
    result
}

fn verify_file(
    path: &Path,
    publisher: &str,
    thumbprint: &str,
    expected_hash: &str,
) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    let script = "$hash = (Get-FileHash -LiteralPath $env:PLUTO_UPDATE_FILE -Algorithm SHA256).Hash; $sig = Get-AuthenticodeSignature -LiteralPath $env:PLUTO_UPDATE_FILE; if ($hash -ine $env:PLUTO_UPDATE_SHA256 -or $sig.Status -notin @('Valid','NotTrusted') -or $null -eq $sig.SignerCertificate -or $sig.SignerCertificate.Subject -cne $env:PLUTO_UPDATE_PUBLISHER -or $sig.SignerCertificate.Thumbprint -ine $env:PLUTO_UPDATE_THUMBPRINT) { exit 1 }";
    let output = Command::new("powershell.exe")
        .creation_flags(0x0800_0000)
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .env("PLUTO_UPDATE_FILE", path)
        .env("PLUTO_UPDATE_PUBLISHER", publisher)
        .env("PLUTO_UPDATE_THUMBPRINT", thumbprint)
        .env("PLUTO_UPDATE_SHA256", expected_hash)
        .output()
        .map_err(|_| "Could not verify the installer signature.".to_owned())?;
    if output.status.success() {
        Ok(())
    } else {
        Err("Installer checksum or signature did not match Pluto.".into())
    }
}

#[cfg(test)]
mod tests {
    use super::version_parts;
    #[test]
    fn versions_are_strictly_numeric() {
        assert_eq!(version_parts("1.2.3"), Some([1, 2, 3]));
        assert_eq!(version_parts("1.2.3-beta"), None);
        assert_eq!(version_parts("1.2"), None);
    }
}
