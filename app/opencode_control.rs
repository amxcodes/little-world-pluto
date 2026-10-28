//! OpenCode V2 local-service adapter. Reads session metadata, pending permissions,
//! and the latest assistant reply for the selected session.
//! Pairing tokens stay in this worker's memory and are never sent to the WebView.
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddrV4, TcpStream};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{
    Arc,
    mpsc::{self, Receiver, Sender},
};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::{Value, json};

#[derive(Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub connected: bool,
    pub missing: bool,
    pub installing: bool,
    pub error: String,
    pub notice: String,
    pub operation_revision: u64,
    pub operation_ok: bool,
    pub operation_kind: String,
    pub last_created_id: String,
    pub installed_version: String,
    pub latest_version: String,
    pub update_available: bool,
    pub checking_update: bool,
    pub install_source: String,
    pub models: Vec<Model>,
    pub default_model: String,
    pub model_directory: String,
    pub model_error: String,
    pub agents: Vec<Agent>,
    pub agent_error: String,
    pub sessions: Vec<Session>,
    pub active_count: usize,
    pub response: Option<ResponsePreview>,
    pub response_error: String,
    pub permissions: Vec<Permission>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub id: String,
    pub title: String,
    pub directory: String,
    pub active: bool,
    pub updated: i64,
    pub model: String,
    pub agent: String,
}

#[derive(Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Permission {
    pub id: String,
    pub session_id: String,
    pub action: String,
    pub resources: Vec<String>,
}

#[derive(Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Model {
    pub key: String,
    pub name: String,
    pub provider: String,
}

#[derive(Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Agent {
    pub id: String,
    pub description: String,
    pub mode: String,
    pub model: String,
    pub hidden: bool,
    pub managed: bool,
    pub system: String,
}

#[derive(Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResponsePreview {
    pub session_id: String,
    pub text: String,
    pub truncated: bool,
    pub waiting: bool,
    pub plan: Vec<ResponsePlanItem>,
    pub steps: Vec<ResponseStep>,
}

#[derive(Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResponsePlanItem {
    pub id: String,
    pub content: String,
    pub status: String,
}

#[derive(Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResponseStep {
    pub id: String,
    pub title: String,
    pub detail: String,
    pub status: String,
}

pub enum Action {
    Install,
    Update,
    CheckUpdate,
    Catalog(Option<String>),
    Refresh,
    More,
    Select(String),
    Start {
        directory: Option<String>,
        text: String,
        model: Option<String>,
        agent: Option<String>,
        files: Vec<String>,
    },
    Prompt {
        session_id: String,
        text: String,
        agent: Option<String>,
        files: Vec<String>,
    },
    Abort {
        session_id: String,
    },
    SetModel {
        session_id: String,
        model: String,
    },
    SaveAgent {
        id: String,
        description: String,
        mode: String,
        model: String,
        system: String,
    },
    DeleteAgent {
        id: String,
    },
    RefreshAgents,
    Reply {
        session_id: String,
        request_id: String,
        allow: bool,
    },
}

pub fn start() -> (Sender<Action>, pluto::latest::Receiver<Arc<Snapshot>>) {
    let (actions, incoming) = mpsc::channel();
    let (outgoing, snapshots) = pluto::latest::channel();
    let _ = thread::Builder::new()
        .name("island-opencode-control".into())
        .spawn(move || run(incoming, outgoing));
    (actions, snapshots)
}

struct Client {
    port: u16,
    password: String,
}

