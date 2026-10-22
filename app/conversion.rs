//! On-demand local conversion. The UI holds metadata; one worker owns decoded media.
use std::{
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Read},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Arc, atomic::{AtomicBool, Ordering}, mpsc::{self, Receiver, SyncSender}},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use image::{DynamicImage, ImageDecoder, ImageEncoder, ImageFormat};
use serde::{Deserialize, Serialize};

const MAX_FILES: usize = 32;
const MAX_PIXELS: u64 = 32_000_000;
const MAX_SUBTITLE_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub id: u64,
    pub name: String,
    pub kind: String,
    pub bytes: u64,
    pub width: u32,
    pub height: u32,
    pub duration: f64,
    pub status: String,
    pub message: String,
    pub progress: Option<f64>,
    pub formats: Vec<String>,
    pub output_bytes: u64,
    #[serde(skip)]
    pub path: PathBuf,
    #[serde(skip)]
    pub output: Option<PathBuf>,
}

#[derive(Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Profile {
    pub format: String,
    pub width: u32,
    pub height: u32,
    pub max_bytes: u64,
    pub allow_smaller: bool,
    pub quality: u8,
}

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct State {
    pub items: Vec<Item>,
    pub busy: bool,
    pub inspecting: bool,
    pub media_available: bool,
    pub destination: String,
    pub destination_notice: String,
    pub notice: String,
    pub engine_path: String,
    pub engine_source: String,
    pub engine_busy: bool,
    pub engine_notice: String,
    pub engine_phase: String,
    pub engine_downloaded_bytes: u64,
    pub engine_total_bytes: u64,
    pub installed_version: String,
    pub latest_version: String,
    pub update_available: bool,
    pub checking_update: bool,
}

enum Event { Inspected(Vec<Item>, bool), Item(Item), Finished, EngineProgress(String, u64, u64), EngineInstalled(Result<PathBuf, String>), EngineVersion(PathBuf, String, Result<String, String>) }

pub struct Converter {
    pub state: State,
    sender: SyncSender<Event>,
    receiver: Receiver<Event>,
    cancel: Arc<AtomicBool>,
    engine: Option<PathBuf>,
    next_id: u64,
    last_version_check: Instant,
    reinspect_after_finish: bool,
    pending_additions: Vec<PathBuf>,
}

fn config_path() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|p| PathBuf::from(p).join("Island").join("conversion-engine.txt"))
}

fn output_config_path() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|p| PathBuf::from(p).join("Island").join("conversion-output.txt"))
}

fn managed_engine() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|p| PathBuf::from(p).join("Island").join("tools").join("ffmpeg").join("ffmpeg.exe"))
}

fn install_managed_engine(sender: &SyncSender<Event>) -> Result<PathBuf, String> {
    let destination = managed_engine().ok_or("Could not find your app data folder.")?;
    let root = destination.parent().ok_or("Could not find the FFmpeg tools folder.")?;
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    let stage = root.join(format!("stage-{}-{stamp:x}", std::process::id()));
    // FFmpeg links to gyan.dev as a Windows build distributor. Extract only the
    // two expected binaries to fixed app-owned paths; never unpack archive paths.
    const SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
try {
$root = Join-Path $env:LOCALAPPDATA 'Island\tools\ffmpeg'
New-Item -ItemType Directory -Path $root -Force | Out-Null
$stage = $env:ISLAND_FFMPEG_STAGE
$prefix = [IO.Path]::GetFullPath($root).TrimEnd('\') + '\'
if (-not [IO.Path]::GetFullPath($stage).StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)) { throw 'Invalid FFmpeg staging folder' }
New-Item -ItemType Directory -Path $stage -Force | Out-Null
try {
  Set-Content -LiteralPath (Join-Path $stage 'phase.txt') -Value 'Finding release' -NoNewline
  $base = 'https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip'
  $archive = Join-Path $stage 'ffmpeg.zip'
  try {
    $release = Invoke-RestMethod -Uri 'https://api.github.com/repos/GyanD/codexffmpeg/releases/latest' -Headers @{ 'User-Agent' = 'Littleworld' } -TimeoutSec 30
    $asset = @($release.assets | Where-Object { $_.name -match 'essentials_build\.zip$' }) | Select-Object -First 1
    if (-not $asset -or -not $asset.digest -or $asset.digest -notmatch '^sha256:[0-9a-fA-F]{64}$') { throw 'The verified FFmpeg release archive is unavailable' }
    $checksum = $asset.digest.Substring(7)
    Set-Content -LiteralPath (Join-Path $stage 'total.txt') -Value ([string]$asset.size) -NoNewline
    Set-Content -LiteralPath (Join-Path $stage 'phase.txt') -Value 'Downloading' -NoNewline
    Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $archive -UseBasicParsing -TimeoutSec 1200
  } catch {
    Remove-Item -LiteralPath $archive -Force -ErrorAction SilentlyContinue
    Set-Content -LiteralPath (Join-Path $stage 'phase.txt') -Value 'Trying another source' -NoNewline
    $checksum = (Invoke-WebRequest -Uri ($base + '.sha256') -UseBasicParsing -TimeoutSec 30).Content.Trim()
    Remove-Item -LiteralPath (Join-Path $stage 'total.txt') -Force -ErrorAction SilentlyContinue
    try {
      $head = Invoke-WebRequest -Uri $base -Method Head -UseBasicParsing -TimeoutSec 15
      $length = [long]$head.Headers['Content-Length']
      if ($length -gt 0) { Set-Content -LiteralPath (Join-Path $stage 'total.txt') -Value ([string]$length) -NoNewline }
    } catch { }
    Set-Content -LiteralPath (Join-Path $stage 'phase.txt') -Value 'Downloading' -NoNewline
    Invoke-WebRequest -Uri $base -OutFile $archive -UseBasicParsing -TimeoutSec 1200
  }
  Set-Content -LiteralPath (Join-Path $stage 'phase.txt') -Value 'Verifying download' -NoNewline
  if ($checksum -notmatch '^[0-9a-fA-F]{64}$') { throw 'Invalid download checksum' }
  if ((Get-Item -LiteralPath $archive).Length -gt 600MB) { throw 'Archive is too large' }
  if ((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash -ne $checksum) { throw 'Download checksum did not match' }
  Add-Type -AssemblyName System.IO.Compression.FileSystem
  $zip = [System.IO.Compression.ZipFile]::OpenRead($archive)
  try {
    Set-Content -LiteralPath (Join-Path $stage 'phase.txt') -Value 'Preparing tools' -NoNewline
    foreach ($name in @('ffmpeg.exe', 'ffprobe.exe')) {
      $entries = @($zip.Entries | Where-Object { $_.FullName -match ('^[^/\\]+/bin/' + [regex]::Escape($name) + '$') })
      if ($entries.Count -ne 1) { throw ('Archive is missing ' + $name) }
      $target = Join-Path $stage $name
      [System.IO.Compression.ZipFileExtensions]::ExtractToFile($entries[0], $target)
    }
  } finally { $zip.Dispose() }
  & (Join-Path $stage 'ffmpeg.exe') -version | Out-Null
  if ($LASTEXITCODE -ne 0) { throw 'FFmpeg could not start' }
  Set-Content -LiteralPath (Join-Path $stage 'phase.txt') -Value 'Installing' -NoNewline
  $current = Join-Path $root 'ffmpeg.exe'
  $currentProbe = Join-Path $root 'ffprobe.exe'
  if (Test-Path -LiteralPath $current) { Copy-Item -LiteralPath $current -Destination (Join-Path $stage 'previous.exe') }
  if (Test-Path -LiteralPath $currentProbe) { Copy-Item -LiteralPath $currentProbe -Destination (Join-Path $stage 'previous-probe.exe') }
  try {
    Copy-Item -LiteralPath (Join-Path $stage 'ffprobe.exe') -Destination $currentProbe -Force
    Copy-Item -LiteralPath (Join-Path $stage 'ffmpeg.exe') -Destination $current -Force
  } catch {
    if (Test-Path -LiteralPath (Join-Path $stage 'previous.exe')) { Copy-Item -LiteralPath (Join-Path $stage 'previous.exe') -Destination $current -Force }
    if (Test-Path -LiteralPath (Join-Path $stage 'previous-probe.exe')) { Copy-Item -LiteralPath (Join-Path $stage 'previous-probe.exe') -Destination $currentProbe -Force }
    throw
  }
} finally {
  if ([IO.Path]::GetFullPath($stage).StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)) {
    Remove-Item -LiteralPath $stage -Recurse -Force -ErrorAction SilentlyContinue
  }
}

} catch {
  [Console]::Error.WriteLine($_.Exception.Message)
  exit 1
}
"#;
    let mut command = Command::new("powershell.exe");
    command.args(["-NoProfile", "-NonInteractive", "-Command", SCRIPT]);
    command.env("ISLAND_FFMPEG_STAGE", &stage);
    #[cfg(windows)] {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped())
        .spawn().map_err(|_| "PowerShell could not start FFmpeg setup.")?;
    let stderr = child.stderr.take().expect("piped installer stderr");
    let errors = std::thread::spawn(move || {
        let mut output = String::new();
        let _ = BufReader::new(stderr).take(4096).read_to_string(&mut output);
        output
    });
    let start = Instant::now();
    let mut last_sample = start - Duration::from_secs(1);
    let mut last_progress = start;
    let mut last_bytes = 0;
    let mut last_phase = String::new();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let detail = errors.join().unwrap_or_default();
                let _ = fs::remove_dir_all(&stage);
                if status.success() && destination.is_file() { return Ok(destination); }
                let detail = detail.lines().find(|line| !line.trim().is_empty()).unwrap_or("").trim();
                return Err(if detail.is_empty() { "FFmpeg setup failed. Check your connection and disk space, then retry.".into() }
                    else { format!("FFmpeg setup failed: {detail}") });
            }
            Ok(None) => {
                if last_sample.elapsed() >= Duration::from_millis(500) {
                    last_sample = Instant::now();
                    let bytes = fs::metadata(stage.join("ffmpeg.zip")).map(|m| m.len()).unwrap_or(0);
                    let total = fs::read_to_string(stage.join("total.txt")).ok()
                        .and_then(|s| s.trim().parse().ok()).unwrap_or(0);
                    let phase = fs::read_to_string(stage.join("phase.txt")).unwrap_or_else(|_| "Connecting".into());
                    if bytes != last_bytes || phase != last_phase {
                        last_progress = Instant::now();
                        last_bytes = bytes;
                        last_phase = phase.clone();
                    }
                    let _ = sender.try_send(Event::EngineProgress(phase, bytes, total));
                }
                if start.elapsed() >= Duration::from_secs(45 * 60) || last_progress.elapsed() >= Duration::from_secs(4 * 60) {
                    let _ = child.kill(); let _ = child.wait(); let _ = errors.join();
                    let _ = fs::remove_dir_all(&stage);
                    return Err("FFmpeg download stopped making progress. Check your connection and retry.".into());
                }
                std::thread::sleep(Duration::from_millis(250));
            }
            Err(error) => {
                let _ = child.kill(); let _ = child.wait(); let _ = errors.join();
                let _ = fs::remove_dir_all(&stage);
                return Err(format!("FFmpeg setup stopped: {error}"));
            }
        }
    }
}

