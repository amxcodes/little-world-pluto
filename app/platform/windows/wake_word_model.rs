use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread::{self, JoinHandle};

pub const MODEL_VERSION: &str = "2024-01-01";
const MODEL_DIR_NAME: &str = "english-zipformer";

pub fn model_dir() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|root| {
        PathBuf::from(root)
            .join("Pluto")
            .join("wake-model")
            .join(MODEL_DIR_NAME)
    })
}

#[derive(Clone, Debug)]
pub struct ModelState {
    pub status: &'static str,
    pub installed_version: Option<String>,
    pub error: Option<String>,
}

pub struct WakeModelManager {
    state: ModelState,
    receiver: Option<Receiver<Result<(), String>>>,
    worker: Option<JoinHandle<()>>,
}

impl WakeModelManager {
    pub fn new() -> Self {
        let installed_version = installed_version();
        let status = if installed_version.as_deref() == Some(MODEL_VERSION) {
            "ready"
        } else {
            "missing"
        };
        Self {
            state: ModelState {
                status,
                installed_version,
                error: None,
            },
            receiver: None,
            worker: None,
        }
    }

    pub fn state(&self) -> &ModelState {
        &self.state
    }

    pub fn ready(&self) -> bool {
        self.state.status == "ready"
    }

    pub fn install(&mut self) {
        if self.state.status == "installing" {
            return;
        }
        self.state.status = "installing";
        self.state.error = None;
        let (sender, receiver) = mpsc::channel();
        self.receiver = Some(receiver);
        let worker = thread::Builder::new()
            .name("pluto-wake-model".into())
            .spawn(move || {
                let result = install_model();
                let _ = sender.send(result);
            });
        match worker {
            Ok(worker) => self.worker = Some(worker),
            Err(error) => {
                self.state.status = "error";
                self.state.error =
                    Some(format!("Could not start the wake model installer: {error}"));
            }
        }
    }

    pub fn poll(&mut self) -> bool {
        let Some(receiver) = self.receiver.as_ref() else {
            return false;
        };
        match receiver.try_recv() {
            Ok(Ok(())) => {
                self.state.status = "ready";
                self.state.installed_version = Some(MODEL_VERSION.to_owned());
                self.state.error = None;
                self.receiver = None;
                self.join_worker();
                true
            }
            Ok(Err(error)) => {
                self.state.status = "error";
                self.state.installed_version = installed_version();
                self.state.error = Some(error);
                self.receiver = None;
                self.join_worker();
                true
            }
            Err(TryRecvError::Disconnected) => {
                self.state.status = "error";
                self.state.error = Some("The wake model installer stopped unexpectedly.".into());
                self.receiver = None;
                self.join_worker();
                true
            }
            Err(TryRecvError::Empty) => false,
        }
    }