fn run(incoming: Receiver<Action>, outgoing: pluto::latest::Sender<Arc<Snapshot>>) {
    let mut client: Option<Client> = None;
    let mut snapshot = Snapshot::default();
    let mut last_poll = Instant::now() - Duration::from_secs(10);
    let mut last_sent: Option<Arc<Snapshot>> = None;
    let mut selected_session = String::new();
    let mut loaded_count = 40usize;
    let (versions_tx, versions_rx) = mpsc::channel();
    let mut version_generation = 0u64;
    let mut last_version_check = Instant::now() - Duration::from_secs(6 * 60 * 60);
    let mut last_model_refresh = Instant::now() - Duration::from_secs(60);
    let mut last_agent_refresh = Instant::now() - Duration::from_secs(30);
    loop {
        let mut action = incoming.recv_timeout(Duration::from_millis(300)).ok();
        let pending_kind = match &action {
            Some(Action::Start { .. }) => "start",
            Some(Action::Prompt { .. }) => "prompt",
            Some(Action::Abort { .. }) => "abort",
            Some(Action::SetModel { .. }) => "model",
            Some(Action::Reply { .. }) => "approval",
            _ => "",
        };
        if matches!(action, Some(Action::CheckUpdate)) {
            last_version_check = Instant::now() - Duration::from_secs(6 * 60 * 60);
            action = None;
        }
        let pending_operation = matches!(
            action,
            Some(
                Action::Start { .. }
                    | Action::Prompt { .. }
                    | Action::Abort { .. }
                    | Action::SetModel { .. }
                    | Action::Reply { .. }
            )
        );
        if matches!(action, Some(Action::Install | Action::Update)) {
            let updating = matches!(action, Some(Action::Update));
            if snapshot.active_count > 0 {
                snapshot.notice = "Finish active OpenCode runs before updating.".into();
                last_sent = None;
                continue;
            }
            snapshot.installing = true;
            version_generation = version_generation.wrapping_add(1);
            snapshot.checking_update = false;
            snapshot.connected = false;
            snapshot.notice = if updating {
                "Updating OpenCode…"
            } else {
                "Installing OpenCode…"
            }
            .into();
            snapshot.error.clear();
            let _ = outgoing.send(Arc::new(snapshot.clone()));
            match if updating {
                update_cli(&snapshot.install_source)
            } else {
                install_cli()
            } {
                Ok(()) => {
                    snapshot.notice = if updating {
                        "OpenCode updated. Reconnecting…"
                    } else {
                        "OpenCode installed. Connecting…"
                    }
                    .into();
                    snapshot.missing = false;
                    snapshot.update_available = false;
                    snapshot.models.clear();
                }
                Err(error) => {
                    snapshot.notice = error;
                }
            }
            snapshot.installing = false;
            client = None;
            last_version_check = Instant::now() - Duration::from_secs(6 * 60 * 60);
            last_model_refresh = Instant::now() - Duration::from_secs(60);
            last_agent_refresh = Instant::now() - Duration::from_secs(30);
            last_sent = None;
            continue;
        }
        if client.is_none() {
            match connect() {
                Ok(value) => {
                    client = Some(value);
                    snapshot.error.clear();
                    snapshot.missing = false;
                }
                Err(error) => {
                    snapshot.connected = false;
                    snapshot.missing = error == "OpenCode is not installed";
                    snapshot.error = error;
                }
            }
            if client.is_none() && pending_operation {
                snapshot.operation_revision = snapshot.operation_revision.wrapping_add(1);
                snapshot.operation_ok = false;
                snapshot.operation_kind = pending_kind.into();
                snapshot.notice = "OpenCode is not connected. The action was not sent.".into();
            }
        }
        if let Some(ref conn) = client {
            if let Some(action) = action {
                if matches!(action, Action::More) && snapshot.next_cursor.is_some() {
                    loaded_count = loaded_count.saturating_add(40);
                }
                let is_operation = matches!(
                    action,
                    Action::Start { .. }
                        | Action::Prompt { .. }
                        | Action::Abort { .. }
                        | Action::SetModel { .. }
                        | Action::Reply { .. }
                );
                let result = match action {
                    Action::Select(id) => {
                        if id.is_empty() || valid_id(&id) {
                            selected_session = id;
                            snapshot.response = None;
                            snapshot.response_error.clear();
                        }
                        Ok(())
                    }
                    Action::Catalog(directory) => {
                        snapshot.model_directory = directory.unwrap_or_default();
                        last_model_refresh = Instant::now() - Duration::from_secs(60);
                        Ok(())
                    }
                    Action::Install | Action::Update | Action::CheckUpdate => Ok(()),
                    other => apply_action(conn, &mut snapshot, other),
                };
                if is_operation {
                    snapshot.operation_revision = snapshot.operation_revision.wrapping_add(1);
                    snapshot.operation_ok = result.is_ok();
                    snapshot.operation_kind = pending_kind.into();
                    if result.is_ok() && pending_kind == "start" {
                        selected_session = snapshot.last_created_id.clone();
                    }
                }
                if let Err(error) = result {
                    snapshot.notice = error;
                }
                last_poll = Instant::now() - Duration::from_secs(10);
            }
            if snapshot.connected && last_model_refresh.elapsed() >= Duration::from_secs(60) {
                match refresh_models(conn, &mut snapshot) {
                    Ok(()) => snapshot.model_error.clear(),
                    Err(error) => {
                        snapshot.models.clear();
                        snapshot.model_error = error;
                    }
                }
                last_model_refresh = Instant::now();
            }
            if snapshot.connected && last_agent_refresh.elapsed() >= Duration::from_secs(20) {
                match refresh_agents(conn, &mut snapshot) {
                    Ok(()) => snapshot.agent_error.clear(),
                    Err(error) => snapshot.agent_error = error,
                }
                last_agent_refresh = Instant::now();
            }
            if last_poll.elapsed() >= Duration::from_secs(2) {
                if let Err(error) = refresh(conn, &mut snapshot, &selected_session, loaded_count) {
                    snapshot.connected = false;
                    snapshot.error = error;
                    snapshot.active_count = 0;
                    snapshot.permissions.clear();
                    for session in &mut snapshot.sessions {
                        session.active = false;
                    }
                    client = None;
                } else {
                    snapshot.connected = true;
                    snapshot.error.clear();
                }
                last_poll = Instant::now();
            }
        }
        if let Ok((generation, installed, latest, source)) = versions_rx.try_recv()
            && generation == version_generation
        {
            snapshot.installed_version = installed;
            snapshot.latest_version = latest;
            snapshot.install_source = source;
            snapshot.update_available =
                newer_version(&snapshot.latest_version, &snapshot.installed_version);
            snapshot.checking_update = false;
        }
        if !snapshot.checking_update
            && last_version_check.elapsed() >= Duration::from_secs(6 * 60 * 60)
        {
            let tx = versions_tx.clone();
            let generation = version_generation;
            snapshot.checking_update = true;
            last_version_check = Instant::now();
            if thread::Builder::new()
                .name("island-opencode-version".into())
                .spawn(move || {
                    let installed =
                        normalized_version(&cli_output(&["--version"]).unwrap_or_default());
                    let latest = latest_version().unwrap_or_default();
                    let _ = tx.send((generation, installed, latest, cli_source().to_owned()));
                })
                .is_err()
            {
                snapshot.checking_update = false;
            }
        }
        // Compare the model directly; do not allocate a full JSON copy every poll.
        if last_sent.as_deref() != Some(&snapshot) {
            let current = Arc::new(snapshot.clone());
            if outgoing.send(Arc::clone(&current)).is_err() {
                break;
            }
            last_sent = Some(current);
        }
        if client.is_none() {
            thread::sleep(Duration::from_secs(3));
        }
    }
}

fn cli() -> std::path::PathBuf {
    if let Some(path) = managed_cli().filter(|path| path.is_file()) {
        return path;
    }
    std::env::var_os("APPDATA")
        .map(|root| {
            std::path::PathBuf::from(root)
                .join("npm")
                .join("opencode.cmd")
        })
        .filter(|path| path.is_file())
        .unwrap_or_else(|| "opencode".into())
}

fn cli_source() -> &'static str {
    if managed_cli().is_some_and(|path| path.is_file()) {
        "island"
    } else if std::env::var_os("APPDATA")
        .map(|root| {
            std::path::PathBuf::from(root)
                .join("npm")
                .join("opencode.cmd")
        })
        .is_some_and(|path| path.is_file())
    {
        "npm"
    } else {
        "external"
    }
}

fn latest_version() -> Result<String, String> {
    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command",
            "$ErrorActionPreference='Stop'; (Invoke-RestMethod -Uri 'https://opencode.ai/update/api/latest/cli/npm' -TimeoutSec 12).version"])
        .stdin(Stdio::null()).creation_flags(0x0800_0000).output()
        .map_err(|_| "Could not check OpenCode releases".to_owned())?;
    if !output.status.success() {
        return Err("Could not check OpenCode releases".into());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn newer_version(latest: &str, installed: &str) -> bool {
    fn parts(value: &str) -> Option<Vec<u32>> {
        let value = value.trim().trim_start_matches('v');
        let value = value.split('-').next()?;
        let numbers: Vec<_> = value
            .split('.')
            .map(str::parse::<u32>)
            .collect::<Result<_, _>>()
            .ok()?;
        if numbers.len() == 3 {
            Some(numbers)
        } else {
            None
        }
    }
    matches!((parts(latest), parts(installed)), (Some(new), Some(old)) if new > old)
}

fn normalized_version(output: &str) -> String {
    output
        .split_whitespace()
        .find_map(|word| {
            let version = word.trim_start_matches('v');
            let core = version.split('-').next()?;
            let mut parts = core.split('.');
            if parts
                .by_ref()
                .take(3)
                .filter(|part| part.parse::<u32>().is_ok())
                .count()
                == 3
                && parts.next().is_none()
            {
                Some(version.to_owned())
            } else {
                None
            }
        })
        .unwrap_or_else(|| output.trim().to_owned())
}

fn update_cli(source: &str) -> Result<(), String> {
    let _ = cli_output(&["service", "stop"]);
    if source == "npm" {
        let mut command = Command::new("npm.cmd");
        command.args(["install", "-g", "@opencode/cli@latest"]);
        if !run_hidden_with_timeout(&mut command, Duration::from_secs(300))? {
            return Err(
                "npm could not update OpenCode. Close other OpenCode windows and retry.".into(),
            );
        }
        Ok(())
    } else {
        install_cli()
    }
}

fn run_hidden_with_timeout(command: &mut Command, timeout: Duration) -> Result<bool, String> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(0x0800_0000)
        .spawn()
        .map_err(|_| "Installer could not start".to_owned())?;
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().map_err(|_| "Installer process failed")? {
            return Ok(status.success());
        }
        if start.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err("OpenCode installation timed out. Check the connection and retry.".into());
        }
        thread::sleep(Duration::from_millis(250));
    }
}