fn record_install_result(result: &Result<PathBuf, String>) {
    let Some(root) = managed_engine().and_then(|path| path.parent().map(Path::to_path_buf)) else { return; };
    if fs::create_dir_all(&root).is_err() { return; }
    let time = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    let detail = match result { Ok(_) => "FFmpeg installed successfully", Err(error) => error.as_str() };
    let _ = fs::write(root.join("install-status.log"), format!("Unix time: {time}\n{detail}\n"));
}

fn record_update_check(installed: &str, latest: &Result<String, String>) {
    let Some(root) = managed_engine().and_then(|path| path.parent().map(Path::to_path_buf)) else { return; };
    if fs::create_dir_all(&root).is_err() { return; }
    let time = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    let detail = match latest {
        Ok(version) => format!("Latest version: {version}"),
        Err(error) => format!("Lookup failed: {error}"),
    };
    let _ = fs::write(root.join("update-check.log"), format!("Unix time: {time}\nInstalled version: {installed}\n{detail}\n"));
}

impl Default for Converter {
    fn default() -> Self {
        let (sender, receiver) = mpsc::sync_channel(64);
        let engine = config_path().and_then(|p| fs::read_to_string(p).ok())
            .map(|s| PathBuf::from(s.trim())).filter(|p| p.is_absolute() && p.is_file());
        let mut converter = Self { state: State::default(), sender, receiver, cancel: Arc::new(AtomicBool::new(false)), engine, next_id: 1,
            last_version_check: Instant::now() - Duration::from_secs(6 * 60 * 60), reinspect_after_finish: false, pending_additions: Vec::new() };
        converter.state.destination = output_config_path().and_then(|p| fs::read_to_string(p).ok())
            .map(|s| s.trim().to_owned()).filter(|s| Path::new(s).is_absolute() && Path::new(s).is_dir()).unwrap_or_default();
        converter.refresh_engine();
        converter
    }
}

impl Drop for Converter {
    fn drop(&mut self) { self.cancel.store(true, Ordering::Release); }
}