    fn join_worker(&mut self) {
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl Drop for WakeModelManager {
    fn drop(&mut self) {
        self.receiver = None;
        if let Some(worker) = self.worker.take()
            && worker.is_finished()
        {
            let _ = worker.join();
        }
    }
}

fn installed_version() -> Option<String> {
    let directory = model_dir()?;
    let version = std::fs::read_to_string(directory.join(".pluto-model-version")).ok()?;
    let version = version.trim();
    if !model_files_exist(&directory) {
        return None;
    }
    Some(version.to_owned())
}

fn model_files_exist(directory: &std::path::Path) -> bool {
    [
        "encoder-epoch-12-avg-2-chunk-16-left-64.int8.onnx",
        "decoder-epoch-12-avg-2-chunk-16-left-64.onnx",
        "joiner-epoch-12-avg-2-chunk-16-left-64.int8.onnx",
        "tokens.txt",
        "bpe.model",
    ]
    .iter()
    .all(|name| std::fs::metadata(directory.join(name)).is_ok_and(|metadata| metadata.is_file()))
}

fn install_model() -> Result<(), String> {
    let target = model_dir().ok_or("Windows user model folder is unavailable.")?;
    let app_data = target
        .parent()
        .and_then(std::path::Path::parent)
        .ok_or("Could not locate Pluto's local data folder.")?;
    std::fs::create_dir_all(app_data)
        .map_err(|error| format!("Could not prepare the wake model folder: {error}"))?;

    let script = r#"
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
$appData = Join-Path $env:LOCALAPPDATA 'Pluto'
$temp = Join-Path ([IO.Path]::GetTempPath()) ('pluto-wake-' + [guid]::NewGuid().ToString('N'))
$stageRoot = Join-Path $appData 'wake-model.new'
$oldRoot = Join-Path $appData 'wake-model.old'
$model = 'sherpa-onnx-kws-zipformer-gigaspeech-3.3M-2024-01-01'
$archive = Join-Path $temp ($model + '-mobile.tar.bz2')
$url = 'https://github.com/k2-fsa/sherpa-onnx/releases/download/kws-models/' + $model + '-mobile.tar.bz2'
New-Item -ItemType Directory -Force -Path $temp | Out-Null
try {
  Remove-Item -LiteralPath $stageRoot,$oldRoot -Recurse -Force -ErrorAction SilentlyContinue
  Invoke-WebRequest -UseBasicParsing -Headers @{ 'User-Agent' = 'Pluto-Desktop' } -Uri $url -OutFile $archive
  $listing = @(& tar.exe -tjf $archive)
  if ($LASTEXITCODE -ne 0 -or $listing.Count -eq 0) { throw 'The official model archive could not be read.' }
  foreach ($entry in $listing) {
    if ($entry -match '(^[\\/]|(^|[\\/])\.\.([\\/]|$)|^[A-Za-z]:)') { throw 'The model archive contains an unsafe path.' }
  }
  $extract = Join-Path $temp 'extract'
  New-Item -ItemType Directory -Force -Path $extract | Out-Null
  & tar.exe -xjf $archive -C $extract
  if ($LASTEXITCODE -ne 0) { throw 'Could not unpack the official model archive.' }
  $files = @(
    @{ Name = 'encoder-epoch-12-avg-2-chunk-16-left-64.int8.onnx'; Hash = '7d5a25fbc14b423074fe0c58e1be388ccf3f1a815c649980232a0c47f81875e4'; Bytes = 4000908 },
    @{ Name = 'decoder-epoch-12-avg-2-chunk-16-left-64.onnx'; Hash = 'f61ebd3eed3773a44d088d53dfae92dbb6aec4839f4dcaee2d402414741663a3'; Bytes = 1063189 },
    @{ Name = 'joiner-epoch-12-avg-2-chunk-16-left-64.int8.onnx'; Hash = 'eae9da0c7e1e6c6a3f4cc42d167899c388f6c6701b94cb96320e4f55df79624c'; Bytes = 163380 },
    @{ Name = 'tokens.txt'; Hash = 'fd2ded4050a55d2b1578870ba8697d02371980217806b7558bd0a5cc60f3ba53'; Bytes = 5006 },
    @{ Name = 'bpe.model'; Hash = 'c8a2a0129c4ab8e463164c142f82d25649661b122c8cd0b7aab5c9e80b90ad24'; Bytes = 244837 }
  )
  $stage = Join-Path $stageRoot 'english-zipformer'
  New-Item -ItemType Directory -Force -Path $stage | Out-Null
  foreach ($file in $files) {
    $source = Get-ChildItem -LiteralPath $extract -Filter $file.Name -File -Recurse | Select-Object -First 1
    if (-not $source -or $source.Length -ne $file.Bytes) { throw "The model archive is missing a valid $($file.Name)." }
    $hash = (Get-FileHash -LiteralPath $source.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($hash -cne $file.Hash) { throw "The model file $($file.Name) failed SHA-256 verification." }
    Copy-Item -LiteralPath $source.FullName -Destination (Join-Path $stage $file.Name)
  }
  [IO.File]::WriteAllText((Join-Path $stage '.pluto-model-version'), '2024-01-01', [Text.Encoding]::ASCII)
  if (Test-Path -LiteralPath (Join-Path $appData 'wake-model')) { Move-Item -LiteralPath (Join-Path $appData 'wake-model') -Destination $oldRoot }
  try { Move-Item -LiteralPath $stageRoot -Destination (Join-Path $appData 'wake-model') }
  catch {
    if (Test-Path -LiteralPath $oldRoot) { Move-Item -LiteralPath $oldRoot -Destination (Join-Path $appData 'wake-model') }
    throw "Could not activate the verified wake model. $($_.Exception.Message)"
  }
  Remove-Item -LiteralPath $oldRoot -Recurse -Force -ErrorAction SilentlyContinue
} finally {
  Remove-Item -LiteralPath $temp -Recurse -Force -ErrorAction SilentlyContinue
  Remove-Item -LiteralPath $stageRoot -Recurse -Force -ErrorAction SilentlyContinue
}
"#;
    let mut command = Command::new("powershell.exe");
    command
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let output = command
        .output()
        .map_err(|error| format!("Could not start the wake model installer: {error}"))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr)
            .lines()
            .last()
            .unwrap_or("")
            .trim()
            .to_owned();
        return Err(if detail.is_empty() {
            "Could not download or verify the Sherpa-ONNX wake model. Check your connection and try again.".into()
        } else {
            format!(
                "Wake model installation failed: {}",
                detail.chars().take(220).collect::<String>()
            )
        });
    }
    if !installed_version().is_some_and(|version| version == MODEL_VERSION) {
        return Err("The downloaded wake model did not pass its installation check.".into());
    }
    Ok(())
}