fn managed_cli() -> Option<std::path::PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|root| {
        std::path::PathBuf::from(root)
            .join("Pluto")
            .join("tools")
            .join("opencode")
            .join("opencode.exe")
    })
}

fn cli_output(args: &[&str]) -> Result<String, String> {
    let output = Command::new(cli())
        .args(args)
        .stdin(Stdio::null())
        .creation_flags(0x0800_0000)
        .output()
        .map_err(|_| "OpenCode is not installed".to_owned())?;
    if !output.status.success() {
        return Err("OpenCode service could not start".into());
    }
    String::from_utf8(output.stdout).map_err(|_| "Invalid OpenCode service response".into())
}

fn install_cli() -> Result<(), String> {
    let destination = managed_cli().ok_or("Cannot locate the Windows app data folder")?;
    let asset = if cfg!(target_arch = "aarch64") {
        "opencode-windows-arm64.zip"
    } else {
        "opencode-windows-x64-baseline.zip"
    };
    let uri = format!("https://opencode.ai/update/api/latest/cli/opencode/{asset}");
    let script = format!(
        r#"
$ErrorActionPreference = 'Stop'
$root = Join-Path $env:LOCALAPPDATA 'Pluto\tools\opencode'
New-Item -ItemType Directory -Path $root -Force | Out-Null
$stage = Join-Path $root ('stage-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $stage -Force | Out-Null
try {{
  $archive = Join-Path $stage 'opencode.zip'
  Invoke-WebRequest -Uri '{uri}' -OutFile $archive -UseBasicParsing -TimeoutSec 180
  $archiveSize = (Get-Item -LiteralPath $archive).Length
  if ($archiveSize -lt 1 -or $archiveSize -gt 1GB) {{ throw 'OpenCode archive size is outside the allowed range' }}
  Add-Type -AssemblyName System.IO.Compression.FileSystem
  $zip = [IO.Compression.ZipFile]::OpenRead($archive)
  try {{
    if ($zip.Entries.Count -gt 256) {{ throw 'OpenCode archive has too many entries' }}
    $expandedSize = ($zip.Entries | Measure-Object -Property Length -Sum).Sum
    if ($expandedSize -gt 2GB) {{ throw 'OpenCode archive expands beyond the allowed size' }}
  }} finally {{ $zip.Dispose() }}
  Expand-Archive -LiteralPath $archive -DestinationPath (Join-Path $stage 'unpacked') -Force
  $binaries = @(Get-ChildItem -LiteralPath (Join-Path $stage 'unpacked') -Recurse -Filter 'opencode.exe' -File)
  if ($binaries.Count -ne 1) {{ throw 'Expected exactly one OpenCode executable in official archive' }}
  $binary = $binaries[0]
  $next = Join-Path $root 'opencode.new.exe'
  Copy-Item -LiteralPath $binary.FullName -Destination $next -Force
  $signature = Get-AuthenticodeSignature -FilePath $next
  if ($signature.Status -ne 'Valid' -or $null -eq $signature.SignerCertificate) {{ throw "OpenCode signature is not valid: $($signature.Status)" }}
  $publisher = $signature.SignerCertificate.GetNameInfo([System.Security.Cryptography.X509Certificates.X509NameType]::SimpleName, $false)
  if ($publisher -ne 'Anomaly Innovations, Inc https://anoma.ly/') {{ throw "Unexpected OpenCode publisher: $publisher" }}
  & $next --version | Out-Null
  if ($LASTEXITCODE -ne 0) {{ throw 'OpenCode binary did not start' }}
  Move-Item -LiteralPath $next -Destination (Join-Path $root 'opencode.exe') -Force
}} finally {{
  $rootFull = [IO.Path]::GetFullPath($root).TrimEnd('\') + '\'
  $stageFull = [IO.Path]::GetFullPath($stage)
  if ($stageFull.StartsWith($rootFull, [StringComparison]::OrdinalIgnoreCase) -and (Test-Path -LiteralPath $stage)) {{ Remove-Item -LiteralPath $stage -Recurse -Force }}
}}
"#
    );
    let mut command = Command::new("powershell.exe");
    command.args(["-NoProfile", "-NonInteractive", "-Command", &script]);
    if !run_hidden_with_timeout(&mut command, Duration::from_secs(300))? || !destination.is_file() {
        return Err("OpenCode installation failed. Check the internet connection and available disk space, then retry.".into());
    }
    Ok(())
}

#[cfg(windows)]
use std::os::windows::process::CommandExt;

fn connect() -> Result<Client, String> {
    let address = cli_output(&["service", "start"])?;
    let port = parse_port(&address).ok_or("OpenCode did not provide a local service address")?;
    let pair = cli_output(&["pair"])?;
    let marker = format!("http://127.0.0.1:{port}/auth/connect/");
    let offset = pair
        .find(&marker)
        .ok_or("OpenCode pairing link was unavailable")?;
    let code: String = pair[offset + marker.len()..]
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    if code.is_empty() {
        return Err("OpenCode pairing code was unavailable".into());
    }
    let (_, body) = request(port, None, "GET", &format!("/auth/connect/{code}"), None)?;
    let value: Value = serde_json::from_slice(&body).map_err(|_| "OpenCode pairing failed")?;
    let password = value["token"]
        .as_str()
        .ok_or("OpenCode pairing token was unavailable")?
        .to_owned();
    let client = Client { port, password };
    let _ = api(&client, "GET", "/api/info", None)?;
    Ok(client)
}

fn parse_port(text: &str) -> Option<u16> {
    let start = text.find("http://127.0.0.1:")? + "http://127.0.0.1:".len();
    text[start..]
        .split(|c: char| !c.is_ascii_digit())
        .next()?
        .parse()
        .ok()
}

fn model_key(value: &Value) -> String {
    match (value["providerID"].as_str(), value["id"].as_str()) {
        (Some(provider), Some(model)) => format!("{provider}/{model}"),
        _ => String::new(),
    }
}

fn checked_model(state: &Snapshot, key: Option<&str>) -> Result<Option<Value>, String> {
    let Some(key) = key.filter(|key| !key.is_empty()) else {
        return Ok(None);
    };
    if !state.models.iter().any(|item| item.key == key) {
        return Err("That model is no longer available here. Choose another model.".into());
    }
    let (provider, id) = key.split_once('/').ok_or("Invalid model selection")?;
    Ok(Some(json!({ "id": id, "providerID": provider })))
}

fn checked_primary_agent<'a>(
    state: &'a Snapshot,
    id: Option<&'a str>,
) -> Result<Option<&'a str>, String> {
    let Some(id) = id.filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let Some(agent) = state
        .agents
        .iter()
        .find(|agent| agent.id == id && !agent.hidden)
    else {
        return Err("That OpenCode agent is no longer available. Refresh the agent list and choose another.".into());
    };
    if agent.mode == "subagent" {
        return Err(
            "Choose a primary agent here; mention subagents in the prompt with @name.".into(),
        );
    }
    Ok(Some(id))
}