impl Converter {
    pub fn add(&mut self, paths: Vec<PathBuf>) {
        let room = MAX_FILES.saturating_sub(self.state.items.len());
        let candidates: Vec<_> = paths.into_iter().filter(|p| p.is_absolute()
            && !self.state.items.iter().any(|i| i.path == *p)).take(room).collect();
        if candidates.is_empty() {
            if room == 0 { self.state.notice = "This batch is full.".into(); }
            else { self.state.notice.clear(); }
            return;
        }
        if self.state.busy {
            for path in candidates {
                if !self.pending_additions.contains(&path) { self.pending_additions.push(path); }
            }
            self.state.notice = "Added to the queue. Files will load shortly.".into();
            return;
        }
        self.state.notice.clear();
        let mut pending = Vec::new();
        for path in candidates {
            if pending.iter().any(|i: &Item| i.path == path) { continue; }
            let item = Item { id: self.next_id, name: path.file_name().unwrap_or_default().to_string_lossy().into(),
                path, kind: "file".into(), bytes: 0, width: 0, height: 0, duration: 0.,
                status: "inspecting".into(), message: String::new(), progress: None, formats: vec![],
                output_bytes: 0, output: None };
            self.next_id += 1;
            pending.push(item);
        }
        self.state.items.extend(pending.clone());
        self.state.busy = true;
        self.state.inspecting = true;
        self.cancel = Arc::new(AtomicBool::new(false));
        let cancel = self.cancel.clone();
        let sender = self.sender.clone();
        let configured = self.engine.clone();
        std::thread::spawn(move || {
            let engine = find_engine(configured);
            let formats = engine.as_ref().map(|p| media_formats(p, &cancel)).unwrap_or_default();
            for item in &mut pending {
                if cancel.load(Ordering::Acquire) { item.status = "cancelled".into(); continue; }
                inspect(item, engine.as_deref(), &formats, &cancel);
            }
            let _ = sender.send(Event::Inspected(pending, !formats.is_empty()));
            let _ = sender.send(Event::Finished);
        });
    }

    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        if self.state.engine_source != "missing" && !self.state.engine_busy && !self.state.checking_update
            && self.last_version_check.elapsed() >= Duration::from_secs(6 * 60 * 60) {
            self.check_engine_update();
            changed = true;
        }
        while let Ok(event) = self.receiver.try_recv() {
            changed = true;
            match event {
                Event::Inspected(items, available) => {
                    self.state.media_available = available;
                    for item in items { self.replace(item); }
                }
                Event::Item(item) => self.replace(item),
                Event::Finished => { self.state.busy = false; self.state.inspecting = false; if self.reinspect_after_finish { self.reinspect_after_finish = false; self.reinspect(); } }
                Event::EngineProgress(phase, downloaded, total) => {
                    self.state.engine_phase = phase;
                    self.state.engine_downloaded_bytes = downloaded;
                    self.state.engine_total_bytes = total;
                }
                Event::EngineInstalled(result) => {
                    self.state.engine_busy = false;
                    self.state.engine_phase.clear();
                    self.state.engine_downloaded_bytes = 0;
                    self.state.engine_total_bytes = 0;
                    match result {
                        Ok(path) => {
                            self.engine = Some(path.clone());
                            if let Some(config) = config_path() {
                                if let Some(parent) = config.parent() { let _ = fs::create_dir_all(parent); }
                                let _ = fs::write(config, path.to_string_lossy().as_bytes());
                            }
                            if self.state.busy { self.reinspect_after_finish = true; } else { self.reinspect(); }
                            self.state.engine_notice = "FFmpeg is ready for video and audio.".into();
                        }
                        Err(error) => self.state.engine_notice = error,
                    }
                    self.refresh_engine();
                    self.state.update_available = false;
                    self.state.latest_version.clear();
                    self.last_version_check = Instant::now() - Duration::from_secs(6 * 60 * 60);
                }
                Event::EngineVersion(path, installed, latest) => {
                    if self.state.engine_path == path.to_string_lossy() {
                        self.state.installed_version = installed;
                        match latest {
                            Ok(version) => {
                                self.state.latest_version = version;
                                self.state.update_available = newer_engine_version(&self.state.latest_version, &self.state.installed_version);
                                self.state.engine_notice = if self.state.installed_version.is_empty() { "Could not read the installed FFmpeg version." }
                                    else if self.state.update_available { "An FFmpeg update is ready." }
                                    else { "FFmpeg is up to date." }.into();
                            }
                            Err(_) => {
                                self.state.latest_version.clear();
                                self.state.update_available = false;
                                self.state.engine_notice = "Could not reach FFmpeg release servers. Check your connection and try again.".into();
                            }
                        }
                    }
                    self.state.checking_update = false;
                }
            }
        }
        if !self.state.busy && !self.pending_additions.is_empty() {
            let paths = std::mem::take(&mut self.pending_additions);
            self.add(paths);
            changed = true;
        }
        changed
    }

    fn replace(&mut self, item: Item) {
        if let Some(slot) = self.state.items.iter_mut().find(|i| i.id == item.id) { *slot = item; }
    }

    pub fn remove(&mut self, id: u64) {
        if !self.state.busy { self.state.items.retain(|i| i.id != id); }
    }
    pub fn cancel(&mut self) { self.cancel.store(true, Ordering::Release); }
    pub fn destination(&mut self, path: String) {
        if self.state.busy { return; }
        if !path.is_empty() && (!Path::new(&path).is_absolute() || !Path::new(&path).is_dir()) {
            self.state.destination_notice = "Choose an existing output folder.".into(); return;
        }
        self.state.destination = path;
        self.state.destination_notice = if self.state.destination.is_empty() { "Saving beside original files." } else { "Output folder saved." }.into();
        if let Some(config) = output_config_path() {
            if self.state.destination.is_empty() {
                if let Err(error) = fs::remove_file(config) {
                    if error.kind() != std::io::ErrorKind::NotFound { self.state.destination_notice = "Could not clear the saved output folder.".into(); }
                }
            }
            else {
                if let Some(parent) = config.parent() { let _ = fs::create_dir_all(parent); }
                if fs::write(config, &self.state.destination).is_err() {
                    self.state.destination_notice = "Output folder is set for this session, but could not be saved.".into();
                }
            }
        }
    }
    pub fn engine(&mut self, path: PathBuf) {
        if self.state.busy || self.state.engine_busy { return; }
        // Executables are selected explicitly in a native picker, never supplied by media.
        if !path.is_absolute() || !path.is_file() || path.file_name().is_none_or(|s| !s.to_string_lossy().eq_ignore_ascii_case("ffmpeg.exe")) {
            self.state.engine_notice = "Choose ffmpeg.exe from a local FFmpeg installation.".into(); return;
        }
        self.engine = Some(path.clone());
        if let Some(config) = config_path() {
            if let Some(parent) = config.parent() { let _ = fs::create_dir_all(parent); }
            let _ = fs::write(config, path.to_string_lossy().as_bytes());
        }
        let paths = self.state.items.iter().map(|i| i.path.clone()).collect();
        self.state.items.clear();
        self.add(paths);
        if self.state.items.is_empty() { self.state.notice = "Media engine selected. Add files to inspect its capabilities.".into(); }
        self.refresh_engine();
    }

    fn refresh_engine(&mut self) {
        let previous_path = self.state.engine_path.clone();
        let found = find_engine(self.engine.clone());
        self.state.engine_path = found.as_ref().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
        self.state.engine_source = match found {
            Some(ref path) if managed_engine().as_ref() == Some(path) => "managed",
            Some(ref path) if self.engine.as_ref() == Some(path) => "selected",
            Some(_) => "system",
            None => "missing",
        }.into();
        if self.state.engine_path != previous_path || self.state.engine_source == "missing" {
            self.state.update_available = false;
            self.state.latest_version.clear();
            self.state.installed_version.clear();
            self.last_version_check = Instant::now() - Duration::from_secs(6 * 60 * 60);
        }
    }

    pub fn check_engine_update(&mut self) {
        if self.state.engine_source == "missing" || self.state.engine_busy || self.state.checking_update { return; }
        let path = PathBuf::from(&self.state.engine_path);
        if !path.is_file() { return; }
        self.state.checking_update = true;
        self.state.engine_notice = "Checking FFmpeg releases…".into();
        self.last_version_check = Instant::now();
        let sender = self.sender.clone();
        std::thread::spawn(move || {
            let installed = installed_engine_version(&path).unwrap_or_default();
            let latest = latest_engine_version();
            record_update_check(&installed, &latest);
            let _ = sender.send(Event::EngineVersion(path, installed, latest));
        });
    }

    pub fn install_engine(&mut self) {
        if self.state.busy || self.state.engine_busy { return; }
        self.state.engine_busy = true;
        self.state.checking_update = false;
        self.state.engine_phase = "Connecting".into();
        self.state.engine_downloaded_bytes = 0;
        self.state.engine_total_bytes = 0;
        self.state.engine_notice = "Getting the verified FFmpeg build…".into();
        let sender = self.sender.clone();
        std::thread::spawn(move || {
            let result = install_managed_engine(&sender);
            record_install_result(&result);
            let _ = sender.send(Event::EngineInstalled(result));
        });
    }

    pub fn remove_engine(&mut self) {
        if self.state.busy || self.state.engine_busy { return; }
        if self.state.engine_source != "managed" {
            self.state.engine_notice = "Only the app-managed FFmpeg copy can be removed here.".into(); return;
        }
        let Some(path) = managed_engine() else { return; };
        if let Err(error) = fs::remove_file(&path) {
            self.state.engine_notice = format!("Could not remove FFmpeg: {error}"); return;
        }
        if let Some(probe) = path.parent().map(|parent| parent.join("ffprobe.exe")) { let _ = fs::remove_file(probe); }
        if self.engine.as_ref() == Some(&path) {
            self.engine = None;
            if let Some(config) = config_path() { let _ = fs::remove_file(config); }
        }
        self.refresh_engine();
        self.state.media_available = false;
        self.state.engine_notice = "The app-managed FFmpeg copy was removed.".into();
        self.reinspect();
    }
    pub fn forget_engine(&mut self) {
        if self.state.busy || self.state.engine_busy { return; }
        self.engine = None;
        if let Some(config) = config_path() { let _ = fs::remove_file(config); }
        self.refresh_engine();
        self.state.engine_notice = "Custom FFmpeg location was forgotten.".into();
        self.reinspect();
    }
    fn reinspect(&mut self) {
        let paths: Vec<_> = self.state.items.iter().filter(|item| item.status != "complete").map(|item| item.path.clone()).collect();
        self.state.items.retain(|item| item.status == "complete");
        if !paths.is_empty() { self.add(paths); }
    }
    pub fn output(&self, id: u64) -> Option<PathBuf> {
        self.state.items.iter().find(|i| i.id == id).and_then(|i| i.output.clone())
    }

    pub fn start(&mut self, profile: Profile, ids: &[u64]) {
        if self.state.busy || self.state.engine_busy { return; }
        if profile.width > 12000 || profile.height > 12000 || (profile.max_bytes > 0 && profile.max_bytes < 1000) {
            self.state.notice = "Use dimensions up to 12,000 px and a size limit of at least 1 KB.".into(); return;
        }
        let mut jobs = Vec::new();
        for item in &mut self.state.items {
            if !ids.contains(&item.id) || !item.formats.contains(&profile.format) || item.status == "complete" { continue; }
            item.status = "queued".into(); item.message.clear(); item.progress = None;
            jobs.push(item.clone());
        }
        if jobs.is_empty() { self.state.notice = "Choose a supported output for the selected files.".into(); return; }
        self.state.busy = true;
        self.state.notice.clear();
        self.cancel = Arc::new(AtomicBool::new(false));
        let cancel = self.cancel.clone();
        let sender = self.sender.clone();
        let destination = self.state.destination.clone();
        let engine = find_engine(self.engine.clone());
        std::thread::spawn(move || {
            for mut item in jobs {
                if cancel.load(Ordering::Acquire) {
                    item.status = "cancelled".into();
                    let _ = sender.send(Event::Item(item)); continue;
                }
                item.status = "running".into(); item.message = "Preparing".into();
                let _ = sender.send(Event::Item(item.clone()));
                let result = convert(&item, &profile, &destination, engine.as_deref(), &cancel, &sender);
                match result {
                    Ok(path) => {
                        item.output_bytes = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                        item.output = Some(path); item.status = "complete".into(); item.message = "Saved".into(); item.progress = Some(1.);
                    }
                    Err(reason) => {
                        item.status = if cancel.load(Ordering::Acquire) { "cancelled" } else { "failed" }.into();
                        item.message = reason; item.progress = None;
                    }
                }
                let _ = sender.send(Event::Item(item));
            }
            let _ = sender.send(Event::Finished);
        });
    }
}