fn prompt_agent_mentions(prompt: &str, agents: &[Agent]) -> Vec<Value> {
    let mut matches: Vec<(usize, Value)> = Vec::new();
    for agent in agents
        .iter()
        .filter(|agent| !agent.hidden && matches!(agent.mode.as_str(), "subagent" | "all"))
    {
        let token = format!("@{}", agent.id);
        let mut offset = 0;
        while let Some(relative) = prompt[offset..].find(&token) {
            let start = offset + relative;
            let end = start + token.len();
            let before_ok = prompt[..start]
                .chars()
                .next_back()
                .is_none_or(|value| !value.is_ascii_alphanumeric() && !matches!(value, '_' | '-'));
            let after_ok = prompt[end..]
                .chars()
                .next()
                .is_none_or(|value| !value.is_ascii_alphanumeric() && !matches!(value, '_' | '-'));
            if before_ok && after_ok {
                let utf16_start = prompt[..start].encode_utf16().count();
                let utf16_end = utf16_start + token.encode_utf16().count();
                matches.push((
                    start,
                    json!({
                        "name": agent.id,
                        "mention": { "start": utf16_start, "end": utf16_end, "text": token }
                    }),
                ));
            }
            offset = end;
            if offset >= prompt.len() {
                break;
            }
        }
    }
    matches.sort_by_key(|(start, _)| *start);
    let mut seen = std::collections::HashSet::new();
    matches
        .into_iter()
        .filter_map(|(_, value)| {
            let name = value["name"].as_str().unwrap_or("").to_owned();
            let start = value["mention"]["start"].as_u64().unwrap_or(0);
            seen.insert((name, start)).then_some(value)
        })
        .collect()
}

fn refresh_models(client: &Client, state: &mut Snapshot) -> Result<(), String> {
    let location = if state.model_directory.is_empty() {
        String::new()
    } else {
        format!(
            "?location%5Bdirectory%5D={}",
            url_encode(&state.model_directory)
        )
    };
    let providers = api(client, "GET", &format!("/api/provider{location}"), None)?;
    let provider_ids: std::collections::HashSet<_> = providers["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| item["id"].as_str())
        .collect();
    let response = api(client, "GET", &format!("/api/model{location}"), None)?;
    state.models = response["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| {
            if item["enabled"].as_bool() != Some(true) {
                return None;
            }
            let provider = item["providerID"].as_str()?;
            if !provider_ids.contains(provider) {
                return None;
            }
            let id = item["id"].as_str()?;
            Some(Model {
                key: format!("{provider}/{id}"),
                name: item["name"].as_str().unwrap_or(id).to_owned(),
                provider: provider.to_owned(),
            })
        })
        .take(200)
        .collect();
    state.default_model = api(
        client,
        "GET",
        &format!("/api/model/default{location}"),
        None,
    )
    .map(|value| model_key(&value["data"]))
    .unwrap_or_default();
    Ok(())
}

fn refresh_agents(client: &Client, state: &mut Snapshot) -> Result<(), String> {
    let response = api(client, "GET", "/api/agent", None)?;
    let rows = response["data"]
        .as_array()
        .or_else(|| response.as_array())
        .ok_or("OpenCode returned an invalid agent list")?;
    let mut agents = Vec::new();
    for item in rows {
        let id = item["name"]
            .as_str()
            .or_else(|| item["id"].as_str())
            .unwrap_or("");
        if !valid_id(id) {
            continue;
        }
        let (managed, system) = read_managed_agent(id).unwrap_or_default();
        let model = item["model"]
            .as_str()
            .map(str::to_owned)
            .or_else(|| {
                let value = model_key(&item["model"]);
                (!value.is_empty()).then_some(value)
            })
            .unwrap_or_default();
        agents.push(Agent {
            id: id.to_owned(),
            description: item["description"].as_str().unwrap_or("").to_owned(),
            mode: item["mode"].as_str().unwrap_or("all").to_owned(),
            model,
            hidden: item["hidden"].as_bool().unwrap_or(false),
            managed,
            system,
        });
    }
    agents.sort_by_key(|left| left.id.to_lowercase());
    state.agents = agents;
    Ok(())
}

const ISLAND_AGENT_MARKER: &str = "<!-- Island-managed OpenCode profile -->";

fn opencode_config_root() -> Result<PathBuf, String> {
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .ok_or("Could not find the current user's home folder")?;
    Ok(PathBuf::from(home).join(".config").join("opencode"))
}

fn agent_directory(create: bool) -> Result<PathBuf, String> {
    let root = opencode_config_root()?;
    let directory = root.join("agents");
    if create {
        fs::create_dir_all(&directory).map_err(|_| "Could not create OpenCode's agent folder")?;
    }
    if !directory.exists() {
        return Ok(directory);
    }
    let canonical_root =
        fs::canonicalize(&root).map_err(|_| "Could not resolve OpenCode's configuration folder")?;
    let canonical_directory =
        fs::canonicalize(&directory).map_err(|_| "Could not resolve OpenCode's agent folder")?;
    if !canonical_directory.starts_with(&canonical_root) {
        return Err("OpenCode's agent folder resolves outside its configuration directory".into());
    }
    Ok(canonical_directory)
}