pub fn could_convert(path: &Path) -> bool {
    // This is only the cheap drag-time hint; content inspection owns acceptance.
    !matches!(path.extension().and_then(|s| s.to_str()).unwrap_or("").to_ascii_lowercase().as_str(),
        "exe" | "lnk" | "url" | "msi" | "bat" | "cmd" | "ps1" | "appref-ms")
}

fn find_engine(configured: Option<PathBuf>) -> Option<PathBuf> {
    configured.filter(|p| p.is_file()).or_else(|| managed_engine().filter(|p| p.is_file())).or_else(|| {
        std::env::var_os("PATH").and_then(|paths| std::env::split_paths(&paths)
            .map(|p| p.join("ffmpeg.exe")).find(|p| p.is_absolute() && p.is_file()))
    })
}

fn installed_engine_version(path: &Path) -> Option<String> {
    let output = process(path).arg("-version").output().ok()?;
    if !output.status.success() { return None; }
    let first = String::from_utf8_lossy(&output.stdout).lines().next()?.to_owned();
    let version = first.split_whitespace().nth(2)?.split('-').next()?;
    valid_engine_version(version).then(|| version.to_owned())
}

fn latest_engine_version() -> Result<String, String> {
    const SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
$version = $null
$gyanError = 'no valid version returned'
$githubError = 'not tried'
try {
  $candidate = (Invoke-RestMethod -Uri 'https://www.gyan.dev/ffmpeg/builds/release-version' -TimeoutSec 15).ToString().Trim()
  if ($candidate -match '^[0-9]+\.[0-9]+\.[0-9]+$') { $version = $candidate }
  else { $gyanError = 'invalid version response' }
} catch { $gyanError = $_.Exception.Message }
if (-not $version) {
  try {
    $release = Invoke-RestMethod -Uri 'https://api.github.com/repos/GyanD/codexffmpeg/releases/latest' -Headers @{ 'User-Agent' = 'Littleworld' } -TimeoutSec 20
    $candidate = ([string]$release.tag_name).Trim() -replace '^[vV]', ''
    if ($candidate -match '^[0-9]+\.[0-9]+\.[0-9]+$') { $version = $candidate }
    else { $githubError = 'invalid version response' }
  } catch { $githubError = $_.Exception.Message }
}
if (-not $version) {
  [Console]::Error.WriteLine("Gyan: $gyanError; GitHub: $githubError")
  exit 1
}
[Console]::Out.Write($version)
"#;
    let mut command = Command::new("powershell.exe");
    command.args(["-NoProfile", "-NonInteractive", "-Command", SCRIPT]);
    #[cfg(windows)] {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let output = command.stdin(Stdio::null()).output().map_err(|error| format!("Could not start PowerShell: {error}"))?;
    let version = String::from_utf8_lossy(&output.stdout).trim().trim_start_matches('\u{feff}').to_owned();
    if output.status.success() && valid_engine_version(&version) {
        Ok(version)
    } else {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        Err(if detail.is_empty() { "Both release sources failed or returned an invalid version.".into() } else { detail.chars().take(500).collect() })
    }
}

fn valid_engine_version(value: &str) -> bool {
    value.split('.').count() == 3 && value.split('.').all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
}

fn newer_engine_version(latest: &str, installed: &str) -> bool {
    let parts = |value: &str| -> Option<Vec<u32>> {
        valid_engine_version(value).then(|| value.split('.').map(str::parse).collect::<Result<Vec<u32>, _>>().ok()).flatten()
    };
    matches!((parts(latest), parts(installed)), (Some(new), Some(old)) if new > old)
}

fn process(exe: &Path) -> Command {
    let mut command = Command::new(exe);
    #[cfg(windows)] {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    command.stdin(Stdio::null());
    command
}

fn capture(exe: &Path, args: &[&str], cancel: &AtomicBool) -> Result<Vec<u8>, String> {
    let mut child = process(exe).args(args).stdout(Stdio::piped()).stderr(Stdio::null()).spawn()
        .map_err(|_| "Media support unavailable".to_owned())?;
    let stdout = child.stdout.take().unwrap();
    let reader = std::thread::spawn(move || {
        let mut out = Vec::new();
        let mut reader = BufReader::new(stdout);
        let _ = reader.by_ref().take(256 * 1024).read_to_end(&mut out);
        let _ = std::io::copy(&mut reader, &mut std::io::sink());
        out
    });
    let started = Instant::now();
    let success = loop {
        if cancel.load(Ordering::Acquire) || started.elapsed() > Duration::from_secs(15) {
            let _ = child.kill(); let _ = child.wait(); break false;
        }
        match child.try_wait() {
            Ok(Some(status)) => break status.success(),
            Ok(None) => std::thread::sleep(Duration::from_millis(40)),
            Err(_) => { let _ = child.kill(); let _ = child.wait(); break false; }
        }
    };
    let output = reader.join().unwrap_or_default();
    if success { Ok(output) } else { Err("Can’t inspect this media file".into()) }
}

fn media_formats(engine: &Path, cancel: &AtomicBool) -> Vec<String> {
    if !engine.with_file_name("ffprobe.exe").is_file() { return vec![]; }
    let encoders = capture(engine, &["-hide_banner", "-encoders"], cancel).unwrap_or_default();
    let encoders = String::from_utf8_lossy(&encoders);
    let muxers = capture(engine, &["-hide_banner", "-muxers"], cancel).unwrap_or_default();
    let muxers = String::from_utf8_lossy(&muxers);
    let has_encoder = |encoder: &str| encoders.split_whitespace().any(|v| v == encoder);
    let has_muxer = |muxer: &str| muxers.split_whitespace().any(|v| v.split(',').any(|m| m == muxer));
    // Only advertise an output when this exact local FFmpeg build has the
    // encoder pair and container needed to write it. This keeps the picker
    // truthful for system FFmpeg installations with different build options.
    [
        ("mp4", "libx264", Some("aac"), "mp4"),
        ("mov", "libx264", Some("aac"), "mov"),
        ("mkv", "libx264", Some("aac"), "matroska"),
        ("webm", "libvpx-vp9", Some("libopus"), "webm"),
        ("avi", "mpeg4", Some("libmp3lame"), "avi"),
        ("wmv", "wmv2", Some("wmav2"), "asf"),
        ("gif", "gif", None, "gif"),
        ("mp3", "libmp3lame", None, "mp3"),
        ("m4a", "aac", None, "ipod"),
        ("aac", "aac", None, "adts"),
        ("wav", "pcm_s16le", None, "wav"),
        ("flac", "flac", None, "flac"),
        ("ogg", "libvorbis", None, "ogg"),
        ("opus", "libopus", None, "opus"),
        ("aiff", "pcm_s16be", None, "aiff"),
        ("wma", "wmav2", None, "asf"),
    ].into_iter().filter(|(_, encoder, paired, muxer)| has_encoder(encoder)
        && paired.map(|paired| has_encoder(paired)).unwrap_or(true) && has_muxer(muxer))
        .map(|(format, _, _, _)| format.to_owned()).collect()
}

fn inspect(item: &mut Item, engine: Option<&Path>, formats: &[String], cancel: &AtomicBool) {
    let result = (|| -> Result<(), String> {
        let metadata = fs::metadata(&item.path).map_err(|_| "Can’t read this file")?;
        if !metadata.is_file() || !could_convert(&item.path) { return Err("Use Shelf for this item".into()); }
        item.bytes = metadata.len();
        let extension = item.path.extension().and_then(|s| s.to_str()).unwrap_or("").to_ascii_lowercase();
        if matches!(extension.as_str(), "mp3" | "wav" | "flac" | "m4a" | "aac" | "ogg" | "oga" | "opus" | "wma" | "aif" | "aiff" | "m4b" | "ac3" | "amr" | "ape" | "mid" | "midi" | "mka") {
            // Keep audio visible as audio even if FFmpeg is missing or cannot
            // inspect this particular file, so the UI never leaves music under
            // a stale video format selection.
            item.kind = "audio".into();
        }
        if matches!(extension.as_str(), "srt" | "vtt") {
            if item.bytes > MAX_SUBTITLE_BYTES { return Err("Subtitle file exceeds the 16 MB text limit".into()); }
            let mut source = String::new();
            File::open(&item.path).and_then(|mut file| file.read_to_string(&mut source)).map_err(|_| "Use a UTF-8 SRT or WebVTT subtitle file")?;
            if !source.lines().any(|line| line.contains("-->")) { return Err("No subtitle cues were found".into()); }
            item.kind = "subtitle".into();
            item.formats = if extension == "srt" { vec!["vtt".into(), "txt".into()] } else { vec!["srt".into(), "txt".into()] };
            return Ok(());
        }
        if let Ok(reader) = image::ImageReader::open(&item.path).and_then(|r| r.with_guessed_format()) {
            if let Some(format) = reader.format() {
                // Animated/multipage inputs must not silently lose frames/pages.
                if matches!(format, ImageFormat::Gif | ImageFormat::Tiff) {
                    return Err("Animated and multipage images are not available yet".into());
                }
                let decoder = reader.into_decoder().map_err(|_| "Can’t read this image")?;
                if decoder.dimensions().0 as u64 * decoder.dimensions().1 as u64 > MAX_PIXELS {
                    return Err("Image exceeds the 32 megapixel memory limit".into());
                }
                if format == ImageFormat::WebP {
                    // Inspect the VP8X animation flag without allocating a frame.
                    let mut header = [0; 21];
                    if File::open(&item.path).and_then(|mut f| f.read_exact(&mut header)).is_ok()
                        && &header[12..16] == b"VP8X" && header[20] & 2 != 0 {
                        return Err("Animated images are not available yet".into());
                    }
                }
                if format == ImageFormat::Png {
                    let decoder = png::Decoder::new(BufReader::new(File::open(&item.path).map_err(|_| "Can’t read this image")?));
                    if decoder.read_info().map(|r| r.info().animation_control.is_some()).unwrap_or(false) {
                        return Err("Animated images are not available yet".into());
                    }
                }
                (item.width, item.height) = decoder.dimensions();
                item.kind = "image".into();
                item.formats = ["png", "jpg", "webp", "bmp", "tiff", "gif"].map(String::from).to_vec();
                return Ok(());
            }
        }
        let engine = engine.ok_or("Select a local FFmpeg installation for video and audio")?;
        let probe = engine.with_file_name("ffprobe.exe");
        let data = capture(&probe, &["-v", "error", "-protocol_whitelist", "file,pipe", "-show_entries",
            "format=duration:stream=codec_type,width,height:stream_disposition=attached_pic", "-of", "json", &item.path.to_string_lossy()], cancel)?;
        let value: serde_json::Value = serde_json::from_slice(&data).map_err(|_| "Can’t read this media file")?;
        let streams = value["streams"].as_array().ok_or("No supported media streams")?;
        // Audio containers commonly include cover art as an attached-picture
        // video stream. It is artwork, not a playable video track, and must not
        // route the music file into the video format list.
        let video_streams = streams.iter().filter(|s| s["codec_type"] == "video"
            && s["disposition"]["attached_pic"].as_u64() != Some(1)).collect::<Vec<_>>();
        let audio_streams = streams.iter().filter(|s| s["codec_type"] == "audio").collect::<Vec<_>>();
        if video_streams.len() > 1
            || audio_streams.len() > 1
            || streams.iter().any(|s| !matches!(s["codec_type"].as_str(), Some("video" | "audio"))) {
            return Err("Multiple tracks or subtitles need a dedicated media editor".into());
        }
        if let Some(video) = video_streams.first() {
            item.kind = "video".into();
            item.width = video["width"].as_u64().unwrap_or(0) as u32;
            item.height = video["height"].as_u64().unwrap_or(0) as u32;
            if item.width == 0 || item.height == 0 || item.width as u64 * item.height as u64 > MAX_PIXELS {
                return Err("Video exceeds the supported frame size".into());
            }
            item.formats = formats.iter().filter(|f| matches!(f.as_str(), "mp4" | "mov" | "mkv" | "webm" | "avi" | "wmv" | "gif")).cloned().collect();
        } else if !audio_streams.is_empty() {
            item.kind = "audio".into();
            item.formats = formats.iter().filter(|f| matches!(f.as_str(), "mp3" | "m4a" | "aac" | "wav" | "flac" | "ogg" | "opus" | "aiff" | "wma")).cloned().collect();
        } else { return Err("No supported media streams".into()); }
        item.duration = value["format"]["duration"].as_str().and_then(|v| v.parse().ok()).unwrap_or(0.);
        if item.formats.is_empty() { return Err("No compatible output encoder is installed".into()); }
        Ok(())
    })();
    match result {
        Ok(()) => { item.status = "ready".into(); item.message.clear(); }
        Err(message) => { item.status = "unsupported".into(); item.message = message; }
    }
}