fn read_managed_agent(id: &str) -> Result<(bool, String), String> {
    if !valid_id(id) {
        return Ok((false, String::new()));
    }
    let path = agent_directory(false)?.join(format!("{id}.md"));
    let Ok(contents) = fs::read_to_string(path) else {
        return Ok((false, String::new()));
    };
    if !contents.contains(ISLAND_AGENT_MARKER) {
        return Ok((false, String::new()));
    }
    let system = contents
        .split_once("---\n")
        .and_then(|(_, after)| after.split_once("\n---"))
        .map(|(_, body)| body.trim_start_matches(['\r', '\n']))
        .unwrap_or("")
        .lines()
        .filter(|line| *line != ISLAND_AGENT_MARKER)
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_owned();
    Ok((true, system))
}

fn yaml_string(value: &str) -> String {
    let escaped = value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\r', "\\r")
        .replace('\n', "\\n");
    format!("\"{escaped}\"")
}

fn save_agent(
    state: &Snapshot,
    id: &str,
    description: &str,
    mode: &str,
    model: &str,
    system: &str,
) -> Result<(), String> {
    if !valid_id(id) || id.len() > 48 {
        return Err("Use a simple agent name with letters, numbers, dashes, or underscores".into());
    }
    if !matches!(mode, "primary" | "subagent" | "all") {
        return Err("Choose a supported agent role".into());
    }
    if description.trim().chars().count() > 500 || system.trim().chars().count() > 24_000 {
        return Err("The description or agent instructions are too long".into());
    }
    if description.trim().is_empty() && system.trim().is_empty() {
        return Err("Add a short description or instructions for this agent".into());
    }
    if !model.is_empty() && !state.models.iter().any(|item| item.key == model) {
        return Err(
            "That model is no longer available. Choose a current model before saving.".into(),
        );
    }
    if state
        .agents
        .iter()
        .any(|agent| agent.id == id && !agent.managed)
    {
        return Err("OpenCode already has an agent with that name. Choose another name so its configuration stays safe.".into());
    }
    let directory = agent_directory(true)?;
    let target = directory.join(format!("{id}.md"));
    if target.exists() {
        let contents =
            fs::read_to_string(&target).map_err(|_| "Could not read the existing agent profile")?;
        if !contents.contains(ISLAND_AGENT_MARKER) {
            return Err("An OpenCode agent already uses that name. Choose another name to avoid overwriting its file.".into());
        }
    }
    let mut source = format!(
        "---\ndescription: {}\nmode: {mode}\n",
        yaml_string(description.trim())
    );
    if !model.is_empty() {
        source.push_str(&format!("model: {}\n", yaml_string(model)));
    }
    source.push_str(&format!(
        "# {ISLAND_AGENT_MARKER}\n---\n\n{}\n",
        system.trim()
    ));
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temporary = directory.join(format!(".{id}.island-{}-{nonce}.tmp", std::process::id()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|_| "Could not prepare the agent profile file")?;
    if file
        .write_all(source.as_bytes())
        .and_then(|_| file.sync_all())
        .is_err()
    {
        let _ = fs::remove_file(&temporary);
        return Err("Could not write the agent profile".into());
    }
    drop(file);
    if target.exists() {
        let backup = directory.join(format!(".{id}.island-{}-{nonce}.bak", std::process::id()));
        fs::rename(&target, &backup).map_err(|_| {
            let _ = fs::remove_file(&temporary);
            "Could not replace the existing agent profile"
        })?;
        if fs::rename(&temporary, &target).is_err() {
            let _ = fs::rename(&backup, &target);
            let _ = fs::remove_file(&temporary);
            return Err("Could not replace the existing agent profile".into());
        }
        let _ = fs::remove_file(backup);
    } else if fs::rename(&temporary, &target).is_err() {
        let _ = fs::remove_file(&temporary);
        return Err("Could not finish writing the agent profile".into());
    }
    Ok(())
}

fn delete_agent(state: &Snapshot, id: &str) -> Result<(), String> {
    if !valid_id(id) {
        return Err("Choose a valid agent profile".into());
    }
    if state
        .sessions
        .iter()
        .any(|session| session.active && session.agent == id)
    {
        return Err("Finish runs using this agent before removing its profile".into());
    }
    let path = agent_directory(false)?.join(format!("{id}.md"));
    let contents = fs::read_to_string(&path)
        .map_err(|_| "That Pluto agent profile no longer exists".to_owned())?;
    if !contents.contains(ISLAND_AGENT_MARKER) {
        return Err("Pluto can only remove profiles it created".into());
    }
    fs::remove_file(path).map_err(|_| "Could not remove the agent profile".into())
}

fn refresh(
    client: &Client,
    state: &mut Snapshot,
    selected_session: &str,
    loaded_count: usize,
) -> Result<(), String> {
    let sessions = api(client, "GET", "/api/session?limit=40&order=desc", None)?;
    let active = api(client, "GET", "/api/session/active", None)?;
    let active_map = active["data"].as_object();
    state.active_count = active_map.map_or(0, |m| m.len());
    let mut newest: Vec<Session> = sessions["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|s| {
            let id = s["id"].as_str()?.to_owned();
            Some(Session {
                title: s["title"].as_str().unwrap_or("Untitled session").to_owned(),
                directory: s["location"]["directory"].as_str().unwrap_or("").to_owned(),
                active: active_map.is_some_and(|map| map.contains_key(&id)),
                updated: s["time"]["updated"].as_i64().unwrap_or(0),
                id,
                model: model_key(&s["model"]),
                agent: s["agent"].as_str().unwrap_or("").to_owned(),
            })
        })
        .collect();
    let new_cursor = sessions["cursor"]["next"].as_str().map(str::to_owned);
    let retained = loaded_count > 40;
    for old in std::mem::take(&mut state.sessions) {
        if !newest.iter().any(|s| s.id == old.id)
            && (newest.len() < loaded_count || active_map.is_some_and(|m| m.contains_key(&old.id)))
        {
            newest.push(Session {
                active: active_map.is_some_and(|m| m.contains_key(&old.id)),
                ..old
            });
        }
    }
    state.sessions = newest;
    if !retained {
        state.next_cursor = new_cursor;
    }
    let mut ids: Vec<String> = active_map
        .into_iter()
        .flat_map(|map| map.keys().take(24).cloned())
        .collect();
    for id in &ids {
        if state.sessions.iter().any(|session| &session.id == id) {
            continue;
        }
        if let Ok(detail) = api(client, "GET", &format!("/api/session/{id}"), None) {
            let s = &detail["data"];
            state.sessions.insert(
                0,
                Session {
                    id: id.clone(),
                    title: s["title"].as_str().unwrap_or("Active session").to_owned(),
                    directory: s["location"]["directory"].as_str().unwrap_or("").to_owned(),
                    updated: s["time"]["updated"].as_i64().unwrap_or(0),
                    active: true,
                    model: model_key(&s["model"]),
                    agent: s["agent"].as_str().unwrap_or("").to_owned(),
                },
            );
        }
    }
    if valid_id(selected_session) && !ids.iter().any(|id| id == selected_session) {
        ids.push(selected_session.to_owned());
    }
    let mut pending = Vec::new();
    for id in ids {
        if !valid_id(&id) {
            continue;
        }
        let Ok(response) = api(
            client,
            "GET",
            &format!("/api/session/{id}/permission"),
            None,
        ) else {
            continue;
        };
        for p in response["data"].as_array().into_iter().flatten() {
            if let Some(request_id) = p["id"].as_str() {
                pending.push(Permission {
                    id: request_id.to_owned(),
                    session_id: id.clone(),
                    action: p["action"]
                        .as_str()
                        .unwrap_or("Approval requested")
                        .to_owned(),
                    resources: p["resources"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|r| r.as_str().map(str::to_owned))
                        .collect(),
                });
            }
        }
    }
    state.permissions = pending;
    if valid_id(selected_session) {
        match latest_response(client, selected_session) {
            Ok(response) => {
                state.response = Some(response);
                state.response_error.clear();
            }
            Err(error) => {
                state.response = None;
                state.response_error = error;
            }
        }
    } else {
        state.response = None;
        state.response_error.clear();
    }
    Ok(())
}

fn latest_response(client: &Client, session_id: &str) -> Result<ResponsePreview, String> {
    let assistant = api(
        client,
        "GET",
        &format!("/api/session/{session_id}/message?type=assistant&limit=1&order=desc"),
        None,
    )?;
    let user = api(
        client,
        "GET",
        &format!("/api/session/{session_id}/message?type=user&limit=1&order=desc"),
        None,
    )?;
    let assistant = assistant["data"]
        .as_array()
        .and_then(|messages| messages.first());
    let user = user["data"]
        .as_array()
        .and_then(|messages| messages.first());
    let assistant_info = assistant.map(|item| item.get("info").unwrap_or(item));
    let user_info = user.map(|item| item.get("info").unwrap_or(item));
    let assistant_at = assistant_info
        .and_then(|item| item["time"]["created"].as_i64())
        .unwrap_or(0);
    let user_at = user_info
        .and_then(|item| item["time"]["created"].as_i64())
        .unwrap_or(0);
    let waiting = assistant.is_none() || user_at > assistant_at;
    let mut text = String::new();
    let mut truncated = false;
    let mut steps = Vec::new();
    if !waiting {
        for part in assistant
            .into_iter()
            .flat_map(|item| {
                item["parts"]
                    .as_array()
                    .or_else(|| item["content"].as_array())
            })
            .flatten()
        {
            match part["type"].as_str().unwrap_or("") {
                "text" => {
                    let Some(content) = part["text"].as_str() else {
                        continue;
                    };
                    if !text.is_empty() {
                        text.push_str("\n\n");
                    }
                    let space = 50_000usize.saturating_sub(text.chars().count());
                    let part_length = content.chars().count();
                    text.extend(content.chars().take(space));
                    if part_length > space {
                        truncated = true;
                        break;
                    }
                }
                // OpenCode exposes tool progress as public message parts. Keep only
                // their short labels and status; never forward arguments or output.
                "tool" if steps.len() < 16 => {
                    let tool = part["tool"].as_str().unwrap_or("Tool");
                    let state = &part["state"];
                    let status = match state["status"].as_str().unwrap_or("pending") {
                        "running" => "active",
                        "completed" => "complete",
                        "error" => "error",
                        _ => "pending",
                    };
                    let title = state["title"]
                        .as_str()
                        .filter(|value| !value.trim().is_empty())
                        .map(str::to_owned)
                        .unwrap_or_else(|| tool_label(tool));
                    let detail = if status == "error" {
                        bounded_text(state["error"].as_str().unwrap_or("Tool failed"), 220)
                    } else {
                        String::new()
                    };
                    steps.push(ResponseStep {
                        id: part["id"].as_str().unwrap_or(tool).to_owned(),
                        title,
                        detail,
                        status: status.into(),
                    });
                }
                "subtask" if steps.len() < 16 => {
                    let agent = part["agent"].as_str().unwrap_or("Subagent");
                    let detail = part["description"].as_str().unwrap_or("");
                    steps.push(ResponseStep {
                        id: part["id"].as_str().unwrap_or(agent).to_owned(),
                        title: format!("{agent} task"),
                        detail: bounded_text(detail, 180),
                        status: if assistant_info
                            .is_some_and(|item| item["time"]["completed"].is_null())
                        {
                            "active"
                        } else {
                            "complete"
                        }
                        .into(),
                    });
                }
                // Reasoning parts are deliberately omitted. The activity view shows
                // observable work and plans, not private model reasoning.
                _ => {}
            }
        }
    }
    let plan = api(
        client,
        "GET",
        &format!("/api/session/{session_id}/todo"),
        None,
    )
    .ok()
    .and_then(|value| {
        value
            .get("data")
            .and_then(Value::as_array)
            .cloned()
            .or_else(|| value.as_array().cloned())
    })
    .into_iter()
    .flatten()
    .take(24)
    .enumerate()
    .filter_map(|(index, item)| {
        let content = item["content"].as_str()?;
        if content.trim().is_empty() {
            return None;
        }
        let status = match item["status"].as_str().unwrap_or("pending") {
            "completed" => "complete",
            "in_progress" => "active",
            "cancelled" => "cancelled",
            _ => "pending",
        };
        Some(ResponsePlanItem {
            id: item["id"]
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| index.to_string()),
            content: bounded_text(content, 220),
            status: status.into(),
        })
    })
    .collect();
    Ok(ResponsePreview {
        session_id: session_id.to_owned(),
        text,
        truncated,
        waiting,
        plan,
        steps,
    })
}

fn bounded_text(value: &str, limit: usize) -> String {
    value
        .chars()
        .filter(|ch| !ch.is_control() || *ch == '\n' || *ch == '\t')
        .take(limit)
        .collect()
}