struct Temporary(PathBuf);
impl Drop for Temporary { fn drop(&mut self) { let _ = fs::remove_file(&self.0); } }

fn convert(item: &Item, profile: &Profile, destination: &str, engine: Option<&Path>, cancel: &AtomicBool, sender: &SyncSender<Event>) -> Result<PathBuf, String> {
    let category = match item.kind.as_str() { "image" => "Images", "video" => "Videos", "audio" => "Audio", "subtitle" => "Subtitles", _ => return Err("Unsupported file type".into()) };
    let folder;
    let directory = if destination.is_empty() { item.path.parent().ok_or("Choose an output folder")? } else {
        if !Path::new(destination).is_dir() { return Err("Output folder is unavailable. Choose another folder in Settings.".into()); }
        folder = Path::new(destination).join(category);
        fs::create_dir_all(&folder).map_err(|_| "Couldn’t create the output folder. Check its permissions.")?;
        &folder
    };
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
    let temporary = Temporary(directory.join(format!(".island-{}-{stamp}.{}", item.id, profile.format)));
    OpenOptions::new().write(true).create_new(true).open(&temporary.0).map_err(|_| "Couldn’t save output. Choose another folder.")?;
    match item.kind.as_str() {
        "image" => convert_image(item, profile, &temporary.0, cancel)?,
        "subtitle" => convert_subtitle(item, &profile.format, &temporary.0)?,
        _ => convert_media(item, profile, &temporary.0, engine.ok_or("Media support unavailable")?, cancel, sender)?,
    }
    if cancel.load(Ordering::Acquire) { return Err("Cancelled".into()); }
    let size = fs::metadata(&temporary.0).map_err(|_| "Couldn’t finalize output")?.len();
    if size == 0 { return Err("The encoder produced an empty file".into()); }
    if profile.max_bytes > 0 && size > profile.max_bytes { return Err(format!("Size limit not reached · {} KB. Allow smaller dimensions or raise the limit.", size.div_ceil(1000))); }
    let stem = item.path.file_stem().unwrap_or_default().to_string_lossy();
    // Hard links publish only to a new name, atomically, without overwriting an existing file.
    for index in 0..1000 {
        let suffix = if index == 0 { String::new() } else { format!("-{index}") };
        let output = directory.join(format!("{stem}-converted{suffix}.{}", profile.format));
        match fs::hard_link(&temporary.0, &output) {
            Ok(()) => return Ok(output),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => {
                // FAT/external volumes may not support hard links. Reserve the name first.
                match OpenOptions::new().write(true).create_new(true).open(&output) {
                    Ok(mut out) => {
                        let result = File::open(&temporary.0).and_then(|mut input| std::io::copy(&mut input, &mut out)).and_then(|_| out.sync_all());
                        if result.is_err() { drop(out); let _ = fs::remove_file(&output); return Err("Couldn’t save output. Check free space.".into()); }
                        return Ok(output);
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                    Err(_) => return Err("Couldn’t save output. Choose another folder.".into()),
                }
            }
        }
    }
    Err("Choose a folder with fewer matching output names".into())
}

fn convert_subtitle(item: &Item, format: &str, output: &Path) -> Result<(), String> {
    let mut source = String::new();
    File::open(&item.path).and_then(|mut file| file.read_to_string(&mut source)).map_err(|_| "Use a UTF-8 SRT or WebVTT subtitle file")?;
    if source.len() as u64 > MAX_SUBTITLE_BYTES { return Err("Subtitle file exceeds the 16 MB text limit".into()); }
    let source = source.trim_start_matches('\u{feff}');
    let lines: Vec<&str> = source.lines().collect();
    let input_vtt = item.path.extension().and_then(|s| s.to_str()).is_some_and(|s| s.eq_ignore_ascii_case("vtt"));
    let mut converted = String::new();
    if format == "txt" {
        for (index, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            let cue_identifier = index + 1 < lines.len() && lines[index + 1].contains("-->");
            if trimmed.is_empty() || trimmed == "WEBVTT" || trimmed.starts_with("NOTE") || trimmed == "STYLE" || trimmed == "REGION"
                || cue_identifier
                || trimmed.contains("-->") { continue; }
            converted.push_str(trimmed);
            converted.push('\n');
        }
    } else if format == "vtt" && !input_vtt {
        converted.push_str("WEBVTT\n\n");
        for (index, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            if index + 1 < lines.len() && lines[index + 1].contains("-->") { continue; }
            if trimmed.contains("-->") { converted.push_str(&trimmed.replace(',', ".")); }
            else { converted.push_str(line); }
            converted.push('\n');
        }
    } else if format == "srt" && input_vtt {
        let mut cue = 0u32;
        let mut in_header = true;
        for (index, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            if in_header {
                if trimmed.is_empty() { in_header = false; }
                continue;
            }
            if trimmed.starts_with("NOTE") || trimmed == "STYLE" || trimmed == "REGION" {
                return Err("This WebVTT file contains metadata blocks that SRT cannot preserve".into());
            }
            if index + 1 < lines.len() && lines[index + 1].contains("-->") { continue; }
            if trimmed.contains("-->") {
                cue += 1;
                let timing = trimmed.split_once(" --> ").map(|(start, end)| format!("{start} --> {}", end.split_whitespace().next().unwrap_or(end).replace('.', ",")))
                    .unwrap_or_else(|| trimmed.replace('.', ","));
                converted.push_str(&format!("{cue}\n{timing}\n"));
            } else { converted.push_str(line); converted.push('\n'); }
        }
    } else { return Err("Choose SRT, WebVTT, or plain text output".into()); }
    if converted.trim().is_empty() { return Err("No subtitle text was available to convert".into()); }
    fs::write(output, converted.as_bytes()).map_err(|_| "Couldn’t save the subtitle output".to_owned())
}

fn encode_image(image: &DynamicImage, format: &str, quality: u8, path: &Path) -> Result<(), String> {
    let mut out = File::create(path).map_err(|_| "Couldn’t save output")?;
    let result = match format {
        "jpg" => image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality).encode_image(&image.to_rgb8()),
        "png" => image.write_to(&mut out, ImageFormat::Png),
        "webp" => image::codecs::webp::WebPEncoder::new_lossless(&mut out).write_image(image.to_rgba8().as_raw(), image.width(), image.height(), image::ExtendedColorType::Rgba8),
        "bmp" => image.write_to(&mut out, ImageFormat::Bmp),
        "tiff" => image.write_to(&mut out, ImageFormat::Tiff),
        "gif" => image.write_to(&mut out, ImageFormat::Gif),
        _ => return Err("Format unavailable".into()),
    };
    result.map_err(|_| "Couldn’t encode this image".into())
}

fn convert_image(item: &Item, profile: &Profile, output: &Path, cancel: &AtomicBool) -> Result<(), String> {
    let mut reader = image::ImageReader::open(&item.path).map_err(|_| "Can’t read this image")?
        .with_guessed_format().map_err(|_| "Can’t read this image")?;
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(192 * 1024 * 1024);
    limits.max_image_width = Some(16000);
    limits.max_image_height = Some(16000);
    reader.limits(limits);
    let mut decoder = reader.into_decoder().map_err(|_| "Image exceeds the decoder limits")?;
    let (w, h) = decoder.dimensions();
    if w as u64 * h as u64 > MAX_PIXELS { return Err("Image exceeds the 32 megapixel memory limit".into()); }
    let orientation = decoder.orientation().unwrap_or(image::metadata::Orientation::NoTransforms);
    let mut picture = DynamicImage::from_decoder(decoder).map_err(|_| "Can’t decode this image")?;
    picture.apply_orientation(orientation);
    if profile.format == "jpg" && picture.color().has_alpha() && picture.to_rgba8().pixels().any(|p| p[3] < 255) {
        return Err("JPEG cannot preserve transparency. Choose PNG or WebP.".into());
    }
    if profile.width > 0 || profile.height > 0 {
        let width = if profile.width == 0 { (picture.width() as u64 * profile.height as u64 / picture.height() as u64).max(1) as u32 } else { profile.width };
        let height = if profile.height == 0 { (picture.height() as u64 * width as u64 / picture.width() as u64).max(1) as u32 } else { profile.height };
        if width as u64 * height as u64 > MAX_PIXELS { return Err("Requested dimensions exceed the memory limit".into()); }
        picture = picture.resize(width, height, image::imageops::FilterType::Lanczos3);
    }
    let quality = if profile.quality == 0 { 88 } else { profile.quality.clamp(35, 100) };
    for scale_pass in 0..6 {
        let attempts: Vec<u8> = if profile.max_bytes > 0 && profile.format == "jpg" {
            [quality, 75, 60, 45, 35].into_iter().filter(|q| *q <= quality).collect()
        } else { vec![quality] };
        for q in attempts {
            if cancel.load(Ordering::Acquire) { return Err("Cancelled".into()); }
            encode_image(&picture, &profile.format, q, output)?;
            let size = fs::metadata(output).map_err(|_| "Couldn’t inspect output")?.len();
            if profile.max_bytes == 0 || size <= profile.max_bytes { return Ok(()); }
        }
        if !profile.allow_smaller || scale_pass == 5 { break; }
        let size = fs::metadata(output).map_err(|_| "Couldn’t inspect output")?.len();
        let ratio = ((profile.max_bytes as f64 / size.max(1) as f64).sqrt() * 0.92).clamp(0.35, 0.85);
        picture = picture.resize((picture.width() as f64 * ratio).max(1.) as u32,
            (picture.height() as f64 * ratio).max(1.) as u32, image::imageops::FilterType::Triangle);
    }
    Err("Size limit not reached. Allow smaller dimensions or increase the limit.".into())
}