fn tool_label(value: &str) -> String {
    value
        .replace(['_', '-'], " ")
        .split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            chars
                .next()
                .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn apply_action(client: &Client, state: &mut Snapshot, action: Action) -> Result<(), String> {
    match action {
        Action::Refresh => {
            state.notice.clear();
        }
        Action::More => {
            let Some(cursor) = state.next_cursor.clone() else {
                return Ok(());
            };
            let path = format!("/api/session?limit=40&cursor={}", url_encode(&cursor));
            let page = api(client, "GET", &path, None)?;
            for s in page["data"].as_array().into_iter().flatten() {
                let Some(id) = s["id"].as_str() else {
                    continue;
                };
                if state.sessions.iter().any(|item| item.id == id) {
                    continue;
                }
                state.sessions.push(Session {
                    id: id.to_owned(),
                    title: s["title"].as_str().unwrap_or("Untitled session").to_owned(),
                    directory: s["location"]["directory"].as_str().unwrap_or("").to_owned(),
                    updated: s["time"]["updated"].as_i64().unwrap_or(0),
                    active: false,
                    model: model_key(&s["model"]),
                    agent: s["agent"].as_str().unwrap_or("").to_owned(),
                });
            }
            state.next_cursor = page["cursor"]["next"].as_str().map(str::to_owned);
            state.notice.clear();
        }
        Action::Select(_)
        | Action::Install
        | Action::Update
        | Action::CheckUpdate
        | Action::Catalog(_) => {}
        Action::RefreshAgents => {
            refresh_agents(client, state)?;
            state.agent_error.clear();
        }
        Action::SaveAgent {
            id,
            description,
            mode,
            model,
            system,
        } => {
            save_agent(state, &id, &description, &mode, &model, &system)?;
            refresh_agents(client, state)?;
            state.agent_error.clear();
            state.notice = format!("{id} profile saved");
        }
        Action::DeleteAgent { id } => {
            delete_agent(state, &id)?;
            refresh_agents(client, state)?;
            state.agent_error.clear();
            state.notice = format!("{id} profile removed");
        }
        Action::Start {
            directory,
            text,
            model,
            agent,
            files,
        } => {
            let prompt = text.trim();
            if prompt.is_empty() && files.is_empty() {
                return Err("Enter a task or attach a file".into());
            }
            let attachments = checked_attachments(&files)?;
            let title: String = prompt
                .lines()
                .next()
                .filter(|line| !line.is_empty())
                .unwrap_or("Review attached files")
                .chars()
                .take(72)
                .collect();
            let mut body = json!({ "title": title });
            if let Some(directory) = directory.filter(|value| !value.trim().is_empty()) {
                let path = std::path::Path::new(&directory);
                if !path.is_absolute() || !path.is_dir() {
                    return Err("Choose an existing project folder".into());
                }
                body["location"] = json!({ "directory": directory });
            }
            if let Some(reference) = checked_model(state, model.as_deref())? {
                body["model"] = reference;
            }
            if let Some(agent) = checked_primary_agent(state, agent.as_deref())? {
                body["agent"] = json!(agent);
            }
            let created = api(client, "POST", "/api/session", Some(&body))?;
            let id = created["data"]["id"]
                .as_str()
                .filter(|id| valid_id(id))
                .ok_or("OpenCode did not return a session ID")?
                .to_owned();
            state.last_created_id = id.clone();
            // OpenCode v2 replaced the legacy prompt_async endpoint with
            // /prompt. The v2 route accepts `{ text }` and acknowledges the
            // queued turn before generation finishes.
            let mentions = prompt_agent_mentions(prompt, &state.agents);
            let prompt_body =
                json!({ "prompt": { "text": prompt, "files": attachments, "agents": mentions } });
            let result = api(
                client,
                "POST",
                &format!("/api/session/{id}/prompt"),
                Some(&prompt_body),
            );
            let result = if files.is_empty() && result.is_err() {
                api(
                    client,
                    "POST",
                    &format!("/api/session/{id}/prompt"),
                    Some(&json!({ "text": prompt })),
                )
            } else {
                result
            };
            result
                .map_err(|error| format!("Session was created, but the instruction was not sent: {error}. Choose it from Recent sessions to retry."))?;
            state.notice.clear();
        }
        Action::Prompt {
            session_id,
            text,
            agent,
            files,
        } => {
            if !valid_id(&session_id) || (text.trim().is_empty() && files.is_empty()) {
                return Err("Select a session and enter a task or attach a file".into());
            }
            let attachments = checked_attachments(&files)?;
            if let Some(agent) = checked_primary_agent(state, agent.as_deref())?.map(str::to_owned)
                && !state
                    .sessions
                    .iter()
                    .any(|session| session.id == session_id && session.agent == agent)
            {
                api(
                    client,
                    "POST",
                    &format!("/api/session/{session_id}/agent"),
                    Some(&json!({ "agent": agent })),
                )?;
                if let Some(session) = state
                    .sessions
                    .iter_mut()
                    .find(|session| session.id == session_id)
                {
                    session.agent = agent;
                }
            }
            let path = format!("/api/session/{session_id}/prompt");
            let prompt_text = text.trim();
            let mentions = prompt_agent_mentions(prompt_text, &state.agents);
            if !files.is_empty() {
                let body = json!({ "prompt": { "text": prompt_text, "files": attachments, "agents": mentions } });
                let _ = api(client, "POST", &path, Some(&body))?;
            } else if mentions.is_empty() {
                let _ = api(client, "POST", &path, Some(&json!({ "text": prompt_text })))?;
            } else {
                let body = json!({ "prompt": { "text": prompt_text, "agents": mentions } });
                if let Err(error) = api(client, "POST", &path, Some(&body)) {
                    let fallback =
                        api(client, "POST", &path, Some(&json!({ "text": prompt_text })));
                    fallback.map_err(|_| error)?;
                }
            }
            state.response = None;
            state.notice.clear();
        }
        Action::Abort { session_id } => {
            if !valid_id(&session_id) {
                return Err("Select a valid OpenCode session".into());
            }
            let aborted = api(
                client,
                "POST",
                &format!("/api/session/{session_id}/abort"),
                None,
            )?;
            if aborted.as_bool() == Some(false) {
                return Err("This OpenCode session is no longer running".into());
            }
            state.notice = "OpenCode run stopped".into();
        }
        Action::SetModel { session_id, model } => {
            if !valid_id(&session_id) {
                return Err("Invalid session".into());
            }
            let reference = checked_model(state, Some(&model))?.ok_or("Choose a model")?;
            let _ = api(
                client,
                "POST",
                &format!("/api/session/{session_id}/model"),
                Some(&json!({ "model": reference })),
            )?;
            if let Some(session) = state
                .sessions
                .iter_mut()
                .find(|session| session.id == session_id)
            {
                session.model = model;
            }
            state.notice = "Session model changed".into();
        }
        Action::Reply {
            session_id,
            request_id,
            allow,
        } => {
            if !valid_id(&session_id) || !valid_id(&request_id) {
                return Err("Invalid approval request".into());
            }
            let path = format!("/api/session/{session_id}/permission/{request_id}");
            let pending = api(client, "GET", &path, None)?;
            if pending["data"]["id"].as_str() != Some(request_id.as_str()) {
                return Err("Approval is no longer pending".into());
            }
            let decision = if allow { "once" } else { "reject" };
            let _ = api(
                client,
                "POST",
                &format!("{path}/reply"),
                Some(&json!({ "decision": decision })),
            )?;
            state.notice = if allow { "Allowed once" } else { "Rejected" }.into();
        }
    }
    Ok(())
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_'))
}
fn checked_attachments(paths: &[String]) -> Result<Vec<Value>, String> {
    if paths.len() > 8 {
        return Err("Attach up to 8 files at a time".into());
    }
    let mut total = 0_u64;
    paths
        .iter()
        .map(|raw| {
            let path = std::path::Path::new(raw);
            if !path.is_absolute() || !path.is_file() {
                return Err("An attached file is no longer available".into());
            }
            let size = path
                .metadata()
                .map_err(|_| "Could not read an attached file")?
                .len();
            total += size;
            if total > 20 * 1024 * 1024 {
                return Err("Attachments must total 20 MB or less".into());
            }
            let extension = path
                .extension()
                .and_then(|ext| ext.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if !matches!(extension.as_str(), "png" | "jpg" | "jpeg" | "gif" | "webp") {
                let bytes = std::fs::read(path).map_err(|_| "Could not read an attached file")?;
                std::str::from_utf8(&bytes)
                    .map_err(|_| "Attach a UTF-8 text file or PNG, JPEG, GIF, or WebP image")?;
            }
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("Attached file");
            let normalized = path.to_string_lossy().replace('\\', "/");
            let uri = format!(
                "file:///{}",
                normalized
                    .bytes()
                    .map(|byte| {
                        if byte.is_ascii_alphanumeric()
                            || matches!(byte, b'/' | b':' | b'-' | b'_' | b'.' | b'~')
                        {
                            (byte as char).to_string()
                        } else {
                            format!("%{byte:02X}")
                        }
                    })
                    .collect::<String>()
            );
            Ok(json!({ "uri": uri, "name": name }))
        })
        .collect()
}
fn url_encode(value: &str) -> String {
    value
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_') {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}
fn api(client: &Client, method: &str, path: &str, body: Option<&Value>) -> Result<Value, String> {
    let payload = body.map(|v| v.to_string());
    let (status, bytes) = request(
        client.port,
        Some(&client.password),
        method,
        path,
        payload.as_deref(),
    )?;
    if status == 204 {
        return Ok(Value::Null);
    }
    if !(200..300).contains(&status) {
        let detail = serde_json::from_slice::<Value>(&bytes)
            .ok()
            .and_then(|value| value["message"].as_str().map(str::to_owned))
            .map(|message| {
                message
                    .chars()
                    .filter(|ch| !ch.is_control())
                    .take(180)
                    .collect::<String>()
            })
            .filter(|message| !message.is_empty());
        return Err(match status {
            400 => detail
                .map(|message| format!("OpenCode rejected the request: {message}"))
                .unwrap_or_else(|| "OpenCode rejected the request (HTTP 400)".into()),
            401 => "OpenCode connection expired".into(),
            404 => "OpenCode session or approval no longer exists".into(),
            409 => "OpenCode session is busy".into(),
            _ => format!("OpenCode returned HTTP {status}"),
        });
    }
    serde_json::from_slice(&bytes).map_err(|_| "Invalid OpenCode API response".into())
}

fn request(
    port: u16,
    password: Option<&str>,
    method: &str,
    path: &str,
    body: Option<&str>,
) -> Result<(u16, Vec<u8>), String> {
    let address = SocketAddrV4::new(Ipv4Addr::LOCALHOST, port);
    let mut stream = TcpStream::connect_timeout(&address.into(), Duration::from_secs(2))
        .map_err(|_| "OpenCode local service is unavailable".to_owned())?;
    stream.set_read_timeout(Some(Duration::from_secs(4))).ok();
    stream.set_write_timeout(Some(Duration::from_secs(4))).ok();
    let basic = password.map(|p| base64(format!("opencode:{p}").as_bytes()));
    let auth = basic
        .map(|v| format!("Authorization: Basic {v}\r\n"))
        .unwrap_or_default();
    let payload = body.unwrap_or("");
    let header = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAccept: application/json\r\nContent-Type: application/json\r\n{auth}Content-Length: {}\r\nConnection: close\r\n\r\n",
        payload.len()
    );
    stream
        .write_all(header.as_bytes())
        .and_then(|_| stream.write_all(payload.as_bytes()))
        .map_err(|_| "OpenCode request failed".to_owned())?;
    let mut bytes = Vec::new();
    stream
        .take(4 * 1024 * 1024)
        .read_to_end(&mut bytes)
        .map_err(|_| "OpenCode response timed out".to_owned())?;
    let split = bytes
        .windows(4)
        .position(|v| v == b"\r\n\r\n")
        .ok_or("Invalid OpenCode HTTP response")?;
    let head = std::str::from_utf8(&bytes[..split]).map_err(|_| "Invalid OpenCode headers")?;
    let status = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|v| v.parse::<u16>().ok())
        .ok_or("Invalid OpenCode status")?;
    let body = &bytes[split + 4..];
    let chunked = head.lines().any(|line| {
        line.to_ascii_lowercase()
            .contains("transfer-encoding: chunked")
    });
    if !chunked {
        return Ok((status, body.to_vec()));
    }
    let mut data = Vec::new();
    let mut remaining = body;
    loop {
        let end = remaining
            .windows(2)
            .position(|v| v == b"\r\n")
            .ok_or("Invalid chunked response")?;
        let size = std::str::from_utf8(&remaining[..end])
            .ok()
            .and_then(|v| usize::from_str_radix(v.split(';').next().unwrap_or(""), 16).ok())
            .ok_or("Invalid chunk size")?;
        remaining = &remaining[end + 2..];
        if size == 0 {
            break;
        }
        if remaining.len() < size + 2 || data.len() + size > 4 * 1024 * 1024 {
            return Err("OpenCode response too large".into());
        }
        data.extend_from_slice(&remaining[..size]);
        remaining = &remaining[size + 2..];
    }
    Ok((status, data))
}

fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let a = chunk[0];
        let b = *chunk.get(1).unwrap_or(&0);
        let c = *chunk.get(2).unwrap_or(&0);
        out.push(TABLE[(a >> 2) as usize] as char);
        out.push(TABLE[(((a & 3) << 4) | (b >> 4)) as usize] as char);
        out.push(if chunk.len() > 1 {
            TABLE[(((b & 15) << 2) | (c >> 6)) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[(c & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}