fn convert_media(item: &Item, profile: &Profile, output: &Path, engine: &Path, cancel: &AtomicBool, sender: &SyncSender<Event>) -> Result<(), String> {
    if profile.max_bytes > 0 && (!item.duration.is_finite() || item.duration <= 0.) { return Err("Duration is unavailable for this size limit".into()); }
    if profile.max_bytes > 0 && matches!(profile.format.as_str(), "wav" | "flac" | "aiff") { return Err("Choose a compressed format to fit audio to a size limit".into()); }
    for pass in 0..3 {
        let mut cmd = process(engine);
        cmd.args(["-hide_banner", "-nostdin", "-v", "error", "-y", "-protocol_whitelist", "file,pipe", "-threads", "2", "-i"])
            .arg(&item.path).args(["-map_metadata", "-1", "-threads", "2", "-filter_threads", "1", "-progress", "pipe:1", "-nostats"]);
        let bitrate = if profile.max_bytes > 0 { (profile.max_bytes as f64 * 8. * 0.92 * 0.82_f64.powi(pass) / item.duration) as u64 } else { 0 };
        if item.kind == "video" {
            if profile.format == "gif" { cmd.args(["-map", "0:v:0", "-an"]); }
            else { cmd.args(["-map", "0:v:0", "-map", "0:a:0?"]); }
            match profile.format.as_str() {
                "mp4" | "mov" => { cmd.args(["-c:v", "libx264", "-preset", "veryfast", "-pix_fmt", "yuv420p", "-c:a", "aac", "-movflags", "+faststart"]); }
                "mkv" => { cmd.args(["-c:v", "libx264", "-preset", "veryfast", "-pix_fmt", "yuv420p", "-c:a", "aac"]); }
                "webm" => { cmd.args(["-c:v", "libvpx-vp9", "-deadline", "realtime", "-cpu-used", "5", "-c:a", "libopus"]); }
                "avi" => { cmd.args(["-c:v", "mpeg4", "-c:a", "libmp3lame"]); }
                "wmv" => { cmd.args(["-c:v", "wmv2", "-c:a", "wmav2"]); }
                "gif" => { cmd.args(["-c:v", "gif", "-loop", "0"]); }
                _ => return Err("Format unavailable".into()),
            }
            if bitrate > 0 && profile.format != "gif" {
                if bitrate < 48_000 { return Err("Size limit is too small for this video's duration".into()); }
                let audio = (bitrate / 5).clamp(16_000, 96_000);
                if profile.format == "avi" { cmd.args(["-b:v", &(bitrate - audio).to_string(), "-b:a", &audio.to_string()]); }
                else { cmd.args(["-b:v", &(bitrate - audio).to_string(), "-b:a", &audio.to_string()]); }
            } else {
                match profile.format.as_str() {
                    "mp4" | "mov" | "mkv" => { cmd.args(["-crf", "24", "-b:a", "128k"]); }
                    "webm" => { cmd.args(["-crf", "34", "-b:v", "0", "-b:a", "128k"]); }
                    "avi" => { cmd.args(["-q:v", "4", "-b:a", "128k"]); }
                    "wmv" => { cmd.args(["-b:v", "2M", "-b:a", "128k"]); }
                    _ => {}
                }
            }
            let width = if profile.width > 0 { profile.width } else { item.width };
            let height = if profile.height > 0 { profile.height } else { item.height };
            let shrink = if profile.allow_smaller && bitrate > 0 { 0.8_f64.powi(pass) } else { 1. };
            let w = ((width as f64 * shrink) as u32 / 2 * 2).max(2);
            let h = ((height as f64 * shrink) as u32 / 2 * 2).max(2);
            let filter = if profile.format == "gif" { format!("fps=12,scale={w}:{h}:force_original_aspect_ratio=decrease:flags=lanczos") }
                else { format!("scale={w}:{h}:force_original_aspect_ratio=decrease:force_divisible_by=2") };
            cmd.args(["-vf", &filter]);
        } else {
            let codec = match profile.format.as_str() { "mp3" => "libmp3lame", "m4a" | "aac" => "aac", "wav" => "pcm_s16le", "flac" => "flac", "ogg" => "libvorbis", "opus" => "libopus", "aiff" => "pcm_s16be", "wma" => "wmav2", _ => return Err("Format unavailable".into()) };
            cmd.args(["-map", "0:a:0", "-vn", "-c:a", codec]);
            if bitrate > 0 {
                if bitrate < 16_000 { return Err("Size limit is too small for this audio's duration".into()); }
                cmd.args(["-b:a", &bitrate.min(320_000).to_string()]);
            }
        }
        cmd.arg(output).stdout(Stdio::piped()).stderr(Stdio::null());
        let mut child = cmd.spawn().map_err(|_| "Media support unavailable")?;
        let stdout = child.stdout.take().unwrap();
        let (tx, rx) = mpsc::sync_channel::<f64>(4);
        let reader = std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if let Some(time) = line.strip_prefix("out_time_us=").and_then(|v| v.parse::<f64>().ok()) { let _ = tx.try_send(time / 1_000_000.); }
            }
        });
        let mut last = Instant::now();
        let started = Instant::now();
        let success = loop {
            if cancel.load(Ordering::Acquire) || started.elapsed() > Duration::from_secs(6 * 3600) {
                let _ = child.kill(); let _ = child.wait(); break false;
            }
            let mut seconds = None;
            while let Ok(value) = rx.try_recv() { seconds = Some(value); }
            if let Some(seconds) = seconds.filter(|_| last.elapsed() >= Duration::from_millis(250)) {
                let mut update = item.clone(); update.status = "running".into();
                update.message = if pass == 0 { "Converting" } else { "Refining size" }.into();
                update.progress = (item.duration > 0.).then(|| (seconds / item.duration).clamp(0., 0.99));
                let _ = sender.try_send(Event::Item(update)); last = Instant::now();
            }
            match child.try_wait() {
                Ok(Some(status)) => break status.success(),
                Ok(None) => std::thread::sleep(Duration::from_millis(50)),
                Err(_) => { let _ = child.kill(); let _ = child.wait(); break false; }
            }
        };
        let _ = reader.join();
        if !success { return Err(if cancel.load(Ordering::Acquire) { "Cancelled" } else { "Conversion failed. Check the file, output format, and free space." }.into()); }
        let size = fs::metadata(output).map_err(|_| "Couldn’t inspect output")?.len();
        if profile.max_bytes == 0 || size <= profile.max_bytes { return Ok(()); }
    }
    Err("Size limit not reached. Increase the limit or allow smaller dimensions.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
            let path = std::env::temp_dir().join(format!("island-conversion-{}-{stamp}", std::process::id()));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
        fn image(&self, alpha: u8) -> Item {
            let path = self.0.join("sample.png");
            let pixels = image::RgbaImage::from_fn(256, 192, |x, y| image::Rgba([x as u8, y as u8, (x ^ y) as u8, alpha]));
            pixels.save(&path).unwrap();
            let mut item = Item { id: 1, name: "sample.png".into(), kind: String::new(), bytes: 0,
                width: 0, height: 0, duration: 0., status: String::new(), message: String::new(),
                progress: None, formats: vec![], output_bytes: 0, path, output: None };
            inspect(&mut item, None, &[], &AtomicBool::new(false));
            item
        }
    }
    impl Drop for Fixture { fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); } }

    #[test]
    fn resize_preserves_source_and_never_overwrites_outputs() {
        let fixture = Fixture::new();
        let item = fixture.image(255);
        assert_eq!(item.status, "ready");
        let original = fs::read(&item.path).unwrap();
        let profile = Profile { format: "png".into(), width: 128, ..Default::default() };
        let (sender, _) = mpsc::sync_channel(64);
        let cancel = AtomicBool::new(false);
        let first = convert(&item, &profile, "", None, &cancel, &sender).unwrap();
        let second = convert(&item, &profile, "", None, &cancel, &sender).unwrap();
        assert_ne!(first, second);
        assert_eq!(image::image_dimensions(first).unwrap(), (128, 96));
        assert_eq!(fs::read(&item.path).unwrap(), original);
    }

    #[test]
    fn byte_limit_is_measured_and_transparency_is_not_silently_lost() {
        let fixture = Fixture::new();
        let item = fixture.image(100);
        let (sender, _) = mpsc::sync_channel(64);
        let cancel = AtomicBool::new(false);
        let jpeg = Profile { format: "jpg".into(), ..Default::default() };
        assert!(convert(&item, &jpeg, "", None, &cancel, &sender).unwrap_err().contains("transparency"));
        let profile = Profile { format: "png".into(), max_bytes: 2000, allow_smaller: true, ..Default::default() };
        let output = convert(&item, &profile, "", None, &cancel, &sender).unwrap();
        assert!(fs::metadata(output).unwrap().len() <= 2000);
        assert!(!fs::read_dir(&fixture.0).unwrap().any(|p| p.unwrap().file_name().to_string_lossy().starts_with(".island-")));
    }

    #[test]
    fn cancelled_job_leaves_only_the_original() {
        let fixture = Fixture::new();
        let item = fixture.image(255);
        let (sender, _) = mpsc::sync_channel(64);
        let profile = Profile { format: "png".into(), ..Default::default() };
        assert!(convert(&item, &profile, "", None, &AtomicBool::new(true), &sender).is_err());
        assert_eq!(fs::read_dir(&fixture.0).unwrap().count(), 1);
    }
}
