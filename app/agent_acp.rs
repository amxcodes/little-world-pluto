//! ACP v1 client for the two supported agent providers. Provider processes and
//! credentials stay in Rust; the WebView receives a bounded, typed projection.
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{
    Arc, Mutex,
    mpsc::{self, Receiver, Sender},
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::agent_presentation::AgentPresentation;
use base64::Engine;
use serde::Serialize;
use serde_json::{Value, json};

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub provider: String,
    pub connected: bool,
    pub missing: bool,
    pub installing: bool,
    pub authenticating: bool,
    pub error: String,
    pub notice: String,
    pub notice_severity: String,
    pub auth_required: bool,
    pub version: String,
    pub adapter_version: String,
    pub latest_version: String,
    pub protocol_version: u64,
    pub update_available: bool,
    pub checking_update: bool,
    pub executable: String,
    pub capabilities: Value,
    pub auth_methods: Vec<Value>,
    pub sessions: Vec<Session>,
    pub selected_id: String,
    pub response: Option<Arc<Response>>,
    #[serde(skip)]
    pub last_finished_response: Option<Arc<Response>>,
    pub permissions: Vec<Permission>,
    pub questions: Vec<Question>,
    pub active_count: usize,
    pub subagents: Vec<Subagent>,
    pub operation_revision: u64,
    pub operation_kind: String,
    pub operation_ok: bool,
    pub last_created_id: String,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub id: String,
    pub title: String,
    pub directory: String,
    pub additional_directories: Vec<String>,
    pub active: bool,
    pub ready: bool,
    pub updated: String,
    pub config_options: Vec<Value>,
    pub modes: Option<Value>,
    pub commands: Vec<Value>,
    pub child_count: usize,
}

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Subagent {
    pub id: String,
    pub parent_id: String,
    pub name: String,
    pub state: String,
}

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Response {
    pub session_id: String,
    pub text: String,
    pub truncated: bool,
    pub waiting: bool,
    pub steps: Vec<Step>,
    pub plan: Vec<Value>,
    pub usage: Option<Value>,
    pub compactions: Vec<Value>,
    pub stop_reason: String,
    pub meta: Value,
    pub presentation: Option<AgentPresentation>,
    #[serde(skip)]
    pub visual_policy: crate::agent_presentation::VisualPolicy,
}

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    pub id: String,
    pub title: String,
    pub detail: String,
    pub status: String,
    pub kind: String,
    pub content: Vec<Value>,
    pub meta: Value,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Permission {
    pub id: String,
    pub session_id: String,
    pub owner_session_id: String,
    pub action: String,
    pub detail: String,
    pub options: Vec<Value>,
    pub meta: Value,
    pub raised_at: u128,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Question {
    pub id: String,
    pub session_id: String,
    pub owner_session_id: String,
    pub message: String,
    pub schema: Value,
    pub mode: String,
    pub url: String,
    pub elicitation_id: String,
    pub raised_at: u128,
}

struct TerminalCapture {
    text: String,
    byte_limit: usize,
    truncated: bool,
}

struct TerminalProcess {
    child: Child,
    session_id: String,
    capture: Arc<Mutex<TerminalCapture>>,
    readers: Vec<JoinHandle<()>>,
    waiters: Vec<Value>,
    exit_code: Option<i32>,
    exited: bool,
}

#[derive(Clone)]
struct CodexCandidate {
    directory: PathBuf,
    version: String,
}

const MAX_ACP_LINE_BYTES: usize = 24_000_000;
const MAX_TERMINAL_WAITERS: usize = 128;
const MAX_TERMINAL_HISTORY_BYTES: usize = 8_000_000;

pub enum Action {
    Connect,
    Install,
    CheckUpdate,
    Refresh,
    More,
    Authenticate(String),
    Logout,
    Select(String),
    Prepare {
        directory: String,
        additional_directories: Vec<String>,
        config: Vec<(String, String)>,
    },
    Start {
        directory: String,
        additional_directories: Vec<String>,
        text: String,
        config: Vec<(String, String)>,
        files: Vec<String>,
        system_prompt: String,
        hybrid_whiteboard: bool,
    },
    Prompt {
        session_id: String,
        text: String,
        files: Vec<String>,
        system_prompt: String,
        hybrid_whiteboard: bool,
    },
    Cancel(String),
    Close(String),
    Delete(String),
    Fork(String),
    Configure {
        session_id: String,
        id: String,
        value: String,
    },
    SetMode {
        session_id: String,
        mode: String,
    },
    Reply {
        session_id: String,
        request_id: String,
        option_id: String,
    },
    Answer {
        session_id: String,
        request_id: String,
        choice: String,
        content: Value,
    },
}

enum Pending {
    Initialize,
    List {
        append: bool,
    },
    New {
        prepare: bool,
        text: String,
        config: Vec<(String, String)>,
        files: Vec<String>,
        directory: String,
        additional_directories: Vec<String>,
        system_prompt: String,
        hybrid_whiteboard: bool,
    },
    ConfigureStart {
        prepare: bool,
        session_id: String,
        text: String,
        rest: Vec<(String, String)>,
        files: Vec<String>,
        system_prompt: String,
        hybrid_whiteboard: bool,
    },
    Configure {
        session_id: String,
    },
    Load {
        session_id: String,
    },
    Prompt {
        session_id: String,
    },
    Authenticate,
    Logout,
    SetMode {
        session_id: String,
    },
    Resume {
        session_id: String,
    },
    Close {
        session_id: String,
    },
    Delete {
        session_id: String,
    },
    Fork {
        source_id: String,
        directory: String,
    },
}

pub fn start(provider: &'static str) -> (Sender<Action>, pluto::latest::Receiver<Snapshot>) {
    let (actions, incoming) = mpsc::channel();
    let (outgoing, snapshots) = pluto::latest::channel();
    let _ = thread::Builder::new()
        .name(format!("island-acp-{provider}"))
        .spawn(move || Worker::new(provider, incoming, outgoing).run());
    (actions, snapshots)
}

struct Worker {
    provider: &'static str,
    actions: Receiver<Action>,
    outgoing: pluto::latest::Sender<Snapshot>,
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    messages: Option<Receiver<Value>>,
    snapshot: Snapshot,
    auth_methods: Vec<Value>,
    pending: HashMap<u64, Pending>,
    loaded: HashSet<String>,
    cached_responses: HashMap<String, Arc<Response>>,
    load_fallback: HashMap<String, Arc<Response>>,
    prompt_fallback: HashMap<String, Arc<Response>>,
    message_ids: HashMap<String, String>,
    active_children: HashMap<String, String>,
    child_names: HashMap<String, String>,
    child_states: HashMap<String, String>,
    install_rx: Option<Receiver<Result<CodexCandidate, String>>>,
    candidate_codex: Option<CodexCandidate>,
    auth_rx: Option<Receiver<Result<(), String>>>,
    version_rx: Option<Receiver<Option<String>>>,
    terminals: HashMap<String, TerminalProcess>,
    terminal_history: HashMap<String, String>,
    next_id: u64,
    last_list_applied: u64,
    retry_at: Instant,
    should_reconnect: bool,
    dirty: bool,
    last_publish: Instant,
    last_version_check: Instant,
    next_terminal_id: u64,
}

impl Worker {
    fn new(
        provider: &'static str,
        actions: Receiver<Action>,
        outgoing: pluto::latest::Sender<Snapshot>,
    ) -> Self {
        Self {
            provider,
            actions,
            outgoing,
            child: None,
            stdin: None,
            messages: None,
            snapshot: Snapshot {
                provider: provider.into(),
                ..Snapshot::default()
            },
            auth_methods: Vec::new(),
            pending: HashMap::new(),
            loaded: HashSet::new(),
            cached_responses: HashMap::new(),
            load_fallback: HashMap::new(),
            prompt_fallback: HashMap::new(),
            message_ids: HashMap::new(),
            active_children: HashMap::new(),
            child_names: HashMap::new(),
            child_states: HashMap::new(),
            install_rx: None,
            candidate_codex: None,
            auth_rx: None,
            version_rx: None,
            terminals: HashMap::new(),
            terminal_history: HashMap::new(),
            next_id: 1,
            next_terminal_id: 1,
            last_list_applied: 0,
            retry_at: Instant::now(),
            dirty: true,
            should_reconnect: false,
            last_publish: Instant::now() - Duration::from_secs(1),
            last_version_check: Instant::now() - Duration::from_secs(6 * 60 * 60),
        }
    }

    fn run(&mut self) {
        loop {
            if self.child.is_none() && self.should_reconnect && Instant::now() >= self.retry_at {
                self.connect();
            }
            while let Ok(action) = self.actions.try_recv() {
                self.handle_action(action);
            }
            if let Some(result) = self.install_rx.as_ref().and_then(|rx| rx.try_recv().ok()) {
                self.install_rx = None;
                self.snapshot.installing = false;
                match result {
                    Ok(candidate) => {
                        self.snapshot.notice = format!(
                            "Checking Codex ACP {} against protocol v1…",
                            candidate.version
                        );
                        self.candidate_codex = Some(candidate);
                        self.handle_action(Action::Connect);
                    }
                    Err(error) => self.snapshot.error = error,
                }
                self.dirty = true;
            }
            if let Some(result) = self.auth_rx.as_ref().and_then(|rx| rx.try_recv().ok()) {
                self.auth_rx = None;
                self.snapshot.authenticating = false;
                match result {
                    Ok(()) => {
                        self.complete_operation("auth", true, "Sign-in complete. Reconnecting…");
                        self.handle_action(Action::Connect);
                    }
                    Err(error) => {
                        self.handle_action(Action::Connect);
                        self.fail_operation("auth", &error);
                    }
                }
            }
            if let Some(version) = self.version_rx.as_ref().and_then(|rx| rx.try_recv().ok()) {
                self.version_rx = None;
                self.snapshot.checking_update = false;
                if let Some(version) = version {
                    self.snapshot.latest_version = version;
                    self.snapshot.update_available = newer_version(
                        &self.snapshot.latest_version,
                        &self.snapshot.adapter_version,
                    );
                }
                self.dirty = true;
            }
            if self.provider == "codex"
                && self.version_rx.is_none()
                && self.last_version_check.elapsed() >= Duration::from_secs(6 * 60 * 60)
            {
                self.check_latest_version();
            }
            let incoming = self
                .messages
                .as_ref()
                .and_then(|rx| rx.recv_timeout(Duration::from_millis(30)).ok());
            if self.messages.is_none() {
                thread::sleep(Duration::from_millis(30));
            }
            if let Some(message) = incoming {
                self.handle_message(message);
            }
            if let Some(child) = self.child.as_mut()
                && child.try_wait().ok().flatten().is_some()
            {
                self.disconnect("Agent process stopped. Reconnect to continue.");
            }
            self.poll_terminals();
            self.publish(false);
        }
    }

    fn check_latest_version(&mut self) {
        if self.provider != "codex" || self.snapshot.checking_update || self.snapshot.installing {
            return;
        }
        self.snapshot.checking_update = true;
        self.last_version_check = Instant::now();
        let (tx, rx) = mpsc::channel();
        self.version_rx = Some(rx);
        self.dirty = true;
        let _ = thread::Builder::new()
            .name("island-codex-acp-version".into())
            .spawn(move || {
                let _ = tx.send(latest_codex_acp_version());
            });
    }

    fn connect(&mut self) {
        self.retry_at = Instant::now() + Duration::from_secs(10);
        let mut command = if self.provider == "codex" {
            self.candidate_codex
                .as_ref()
                .and_then(|candidate| codex_command_in(&candidate.directory))
        } else {
            None
        };
        if self.provider == "codex" && self.candidate_codex.is_some() && command.is_none() {
            if let Some(candidate) = self.candidate_codex.take() {
                remove_codex_candidate(&candidate);
            }
            self.snapshot.notice =
                "The staged Codex ACP files were incomplete. Keeping the installed provider."
                    .into();
        }
        if command.is_none() {
            command = provider_command(self.provider);
        }
        let Some((program, args)) = command else {
            self.snapshot.missing = true;
            self.snapshot.error = if self.provider == "codex" {
                "Codex ACP is not installed. Install @agentclientprotocol/codex-acp, then reconnect.".into()
            } else {
                "OpenCode CLI is not installed.".into()
            };
            self.dirty = true;
            return;
        };
        let executable = if self.provider == "codex" {
            args.first().cloned().unwrap_or_default()
        } else {
            program.to_string_lossy().into_owned()
        };
        if self.provider == "codex" {
            self.snapshot.adapter_version =
                codex_version_from_script(&executable).unwrap_or_default();
        }
        match Command::new(&program)
            .args(&args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .creation_flags_no_window()
            .spawn()
        {
            Ok(mut child) => {
                let Some(stdout) = child.stdout.take() else {
                    let _ = child.kill();
                    let _ = child.wait();
                    self.disconnect("ACP output is unavailable");
                    return;
                };
                let (tx, rx) = mpsc::channel();
                thread::spawn(move || read_acp_messages(stdout, tx));
                self.stdin = child.stdin.take();
                self.child = Some(child);
                self.should_reconnect = false;
                self.messages = Some(rx);
                self.snapshot.error.clear();
                self.snapshot.missing = false;
                self.snapshot.executable = executable;
                let mut client_capabilities = json!({
                    "fs": {"readTextFile": true, "writeTextFile": true},
                    "terminal": true,
                    "session": {"configOptions": {"boolean": {}}, "notices": {}, "compaction": {}},
                    "subagents": {},
                    "auth": {"terminal": true},
                    "elicitation": {"form": {}, "url": {}}
                });
                if self.provider == "opencode" {
                    client_capabilities["_meta"] = json!({"terminal-auth":true});
                }
                let params = json!({
                    "protocolVersion": ACP_PROTOCOL_VERSION,
                    "clientCapabilities": client_capabilities,
                    "clientInfo": {"name": "pluto", "title": "Pluto", "version": "0.1.0"}
                });
                self.request("initialize", params, Pending::Initialize);
            }
            Err(error) => {
                self.snapshot.error = format!("Could not start ACP: {error}");
                self.should_reconnect = true;
                if self.candidate_codex.is_some() {
                    self.disconnect(&self.snapshot.error.clone());
                }
                self.dirty = true;
            }
        }
    }

    fn disconnect(&mut self, error: &str) {
        let failed_candidate = if error.is_empty() {
            None
        } else {
            self.candidate_codex.take()
        };
        let interrupted_prompt = self
            .pending
            .values()
            .any(|pending| matches!(pending, Pending::Prompt { .. }));
        let interrupted_start = self.pending.values().any(|pending| {
            matches!(
                pending,
                Pending::New { .. } | Pending::ConfigureStart { .. }
            )
        });
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Some(candidate) = failed_candidate {
            remove_codex_candidate(&candidate);
            self.snapshot.notice = format!(
                "Codex ACP {} failed ACP v1 compatibility. The previous provider version is being restored.",
                candidate.version
            );
            self.snapshot.notice_severity = "warning".into();
        }
        self.stop_all_terminals();
        self.stdin = None;
        self.messages = None;
        self.pending.clear();
        self.loaded.clear();
        for state in self.child_states.values_mut() {
            if matches!(
                state.as_str(),
                "running" | "requires_action" | "unknown" | "in_progress"
            ) {
                *state = "disconnected".into();
            }
        }
        self.refresh_subagents();
        self.snapshot.permissions.clear();
        self.snapshot.questions.clear();
        for (session_id, previous) in self.load_fallback.drain() {
            self.cached_responses
                .insert(session_id.clone(), previous.clone());
            if self.snapshot.selected_id == session_id {
                self.snapshot.response = Some(previous);
            }
        }
        for (session_id, previous) in self.prompt_fallback.drain() {
            if self
                .cached_responses
                .get(&session_id)
                .is_none_or(|response| response.text.is_empty())
            {
                self.cached_responses
                    .insert(session_id.clone(), previous.clone());
                if self.snapshot.selected_id == session_id {
                    self.snapshot.response = Some(previous);
                }
            }
        }
        self.snapshot.connected = false;
        self.snapshot.error = error.into();
        if interrupted_prompt {
            self.fail_operation("prompt", error);
        } else if interrupted_start {
            self.fail_operation("start", error);
        }
        for session in &mut self.snapshot.sessions {
            session.active = false;
            session.ready = false;
        }
        self.snapshot.active_count = 0;
        if let Some(response) = &mut self.snapshot.response {
            let response = Arc::make_mut(response);
            response.waiting = false;
        }
        self.retry_at = Instant::now() + Duration::from_secs(10);
        self.should_reconnect = true;
        self.dirty = true;
    }

    fn request(&mut self, method: &str, params: Value, pending: Pending) -> bool {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        if self.write(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})) {
            self.pending.insert(id, pending);
            true
        } else {
            false
        }
    }

    fn write(&mut self, mut message: Value) -> bool {
        let Some(stdin) = self.stdin.as_mut() else {
            wipe_mcp_secrets(&mut message);
            return false;
        };
        let Ok(mut bytes) = serde_json::to_vec(&message) else {
            wipe_mcp_secrets(&mut message);
            return false;
        };
        bytes.push(b'\n');
        let sent = stdin.write_all(&bytes).and_then(|_| stdin.flush()).is_ok();
        for byte in &mut bytes {
            unsafe { std::ptr::write_volatile(byte, 0) };
        }
        wipe_mcp_secrets(&mut message);
        sent
    }

    fn rpc_result(&mut self, id: Value, result: Value) {
        self.write(json!({"jsonrpc":"2.0","id":id,"result":result}));
    }

    fn rpc_error(&mut self, id: Value, code: i64, message: &str) {
        self.write(json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}}));
    }

    fn stop_all_terminals(&mut self) {
        let ids: Vec<_> = self.terminals.keys().cloned().collect();
        for id in ids {
            self.release_terminal(&id);
        }
    }

    fn release_terminal(&mut self, id: &str) {
        if let Some(mut terminal) = self.terminals.remove(id) {
            if !terminal.exited {
                let _ = terminal.child.kill();
                let _ = terminal.child.wait();
            }
            for reader in terminal.readers.drain(..) {
                let _ = reader.join();
            }
            let text = terminal
                .capture
                .lock()
                .map(|capture| capture.text.clone())
                .unwrap_or_default();
            self.terminal_history.insert(id.to_owned(), text);
            while self.terminal_history.len() > 8
                || self
                    .terminal_history
                    .values()
                    .map(String::len)
                    .sum::<usize>()
                    > MAX_TERMINAL_HISTORY_BYTES
            {
                if let Some(oldest) = self.terminal_history.keys().next().cloned() {
                    self.terminal_history.remove(&oldest);
                } else {
                    break;
                }
            }
        }
    }

    fn poll_terminals(&mut self) {
        let mut completed = Vec::new();
        for (id, terminal) in &mut self.terminals {
            if !terminal.exited
                && let Ok(Some(status)) = terminal.child.try_wait()
            {
                terminal.exit_code = status.code();
                terminal.exited = true;
            }
            let mut index = 0;
            while index < terminal.readers.len() {
                if terminal.readers[index].is_finished() {
                    let reader = terminal.readers.swap_remove(index);
                    let _ = reader.join();
                } else {
                    index += 1;
                }
            }
            if terminal.exited && terminal.readers.is_empty() && !terminal.waiters.is_empty() {
                completed.push((
                    id.clone(),
                    terminal.exit_code,
                    std::mem::take(&mut terminal.waiters),
                ));
            }
        }
        for (_, code, waiters) in completed {
            for waiter in waiters {
                self.rpc_result(waiter, json!({"exitCode":code,"signal":null}));
            }
        }
    }

    fn handle_fs_request(&mut self, message: &Value, write_text: bool) {
        let Some(id) = message.get("id").cloned() else {
            return;
        };
        let params = &message["params"];
        let Some(session_id) = params["sessionId"].as_str() else {
            self.rpc_error(id, -32602, "Missing sessionId");
            return;
        };
        let Some(path) = params["path"].as_str().filter(|path| path.len() <= 4096) else {
            self.rpc_error(id, -32602, "Invalid path");
            return;
        };
        let Some(allowed) = self.resolve_session_path(session_id, path, write_text) else {
            self.rpc_error(
                id,
                -32001,
                "The path is outside this session's approved workspace folders.",
            );
            return;
        };
        if write_text {
            let Some(content) = params["content"]
                .as_str()
                .filter(|content| content.len() <= 10_000_000)
            else {
                self.rpc_error(id, -32602, "File content is missing or too large");
                return;
            };
            match std::fs::write(&allowed, content) {
                Ok(()) => self.rpc_result(id, json!({})),
                Err(error) => self.rpc_error(id, -32002, &format!("Could not write file: {error}")),
            }
        } else {
            match std::fs::read_to_string(&allowed) {
                Ok(content) if content.len() <= 10_000_000 => {
                    let line = params["line"].as_u64().unwrap_or(1).max(1) as usize;
                    let limit = params["limit"]
                        .as_u64()
                        .map(|value| value.clamp(1, 50_000) as usize);
                    let selected = content
                        .split_inclusive('\n')
                        .skip(line.saturating_sub(1))
                        .take(limit.unwrap_or(usize::MAX))
                        .collect::<String>();
                    self.rpc_result(id, json!({"content":selected}));
                }
                Ok(_) => self.rpc_error(id, -32002, "File is too large to read through ACP."),
                Err(error) => {
                    self.rpc_error(id, -32002, &format!("Could not read text file: {error}"))
                }
            }
        }
    }

    fn resolve_session_path(
        &self,
        session_id: &str,
        raw_path: &str,
        allow_missing_file: bool,
    ) -> Option<PathBuf> {
        let root_id = self.root_session(session_id);
        let session = self
            .snapshot
            .sessions
            .iter()
            .find(|session| session.id == root_id)?;
        let mut roots = vec![session.directory.as_str()];
        roots.extend(session.additional_directories.iter().map(String::as_str));
        let path = Path::new(raw_path);
        if !path.is_absolute() {
            return None;
        }
        let candidate = if allow_missing_file && !path.exists() {
            // `Path::exists` follows links and returns false for a dangling link.
            // Refuse any existing directory entry before constructing a path for
            // a new file so a dangling symlink cannot escape an approved root.
            if std::fs::symlink_metadata(path).is_ok() {
                return None;
            }
            let name = path.file_name()?;
            Some(path.parent()?.canonicalize().ok()?.join(name))
        } else {
            Some(path.canonicalize().ok()?)
        }?;
        let candidate_key = path_key(&candidate);
        roots
            .into_iter()
            .any(|root| {
                let Ok(root) = Path::new(root).canonicalize() else {
                    return false;
                };
                let root_key = path_key(&root);
                candidate_key == root_key
                    || candidate_key
                        .strip_prefix(&root_key)
                        .is_some_and(|tail| tail.starts_with('/'))
            })
            .then_some(candidate)
    }

    fn handle_terminal_request(&mut self, message: &Value) {
        let Some(id) = message.get("id").cloned() else {
            return;
        };
        let method = message["method"].as_str().unwrap_or("");
        let params = &message["params"];
        let Some(session_id) = params["sessionId"]
            .as_str()
            .filter(|value| value.len() <= 256)
        else {
            self.rpc_error(id, -32602, "Missing sessionId");
            return;
        };
        match method {
            "terminal/create" => {
                let Some(root_id) = self
                    .snapshot
                    .sessions
                    .iter()
                    .find(|session| session.id == self.root_session(session_id))
                    .map(|session| session.id.clone())
                else {
                    self.rpc_error(id, -32001, "Unknown session");
                    return;
                };
                if self.terminals.len() >= 16 {
                    self.rpc_error(id, -32003, "Terminal limit reached");
                    return;
                }
                let Some(command) = params["command"]
                    .as_str()
                    .filter(|command| !command.is_empty() && command.len() <= 2048)
                else {
                    self.rpc_error(id, -32602, "Invalid command");
                    return;
                };
                let args: Vec<String> = match params["args"].as_array() {
                    Some(args) if args.len() <= 128 => args
                        .iter()
                        .map(|item| {
                            item.as_str()
                                .filter(|arg| arg.len() <= 8192)
                                .map(str::to_owned)
                        })
                        .collect::<Option<Vec<_>>>()
                        .unwrap_or_default(),
                    Some(_) => {
                        self.rpc_error(id, -32602, "Too many command arguments");
                        return;
                    }
                    None => Vec::new(),
                };
                if params["args"].is_array()
                    && params["args"]
                        .as_array()
                        .is_some_and(|items| items.len() != args.len())
                {
                    self.rpc_error(id, -32602, "Invalid command argument");
                    return;
                }
                let requested_cwd = params["cwd"].as_str().unwrap_or_else(|| {
                    self.snapshot
                        .sessions
                        .iter()
                        .find(|session| session.id == root_id)
                        .map(|session| session.directory.as_str())
                        .unwrap_or("")
                });
                let Some(cwd) = self
                    .resolve_session_path(session_id, requested_cwd, false)
                    .filter(|path| path.is_dir())
                else {
                    self.rpc_error(
                        id,
                        -32001,
                        "Terminal working directory must be inside an approved workspace folder.",
                    );
                    return;
                };
                let output_limit = params["outputByteLimit"]
                    .as_u64()
                    .unwrap_or(1_000_000)
                    .clamp(1024, 8_000_000) as usize;
                let capture = Arc::new(Mutex::new(TerminalCapture {
                    text: String::new(),
                    byte_limit: output_limit,
                    truncated: false,
                }));
                let mut process = Command::new(command);
                process
                    .args(args)
                    .current_dir(cwd)
                    .stdin(Stdio::null())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .creation_flags_no_window();
                if let Some(env) = params["env"].as_array() {
                    for item in env.iter().take(32) {
                        let Some(name) = item["name"].as_str().filter(|key| {
                            !key.is_empty()
                                && key.len() <= 128
                                && key
                                    .bytes()
                                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
                        }) else {
                            continue;
                        };
                        let Some(value) =
                            item["value"].as_str().filter(|value| value.len() <= 8192)
                        else {
                            continue;
                        };
                        if !matches!(
                            name.to_ascii_uppercase().as_str(),
                            "SYSTEMROOT"
                                | "WINDIR"
                                | "COMSPEC"
                                | "USERPROFILE"
                                | "APPDATA"
                                | "LOCALAPPDATA"
                                | "CODEX_HOME"
                        ) {
                            process.env(name, value);
                        }
                    }
                }
                match process.spawn() {
                    Ok(mut child) => {
                        let id_string =
                            format!("island-terminal-{}-{}", self.next_terminal_id, self.next_id);
                        self.next_terminal_id = self.next_terminal_id.wrapping_add(1);
                        let mut readers = Vec::with_capacity(2);
                        if let Some(stdout) = child.stdout.take()
                            && let Some(reader) = capture_pipe(stdout, capture.clone())
                        {
                            readers.push(reader);
                        }
                        if let Some(stderr) = child.stderr.take()
                            && let Some(reader) = capture_pipe(stderr, capture.clone())
                        {
                            readers.push(reader);
                        }
                        self.terminals.insert(
                            id_string.clone(),
                            TerminalProcess {
                                child,
                                session_id: root_id,
                                capture,
                                readers,
                                waiters: Vec::new(),
                                exit_code: None,
                                exited: false,
                            },
                        );
                        self.rpc_result(id, json!({"terminalId":id_string}));
                    }
                    Err(error) => self.rpc_error(
                        id,
                        -32002,
                        &format!("Could not start terminal command: {error}"),
                    ),
                }
            }
            "terminal/output" | "terminal/wait_for_exit" | "terminal/kill" | "terminal/release" => {
                let Some(terminal_id) = params["terminalId"].as_str() else {
                    self.rpc_error(id, -32602, "Missing terminalId");
                    return;
                };
                let root_id = self.root_session(session_id);
                if method == "terminal/release" {
                    let Some(terminal) = self.terminals.get(terminal_id) else {
                        self.rpc_error(id, -32004, "Unknown terminal");
                        return;
                    };
                    if terminal.session_id != root_id {
                        self.rpc_error(id, -32001, "Terminal does not belong to this session");
                        return;
                    }
                    self.release_terminal(terminal_id);
                    self.rpc_result(id, json!({}));
                    return;
                }
                let Some(terminal) = self.terminals.get_mut(terminal_id) else {
                    self.rpc_error(id, -32004, "Unknown terminal");
                    return;
                };
                if terminal.session_id != root_id {
                    self.rpc_error(id, -32001, "Terminal does not belong to this session");
                    return;
                }
                match method {
                    "terminal/output" => {
                        let capture = terminal
                            .capture
                            .lock()
                            .map(|capture| (capture.text.clone(), capture.truncated))
                            .unwrap_or_default();
                        let mut result = json!({"output":capture.0,"truncated":capture.1});
                        if terminal.exited {
                            result["exitStatus"] =
                                json!({"exitCode":terminal.exit_code,"signal":null});
                        }
                        self.rpc_result(id, result);
                    }
                    "terminal/wait_for_exit" => {
                        if terminal.exited {
                            let result = json!({"exitCode":terminal.exit_code,"signal":null});
                            self.rpc_result(id, result);
                        } else if terminal.waiters.len() < MAX_TERMINAL_WAITERS {
                            terminal.waiters.push(id);
                        } else {
                            self.rpc_error(id, -32003, "Too many pending terminal wait requests");
                        }
                    }
                    "terminal/kill" => {
                        if !terminal.exited {
                            let _ = terminal.child.kill();
                        }
                        let result = json!({});
                        self.rpc_result(id, result);
                    }
                    _ => {}
                }
            }
            _ => self.rpc_error(id, -32601, "Unknown terminal method"),
        }
    }

    fn handle_action(&mut self, action: Action) {
        match action {
            Action::Connect => {
                if self.snapshot.installing || self.snapshot.authenticating {
                    return;
                }
                if self.snapshot.active_count > 0 {
                    self.snapshot.notice = "Finish active tasks before reconnecting.".into();
                    self.dirty = true;
                    return;
                }
                if let Some(mut child) = self.child.take() {
                    let _ = child.kill();
                }
                self.disconnect("");
                self.retry_at = Instant::now();
                self.connect();
            }
            Action::Install => {
                if self.provider != "codex"
                    || self.snapshot.installing
                    || self.snapshot.active_count > 0
                {
                    return;
                }
                let (tx, rx) = mpsc::channel();
                self.install_rx = Some(rx);
                self.snapshot.installing = true;
                self.snapshot.notice = if self.snapshot.update_available {
                    format!("Preparing Codex ACP {}…", self.snapshot.latest_version)
                } else {
                    "Preparing the latest stable Codex ACP…".into()
                };
                self.dirty = true;
                let package_version = self.snapshot.latest_version.clone();
                thread::spawn(move || {
                    let _ = tx.send(install_codex_acp(&package_version));
                });
            }
            Action::CheckUpdate => self.check_latest_version(),
            Action::Refresh => self.list(false),
            Action::More => self.list(true),
            Action::Authenticate(method_id) => {
                if let Some(method) = self
                    .auth_methods
                    .iter()
                    .find(|item| item["id"] == method_id)
                    .cloned()
                {
                    if method["type"] == "terminal" {
                        if self.snapshot.authenticating || self.snapshot.active_count > 0 {
                            return;
                        }
                        if let Some((program, base_args)) = provider_command(self.provider) {
                            #[cfg(windows)]
                            use std::os::windows::process::CommandExt;
                            let extension = &method["_meta"]["terminal-auth"];
                            let extension_command = extension["command"].as_str().unwrap_or("");
                            let extension_args: Vec<String> = extension["args"]
                                .as_array()
                                .map(|items| {
                                    items
                                        .iter()
                                        .take(8)
                                        .filter_map(|item| {
                                            item.as_str()
                                                .filter(|arg| {
                                                    arg.len() <= 512
                                                        && !arg.chars().any(char::is_control)
                                                })
                                                .map(str::to_owned)
                                        })
                                        .collect()
                                })
                                .unwrap_or_default();
                            let opencode_override = self.provider == "opencode"
                                && Path::new(extension_command)
                                    .file_stem()
                                    .and_then(|name| name.to_str())
                                    .is_some_and(|name| name.eq_ignore_ascii_case("opencode"))
                                && !extension_args.is_empty();
                            let login_args: Vec<String> = method["args"]
                                .as_array()
                                .map(|items| {
                                    items
                                        .iter()
                                        .take(8)
                                        .filter_map(|item| {
                                            item.as_str()
                                                .filter(|arg| {
                                                    arg.len() <= 512
                                                        && !arg.chars().any(char::is_control)
                                                })
                                                .map(str::to_owned)
                                        })
                                        .collect()
                                })
                                .unwrap_or_default();
                            let mut command = Command::new(program);
                            if opencode_override {
                                command.args(extension_args);
                            } else {
                                command.args(base_args).args(login_args);
                            }
                            if let Some(env) = method["env"].as_object() {
                                for (key, value) in env.iter().take(8) {
                                    if key.len() <= 128
                                        && key.bytes().all(|byte| {
                                            byte.is_ascii_alphanumeric() || byte == b'_'
                                        })
                                        && !matches!(
                                            key.to_ascii_uppercase().as_str(),
                                            "PATH"
                                                | "COMSPEC"
                                                | "SYSTEMROOT"
                                                | "WINDIR"
                                                | "USERPROFILE"
                                                | "APPDATA"
                                                | "LOCALAPPDATA"
                                        )
                                        && let Some(value) = value.as_str()
                                        && value.len() <= 2048
                                        && !value.chars().any(char::is_control)
                                    {
                                        command.env(key, value);
                                    }
                                }
                            }
                            #[cfg(windows)]
                            command.creation_flags(0x0000_0010);
                            match command.spawn() {
                                Ok(mut child) => {
                                    let (tx, rx) = mpsc::channel();
                                    self.auth_rx = Some(rx);
                                    self.snapshot.authenticating = true;
                                    self.snapshot.notice = format!(
                                        "Complete sign-in in the {} terminal.",
                                        if self.provider == "codex" {
                                            "Codex"
                                        } else {
                                            "OpenCode"
                                        }
                                    );
                                    self.dirty = true;
                                    let provider = self.provider;
                                    thread::spawn(move || {
                                        let name = if provider == "codex" {
                                            "Codex"
                                        } else {
                                            "OpenCode"
                                        };
                                        let result = child
                                            .wait()
                                            .map_err(|error| {
                                                format!("{name} sign-in did not finish: {error}")
                                            })
                                            .and_then(|status| {
                                                if status.success() {
                                                    Ok(())
                                                } else {
                                                    Err(format!(
                                                        "{name} sign-in exited with {status}."
                                                    ))
                                                }
                                            });
                                        let _ = tx.send(result);
                                    });
                                }
                                Err(error) => self.fail_operation(
                                    "auth",
                                    &format!("Could not open provider sign-in: {error}"),
                                ),
                            }
                        }
                    } else if method["type"].as_str().unwrap_or("agent") == "agent" {
                        self.request(
                            "authenticate",
                            json!({"methodId": method_id}),
                            Pending::Authenticate,
                        );
                    }
                }
            }
            Action::Logout => {
                if self.snapshot.capabilities["auth"]["logout"].is_null() {
                    return;
                }
                if self.snapshot.active_count > 0 {
                    self.snapshot.notice = "Finish active tasks before signing out.".into();
                    self.dirty = true;
                    return;
                }
                self.request("logout", json!({}), Pending::Logout);
            }
            Action::Select(id) => {
                if id == self.snapshot.selected_id && (id.is_empty() || self.loaded.contains(&id)) {
                    return;
                }
                self.snapshot.selected_id = id.clone();
                self.snapshot.response = self.cached_responses.get(&id).cloned();
                self.dirty = true;
                if !id.is_empty() && !self.loaded.contains(&id)
                    && let Some(session) = self
                        .snapshot
                        .sessions
                        .iter()
                        .find(|session| session.id == id)
                    {
                        let params = match self.session_params(
                            &session.directory,
                            &session.additional_directories,
                            Some(&id),
                        ) {
                            Ok(params) => params,
                            Err(error) => {
                                self.fail_operation("session", &error);
                                return;
                            }
                        };
                        if self.snapshot.capabilities["loadSession"] == true {
                            if let Some(previous) = self.cached_responses.get(&id).cloned() {
                                let mut visible = previous.clone();
                                Arc::make_mut(&mut visible).waiting = true;
                                self.snapshot.response = Some(visible);
                                self.load_fallback.insert(id.clone(), previous);
                            } else {
                                self.snapshot.response = Some(Arc::new(Response {
                                    session_id: id.clone(),
                                    waiting: true,
                                    ..Response::default()
                                }));
                            }
                            self.cached_responses.insert(
                                id.clone(),
                                Arc::new(Response {
                                    session_id: id.clone(),
                                    waiting: true,
                                    ..Response::default()
                                }),
                            );
                            self.request("session/load", params, Pending::Load { session_id: id });
                        } else if !self.snapshot.capabilities["sessionCapabilities"]["resume"]
                            .is_null()
                        {
                            self.request(
                                "session/resume",
                                params,
                                Pending::Resume { session_id: id },
                            );
                        }
                    }
            }
            Action::Prepare {
                directory,
                additional_directories,
                config,
            } => {
                if !self.snapshot.connected {
                    self.fail_operation(
                        "prepare",
                        "Connect the provider before setting up a task.",
                    );
                    return;
                }
                let cwd = if directory.is_empty() {
                    std::env::current_dir()
                        .unwrap_or_else(|_| PathBuf::from("C:\\"))
                        .to_string_lossy()
                        .into_owned()
                } else {
                    directory
                };
                if !Path::new(&cwd).is_dir() {
                    self.fail_operation("prepare", "Choose an existing project folder.");
                    return;
                }
                let additional_directories = self.allowed_directories(additional_directories);
                let params = match self.session_params(&cwd, &additional_directories, None) {
                    Ok(params) => params,
                    Err(error) => {
                        self.fail_operation("prepare", &error);
                        return;
                    }
                };
                if !self.request(
                    "session/new",
                    params,
                    Pending::New {
                        prepare: true,
                        text: String::new(),
                        config,
                        files: Vec::new(),
                        directory: cwd,
                        additional_directories,
                        system_prompt: String::new(),
                        hybrid_whiteboard: false,
                    },
                ) {
                    self.fail_operation(
                        "prepare",
                        "The provider connection closed before creating the task.",
                    );
                }
            }
            Action::Start {
                directory,
                additional_directories,
                text,
                config,
                files,
                system_prompt,
                hybrid_whiteboard,
            } => {
                if self.snapshot.installing {
                    self.fail_operation(
                        "start",
                        "Finish updating this provider before starting a task.",
                    );
                    return;
                }
                if !self.snapshot.connected {
                    self.fail_operation("start", "Connect the provider before starting a task.");
                    return;
                }
                let private_workspace = system_prompt.starts_with("Pluto response mode: wake-answer\n")
                    || system_prompt.starts_with("Pluto response mode: meeting-optimize\n");
                let cwd = if private_workspace {
                    let root = std::env::var_os("LOCALAPPDATA")
                        .map(PathBuf::from)
                        .unwrap_or_else(std::env::temp_dir)
                        .join("Pluto")
                        .join(if system_prompt.starts_with("Pluto response mode: meeting-optimize\n") { "MeetingReviewWorkspace" } else { "VoiceWorkspace" });
                    if let Err(error) = std::fs::create_dir_all(&root) {
                        self.fail_operation(
                            "start",
                            &format!("Could not prepare voice conversation: {error}"),
                        );
                        return;
                    }
                    root.to_string_lossy().into_owned()
                } else if directory.is_empty() {
                    std::env::current_dir()
                        .unwrap_or_else(|_| PathBuf::from("C:\\"))
                        .to_string_lossy()
                        .into_owned()
                } else {
                    directory.clone()
                };
                if !Path::new(&cwd).is_dir() {
                    self.fail_operation("start", "Choose an existing project folder.");
                    return;
                }
                let additional_directories =
                    if private_workspace {
                        Vec::new()
                    } else {
                        self.allowed_directories(additional_directories)
                    };
                let params = match self.session_params(&cwd, &additional_directories, None) {
                    Ok(params) => params,
                    Err(error) => {
                        self.fail_operation("start", &error);
                        return;
                    }
                };
                if !self.request(
                    "session/new",
                    params,
                    Pending::New {
                        prepare: false,
                        text,
                        config,
                        files,
                        directory: cwd,
                        additional_directories,
                        system_prompt,
                        hybrid_whiteboard,
                    },
                ) {
                    self.fail_operation(
                        "start",
                        "The provider connection closed before creating the task.",
                    );
                }
            }
            Action::Prompt {
                session_id,
                text,
                files,
                system_prompt,
                hybrid_whiteboard,
            } => {
                if self.snapshot.installing {
                    self.fail_operation(
                        "prompt",
                        "Finish updating this provider before sending a follow-up.",
                    );
                    return;
                }
                if !self.snapshot.connected
                    || !self.loaded.contains(&session_id)
                    || !self
                        .snapshot
                        .sessions
                        .iter()
                        .any(|session| session.id == session_id)
                {
                    self.fail_operation(
                        "prompt",
                        "This session is not ready. Select it again, then retry.",
                    );
                    return;
                }
                if self
                    .snapshot
                    .sessions
                    .iter()
                    .any(|session| session.id == session_id && session.active)
                {
                    self.fail_operation("prompt", "This task is still running. Wait for it to finish or stop it before sending a follow-up.");
                    return;
                }
                self.prompt(session_id, text, files, system_prompt, hybrid_whiteboard);
            }
            Action::Cancel(session_id) => {
                if self
                    .snapshot
                    .sessions
                    .iter()
                    .any(|session| session.id == session_id && session.active)
                {
                    self.write(json!({"jsonrpc":"2.0","method":"session/cancel","params":{"sessionId":session_id}}));
                    let pending: Vec<_> = self
                        .snapshot
                        .permissions
                        .iter()
                        .filter(|item| item.owner_session_id == session_id)
                        .map(|item| item.id.clone())
                        .collect();
                    for id in pending {
                        if let Ok(raw) = serde_json::from_str::<Value>(&id) {
                            self.write(json!({"jsonrpc":"2.0","id":raw,"result":{"outcome":{"outcome":"cancelled"}}}));
                        }
                    }
                    self.snapshot
                        .permissions
                        .retain(|item| item.owner_session_id != session_id);
                    let questions: Vec<_> = self
                        .snapshot
                        .questions
                        .iter()
                        .filter(|item| item.owner_session_id == session_id)
                        .map(|item| item.id.clone())
                        .collect();
                    for id in questions {
                        if let Ok(raw) = serde_json::from_str::<Value>(&id) {
                            self.write(
                                json!({"jsonrpc":"2.0","id":raw,"result":{"action":"cancel"}}),
                            );
                        }
                    }
                    self.snapshot
                        .questions
                        .retain(|item| item.owner_session_id != session_id);
                    self.dirty = true;
                }
            }
            Action::Close(session_id) => {
                if !self.snapshot.capabilities["sessionCapabilities"]["close"].is_null() {
                    self.request(
                        "session/close",
                        json!({"sessionId":session_id}),
                        Pending::Close { session_id },
                    );
                }
            }
            Action::Delete(session_id) => {
                if !self.snapshot.capabilities["sessionCapabilities"]["delete"].is_null()
                    && self
                        .snapshot
                        .sessions
                        .iter()
                        .any(|session| session.id == session_id && !session.active)
                {
                    self.request(
                        "session/delete",
                        json!({"sessionId":session_id}),
                        Pending::Delete { session_id },
                    );
                }
            }
            Action::Fork(source_id) => {
                if !self.snapshot.capabilities["sessionCapabilities"]["fork"].is_null()
                    && let Some(session) = self
                        .snapshot
                        .sessions
                        .iter()
                        .find(|session| session.id == source_id && !session.active)
                    {
                        let directory = session.directory.clone();
                        let params = match self.session_params(
                            &directory,
                            &session.additional_directories,
                            Some(&source_id),
                        ) {
                            Ok(params) => params,
                            Err(error) => {
                                self.fail_operation("fork", &error);
                                return;
                            }
                        };
                        self.request(
                            "session/fork",
                            params,
                            Pending::Fork {
                                source_id,
                                directory,
                            },
                        );
                    }
            }
            Action::Configure {
                session_id,
                id,
                value,
            } => {
                if let Some(session) = self
                    .snapshot
                    .sessions
                    .iter()
                    .find(|session| session.id == session_id)
                {
                    if let Some(option) = session
                        .config_options
                        .iter()
                        .find(|option| option["id"] == id && valid_config_value(option, &value))
                    {
                        let params = config_params(&session_id, &id, &value, option);
                        self.request(
                            "session/set_config_option",
                            params,
                            Pending::Configure { session_id },
                        );
                    } else {
                        self.fail_operation(
                            "configure",
                            "This choice is unavailable in the current session.",
                        );
                    }
                }
            }
            Action::SetMode { session_id, mode } => {
                if self.snapshot.sessions.iter().any(|session| {
                    session.id == session_id
                        && session.modes.as_ref().is_some_and(|modes| {
                            modes["availableModes"]
                                .as_array()
                                .is_some_and(|values| values.iter().any(|item| item["id"] == mode))
                        })
                }) {
                    self.request(
                        "session/set_mode",
                        json!({"sessionId":session_id,"modeId":mode}),
                        Pending::SetMode { session_id },
                    );
                }
            }
            Action::Reply {
                session_id,
                request_id,
                option_id,
            } => {
                if let Some(index) = self.snapshot.permissions.iter().position(|item| {
                    item.session_id == session_id
                        && item.id == request_id
                        && item
                            .options
                            .iter()
                            .any(|option| option["optionId"] == option_id)
                })
                    && let Ok(raw) = serde_json::from_str::<Value>(&request_id)
                        && self.write(json!({"jsonrpc":"2.0","id":raw,"result":{"outcome":{"outcome":"selected","optionId":option_id}}})) {
                            self.snapshot.permissions.remove(index); self.dirty = true;
                        }
            }
            Action::Answer {
                session_id,
                request_id,
                choice,
                content,
            } => {
                if !["accept", "decline", "cancel"].contains(&choice.as_str()) {
                    return;
                }
                if let Some(index) = self
                    .snapshot
                    .questions
                    .iter()
                    .position(|item| item.session_id == session_id && item.id == request_id)
                {
                    if choice == "accept"
                        && self.snapshot.questions[index].mode == "form"
                        && !valid_form_content(&self.snapshot.questions[index].schema, &content)
                    {
                        self.fail_operation(
                            "question",
                            "Complete the required answer before submitting.",
                        );
                        return;
                    }
                    if let Ok(raw) = serde_json::from_str::<Value>(&request_id) {
                        let response = if choice == "accept"
                            && self.snapshot.questions[index].mode == "form"
                        {
                            json!({"action":"accept","content":content})
                        } else {
                            json!({"action":choice})
                        };
                        if self.write(json!({"jsonrpc":"2.0","id":raw,"result":response})) {
                            self.snapshot.questions.remove(index);
                            self.dirty = true;
                        }
                    }
                }
            }
        }
    }

    fn prompt(
        &mut self,
        session_id: String,
        text: String,
        files: Vec<String>,
        system_prompt: String,
        hybrid_whiteboard: bool,
    ) {
        let prompt_text = compose_prompt_text(&system_prompt, &text, hybrid_whiteboard);
        let mut blocks = vec![json!({"type":"text","text":prompt_text})];
        let image_supported = self.snapshot.capabilities["promptCapabilities"]["image"] == true;
        let audio_supported = self.snapshot.capabilities["promptCapabilities"]["audio"] == true;
        let embedded_supported =
            self.snapshot.capabilities["promptCapabilities"]["embeddedContext"] == true;
        let mut embedded_budget = 14 * 1024 * 1024usize;
        for path in files.iter().take(8) {
            let p = Path::new(path);
            if p.is_file() {
                let uri = file_uri(path);
                let name = p
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                let mime = mime_type(p);
                let size = p.metadata().map(|meta| meta.len()).unwrap_or_default();
                if image_supported
                    && mime.starts_with("image/")
                    && size <= 8 * 1024 * 1024
                    && size as usize <= embedded_budget
                    && let Ok(bytes) = std::fs::read(p)
                {
                    embedded_budget =
                        embedded_budget.saturating_sub(bytes.len().saturating_mul(4) / 3);
                    blocks.push(json!({"type":"image","mimeType":mime,"data":base64::engine::general_purpose::STANDARD.encode(bytes)}));
                    continue;
                }
                if audio_supported
                    && mime.starts_with("audio/")
                    && size <= 8 * 1024 * 1024
                    && size as usize <= embedded_budget
                    && let Ok(bytes) = std::fs::read(p)
                {
                    embedded_budget =
                        embedded_budget.saturating_sub(bytes.len().saturating_mul(4) / 3);
                    blocks.push(json!({"type":"audio","mimeType":mime,"data":base64::engine::general_purpose::STANDARD.encode(bytes)}));
                    continue;
                }
                if embedded_supported
                    && mime.starts_with("text/")
                    && size <= 1024 * 1024
                    && size as usize <= embedded_budget
                    && let Ok(text) = std::fs::read_to_string(p)
                {
                    embedded_budget = embedded_budget.saturating_sub(text.len());
                    blocks.push(json!({"type":"resource","resource":{"uri":uri,"mimeType":mime,"text":text}}));
                    continue;
                }
                if embedded_supported
                    && !mime.starts_with("image/")
                    && !mime.starts_with("audio/")
                    && size <= 4 * 1024 * 1024
                    && size as usize <= embedded_budget
                    && let Ok(bytes) = std::fs::read(p)
                {
                    embedded_budget =
                        embedded_budget.saturating_sub(bytes.len().saturating_mul(4) / 3);
                    blocks.push(json!({"type":"resource","resource":{"uri":uri,"mimeType":mime,"blob":base64::engine::general_purpose::STANDARD.encode(bytes)}}));
                    continue;
                }
                blocks.push(json!({"type":"resource_link","uri":uri,"name":name,"mimeType":mime,"size":size}));
            }
        }
        if let Some(session) = self
            .snapshot
            .sessions
            .iter_mut()
            .find(|session| session.id == session_id)
        {
            session.active = true;
            if session.title == "New task" && !text.is_empty() {
                session.title = text.chars().take(80).collect();
            }
        }
        self.snapshot.active_count = self
            .snapshot
            .sessions
            .iter()
            .filter(|session| session.active)
            .count()
            + self.active_subagent_count();
        self.snapshot.selected_id = session_id.clone();
        if let Some(previous) = self.cached_responses.get(&session_id).cloned() {
            self.prompt_fallback.insert(session_id.clone(), previous);
        }
        let response = Arc::new(Response {
            session_id: session_id.clone(),
            waiting: true,
            visual_policy: crate::agent_presentation::VisualPolicy::for_request(
                &text,
                hybrid_whiteboard,
            ),
            ..Response::default()
        });
        self.message_ids.remove(&session_id);
        self.snapshot.response = Some(response.clone());
        self.cached_responses.insert(session_id.clone(), response);
        if self.request(
            "session/prompt",
            json!({"sessionId":session_id,"prompt":blocks}),
            Pending::Prompt {
                session_id: session_id.clone(),
            },
        ) {
            self.complete_operation("prompt", true, "Task sent");
        } else {
            if let Some(previous) = self.prompt_fallback.remove(&session_id) {
                self.cached_responses
                    .insert(session_id.clone(), previous.clone());
                self.snapshot.response = Some(previous);
            }
            self.finish_prompt(&session_id);
            self.fail_operation(
                "prompt",
                "The provider connection closed before sending the task.",
            );
        }
        self.dirty = true;
    }

    fn allowed_directories(&self, directories: Vec<String>) -> Vec<String> {
        if self.snapshot.capabilities["sessionCapabilities"]["additionalDirectories"].is_null() {
            return Vec::new();
        }
        directories
            .into_iter()
            .filter(|directory| {
                directory.len() <= 4096
                    && Path::new(directory).is_absolute()
                    && Path::new(directory).is_dir()
            })
            .take(6)
            .collect()
    }

    fn session_params(
        &self,
        cwd: &str,
        additional_directories: &[String],
        session_id: Option<&str>,
    ) -> Result<Value, String> {
        let http_supported = self.snapshot.capabilities["mcpCapabilities"]["http"] == true;
        let mcp_servers = crate::platform::windows::agent_library::acp_mcp_servers(http_supported)?;
        let mut params = json!({"cwd":cwd,"mcpServers":mcp_servers});
        if let Some(session_id) = session_id {
            params["sessionId"] = json!(session_id);
        }
        if !self.snapshot.capabilities["sessionCapabilities"]["additionalDirectories"].is_null() {
            params["additionalDirectories"] = json!(additional_directories);
        }
        Ok(params)
    }

    fn root_session(&self, id: &str) -> String {
        let mut current = id;
        for _ in 0..8 {
            let Some(parent) = self.active_children.get(current) else {
                break;
            };
            current = parent;
        }
        current.to_owned()
    }

    fn list(&mut self, append: bool) {
        if !self.snapshot.connected
            || self.snapshot.capabilities["sessionCapabilities"]["list"].is_null()
        {
            return;
        }
        let params = if append {
            self.snapshot
                .next_cursor
                .as_ref()
                .map(|cursor| json!({"cursor":cursor}))
        } else {
            Some(json!({}))
        };
        if let Some(params) = params {
            self.request("session/list", params, Pending::List { append });
        }
    }

    fn handle_message(&mut self, message: Value) {
        if let Some(method) = message["method"].as_str() {
            match method {
                "session/update" => self.update(&message["params"]),
                "fs/read_text_file" => self.handle_fs_request(&message, false),
                "fs/write_text_file" => self.handle_fs_request(&message, true),
                "terminal/create"
                | "terminal/output"
                | "terminal/wait_for_exit"
                | "terminal/kill"
                | "terminal/release" => self.handle_terminal_request(&message),
                "elicitation/complete" => {
                    if message["params"]["elicitationId"]
                        .as_str()
                        .is_some_and(|value| !value.is_empty())
                    {
                        self.snapshot.notice = "External sign-in completed.".into();
                        self.dirty = true;
                    }
                }
                "session/request_permission" => {
                    let params = &message["params"];
                    if let (Some(id), Some(session_id)) =
                        (message.get("id"), params["sessionId"].as_str())
                    {
                        let id = id.to_string();
                        let tool = &params["toolCall"];
                        let options: Vec<Value> = params["options"].as_array().map(|items| items.iter().take(8).filter_map(|option| {
                            let id = option["optionId"].as_str().filter(|id| id.len() <= 128)?;
                            Some(json!({"optionId":id,"kind":option["kind"].as_str().unwrap_or("").chars().take(64).collect::<String>(),"name":option["name"].as_str().unwrap_or("Choose").chars().take(120).collect::<String>(),"meta":option["_meta"]}))
                        }).collect()).unwrap_or_default();
                        if options.is_empty() {
                            self.write(json!({"jsonrpc":"2.0","id":message["id"],"result":{"outcome":{"outcome":"cancelled"}}}));
                        } else {
                            let raw = &tool["rawInput"];
                            let detail = if !raw.is_null() {
                                raw.as_str()
                                    .map(str::to_owned)
                                    .unwrap_or_else(|| raw.to_string())
                            } else {
                                let content = self.safe_tool_content(&tool["content"]);
                                let summary = content_blocks_text(&content);
                                if summary.is_empty() {
                                    String::new()
                                } else {
                                    summary
                                }
                            };
                            let owner_session_id =
                                tool["_meta"]["opencode/child-session"]["parentID"]
                                    .as_str()
                                    .map(|parent| self.root_session(parent))
                                    .unwrap_or_else(|| self.root_session(session_id));
                            let metadata = if tool["_meta"].to_string().len() <= 16_000 {
                                tool["_meta"].clone()
                            } else {
                                Value::Null
                            };
                            self.snapshot.permissions.push(Permission {
                                id,
                                session_id: session_id.into(),
                                owner_session_id,
                                action: tool["title"]
                                    .as_str()
                                    .unwrap_or("Agent action")
                                    .chars()
                                    .take(160)
                                    .collect(),
                                detail: detail.chars().take(1200).collect(),
                                options,
                                meta: metadata,
                                raised_at: SystemTime::now()
                                    .duration_since(UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_millis(),
                            });
                            self.dirty = true;
                        }
                    }
                }
                "elicitation/create" => {
                    let params = &message["params"];
                    let mode = params["mode"].as_str().unwrap_or("");
                    if mode == "form"
                        && let Some(id) = message.get("id")
                    {
                        let fallback_session = params["requestId"]
                            .is_null()
                            .then_some(self.snapshot.selected_id.as_str())
                            .filter(|value| !value.is_empty());
                        let session_id = params["sessionId"]
                            .as_str()
                            .filter(|value| !value.is_empty() && value.len() <= 256)
                            .or(fallback_session)
                            .unwrap_or("");
                        let has_scope = !session_id.is_empty() || !params["requestId"].is_null();
                        let schema = &params["requestedSchema"];
                        if has_scope
                            && schema["type"] == "object"
                            && schema["properties"]
                                .as_object()
                                .is_some_and(|items| items.len() <= 8)
                            && schema.to_string().len() <= 32_000
                            && safe_form_schema(schema)
                        {
                            let owner_session_id = if session_id.is_empty() {
                                String::new()
                            } else {
                                self.root_session(session_id)
                            };
                            self.snapshot.questions.push(Question {
                                id: id.to_string(),
                                session_id: session_id.into(),
                                owner_session_id,
                                message: params["message"]
                                    .as_str()
                                    .unwrap_or("Agent question")
                                    .chars()
                                    .take(500)
                                    .collect(),
                                schema: schema.clone(),
                                mode: "form".into(),
                                url: String::new(),
                                elicitation_id: String::new(),
                                raised_at: SystemTime::now()
                                    .duration_since(UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_millis(),
                            });
                            self.dirty = true;
                        } else {
                            self.write(
                                json!({"jsonrpc":"2.0","id":id,"result":{"action":"decline"}}),
                            );
                        }
                    } else if mode == "url"
                        && let Some(id) = message.get("id")
                    {
                        let url = params["url"].as_str().unwrap_or("");
                        let fallback_session = params["requestId"]
                            .is_null()
                            .then_some(self.snapshot.selected_id.as_str())
                            .filter(|value| !value.is_empty());
                        let session_id = params["sessionId"]
                            .as_str()
                            .filter(|value| !value.is_empty() && value.len() <= 256)
                            .or(fallback_session)
                            .unwrap_or("");
                        let has_scope = !session_id.is_empty() || !params["requestId"].is_null();
                        if has_scope
                            && safe_elicitation_url(url)
                            && params["elicitationId"]
                                .as_str()
                                .is_some_and(|value| !value.is_empty() && value.len() <= 256)
                        {
                            let owner_session_id = if session_id.is_empty() {
                                String::new()
                            } else {
                                self.root_session(session_id)
                            };
                            self.snapshot.questions.push(Question {
                                id: id.to_string(),
                                session_id: session_id.into(),
                                owner_session_id,
                                message: params["message"]
                                    .as_str()
                                    .unwrap_or("Continue in your browser")
                                    .chars()
                                    .take(500)
                                    .collect(),
                                schema: Value::Null,
                                mode: "url".into(),
                                url: url.chars().take(2048).collect(),
                                elicitation_id: params["elicitationId"]
                                    .as_str()
                                    .unwrap_or("")
                                    .into(),
                                raised_at: SystemTime::now()
                                    .duration_since(UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_millis(),
                            });
                            self.dirty = true;
                        } else {
                            self.write(
                                json!({"jsonrpc":"2.0","id":id,"result":{"action":"decline"}}),
                            );
                        }
                    } else if let Some(id) = message.get("id") {
                        self.write(json!({"jsonrpc":"2.0","id":id,"error":{"code":-32602,"message":"Unsupported elicitation mode"}}));
                    }
                }
                _ => {
                    if let Some(id) = message.get("id") {
                        self.write(json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":"Client method unavailable"}}));
                    }
                }
            }
            return;
        }
        let Some(id) = message["id"].as_u64() else {
            return;
        };
        let Some(pending) = self.pending.remove(&id) else {
            return;
        };
        if !message["error"].is_null() {
            let error = message["error"]["message"]
                .as_str()
                .unwrap_or("ACP request failed");
            self.snapshot.auth_required = message["error"]["code"] == -32000;
            self.snapshot.error = error.chars().take(500).collect();
            match pending {
                Pending::Initialize => {
                    self.disconnect(&format!("ACP initialization failed: {error}"));
                    return;
                }
                Pending::New { .. } => self.fail_operation("start", error),
                Pending::ConfigureStart { .. } => self.fail_operation("start", error),
                Pending::Prompt { session_id } => {
                    if let Some(previous) = self.prompt_fallback.remove(&session_id)
                        && self
                            .cached_responses
                            .get(&session_id)
                            .is_none_or(|response| response.text.is_empty())
                    {
                        self.cached_responses
                            .insert(session_id.clone(), previous.clone());
                        if self.snapshot.selected_id == session_id {
                            self.snapshot.response = Some(previous);
                        }
                    }
                    self.finish_prompt(&session_id);
                    self.fail_operation("prompt", error);
                }
                Pending::Load { session_id } => {
                    if let Some(previous) = self.load_fallback.remove(&session_id) {
                        self.cached_responses
                            .insert(session_id.clone(), previous.clone());
                        if self.snapshot.selected_id == session_id {
                            self.snapshot.response = Some(previous);
                        }
                    }
                    self.fail_operation("load", error);
                }
                Pending::Resume { .. } => self.fail_operation("load", error),
                Pending::Authenticate => self.fail_operation("auth", error),
                Pending::Logout => self.fail_operation("logout", error),
                _ => self.dirty = true,
            }
            return;
        }
        let result = &message["result"];
        match pending {
            Pending::Initialize => {
                if result["protocolVersion"].as_u64() != Some(ACP_PROTOCOL_VERSION) {
                    self.disconnect("Provider does not support ACP v1.");
                    return;
                }
                self.snapshot.connected = true;
                self.snapshot.capabilities = result["agentCapabilities"].clone();
                self.auth_methods = result["authMethods"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                self.snapshot.auth_methods = self.auth_methods.iter().map(|method| json!({
                    "id": method["id"], "name": method["name"], "description": method["description"], "type": method["type"]
                })).collect();
                self.snapshot.protocol_version =
                    result["protocolVersion"].as_u64().unwrap_or_default();
                self.snapshot.version =
                    result["agentInfo"]["version"].as_str().unwrap_or("").into();
                if let Some(candidate) = self.candidate_codex.take() {
                    match activate_codex_candidate(&candidate) {
                        Ok(()) => {
                            self.snapshot.adapter_version = candidate.version.clone();
                            self.snapshot.notice = format!(
                                "Codex ACP {} is active with ACP v1 compatibility.",
                                candidate.version
                            );
                            self.snapshot.notice_severity = "success".into();
                        }
                        Err(error) => {
                            self.snapshot.notice = format!(
                                "Codex ACP {} works for this session, but could not be saved for the next launch: {error}",
                                candidate.version
                            );
                            self.snapshot.notice_severity = "warning".into();
                        }
                    }
                }
                self.snapshot.update_available = newer_version(
                    &self.snapshot.latest_version,
                    &self.snapshot.adapter_version,
                );
                self.snapshot.error.clear();
                self.list(false);
            }
            Pending::List { append } => {
                if id < self.last_list_applied {
                    return;
                }
                self.last_list_applied = id;
                let incoming = result["sessions"].as_array().cloned().unwrap_or_default();
                if !append {
                    self.snapshot.sessions.retain(|session| {
                        session.active || session.id == self.snapshot.selected_id
                    });
                }
                for item in incoming {
                    let Some(id) = item["sessionId"].as_str() else {
                        continue;
                    };
                    if id.len() > 256 {
                        continue;
                    }
                    let title = item["title"]
                        .as_str()
                        .unwrap_or("Untitled task")
                        .chars()
                        .take(160)
                        .collect();
                    let directory = item["cwd"].as_str().unwrap_or("").into();
                    let additional_directories = item["additionalDirectories"]
                        .as_array()
                        .map(|entries| {
                            entries
                                .iter()
                                .filter_map(|entry| entry.as_str().map(str::to_owned))
                                .take(6)
                                .collect()
                        })
                        .unwrap_or_default();
                    if let Some(session) = self
                        .snapshot
                        .sessions
                        .iter_mut()
                        .find(|session| session.id == id)
                    {
                        session.title = title;
                        session.directory = directory;
                        session.additional_directories = additional_directories;
                    } else {
                        self.snapshot.sessions.push(Session {
                            id: id.into(),
                            title,
                            directory,
                            additional_directories,
                            updated: item["updatedAt"].as_str().unwrap_or("").into(),
                            ..Session::default()
                        });
                    }
                }
                self.snapshot.next_cursor = result["nextCursor"].as_str().map(str::to_owned);
            }
            Pending::New {
                prepare,
                text,
                config,
                files,
                directory,
                additional_directories,
                system_prompt,
                hybrid_whiteboard,
            } => {
                let Some(id) = result["sessionId"].as_str() else {
                    self.fail_operation("start", "Provider returned no session ID.");
                    return;
                };
                let id = id.to_owned();
                self.snapshot.sessions.insert(
                    0,
                    Session {
                        id: id.clone(),
                        title: if text.is_empty() {
                            "New task".into()
                        } else {
                            text.chars().take(80).collect()
                        },
                        directory,
                        additional_directories,
                        ready: true,
                        config_options: result["configOptions"]
                            .as_array()
                            .cloned()
                            .unwrap_or_default(),
                        modes: result.get("modes").cloned(),
                        ..Session::default()
                    },
                );
                self.loaded.insert(id.clone());
                self.snapshot.last_created_id = id.clone();
                self.snapshot.selected_id = id.clone();
                if prepare {
                    self.complete_operation(
                        "prepare",
                        true,
                        "Choose your model and permissions, then start the task",
                    );
                    if !config.is_empty() {
                        self.configure_then_prompt(
                            id,
                            text,
                            config,
                            files,
                            true,
                            system_prompt,
                            hybrid_whiteboard,
                        );
                    }
                } else {
                    self.configure_then_prompt(
                        id,
                        text,
                        config,
                        files,
                        false,
                        system_prompt,
                        hybrid_whiteboard,
                    );
                }
            }
            Pending::ConfigureStart {
                prepare,
                session_id,
                text,
                rest,
                files,
                system_prompt,
                hybrid_whiteboard,
            } => {
                self.apply_options(&session_id, result);
                self.configure_then_prompt(
                    session_id,
                    text,
                    rest,
                    files,
                    prepare,
                    system_prompt,
                    hybrid_whiteboard,
                );
            }
            Pending::Configure { session_id } => self.apply_options(&session_id, result),
            Pending::Load { session_id } => {
                self.loaded.insert(session_id.clone());
                if let Some(session) = self
                    .snapshot
                    .sessions
                    .iter_mut()
                    .find(|session| session.id == session_id)
                {
                    session.ready = true;
                }
                self.apply_options(&session_id, result);
                if let Some(previous) = self.load_fallback.remove(&session_id)
                    && self
                        .cached_responses
                        .get(&session_id)
                        .is_none_or(|response| response.text.is_empty())
                {
                    self.cached_responses.insert(session_id.clone(), previous);
                }
                if let Some(response) = self
                    .cached_responses
                    .get_mut(&session_id)
                    .map(Arc::make_mut)
                {
                    response.waiting = false;
                    project_presentation(response);
                }
                if self.snapshot.selected_id == session_id {
                    self.snapshot.response = self.cached_responses.get(&session_id).cloned();
                }
                self.complete_operation("load", true, "Session ready");
            }
            Pending::Resume { session_id } => {
                self.loaded.insert(session_id.clone());
                if let Some(session) = self
                    .snapshot
                    .sessions
                    .iter_mut()
                    .find(|session| session.id == session_id)
                {
                    session.ready = true;
                }
                self.apply_options(&session_id, result);
                if let Some(response) = self
                    .cached_responses
                    .get_mut(&session_id)
                    .map(Arc::make_mut)
                {
                    project_presentation(response);
                }
                if self.snapshot.selected_id == session_id {
                    self.snapshot.response = self.cached_responses.get(&session_id).cloned();
                }
                self.complete_operation("load", true, "Session ready");
            }
            Pending::Prompt { session_id } => {
                self.prompt_fallback.remove(&session_id);
                if let Some(response) = self
                    .cached_responses
                    .get_mut(&session_id)
                    .map(Arc::make_mut)
                {
                    if !result["usage"].is_null() {
                        response.usage = Some(result["usage"].clone());
                    }
                    response.stop_reason = result["stopReason"].as_str().unwrap_or("").into();
                    response.meta = if result["_meta"].to_string().len() <= 16_000 {
                        result["_meta"].clone()
                    } else {
                        Value::Null
                    };
                }
                self.finish_prompt(&session_id);
                if self.snapshot.selected_id == session_id {
                    self.snapshot.response = self.cached_responses.get(&session_id).cloned();
                }
                self.complete_operation("prompt", true, "Task finished");
                self.list(false);
            }
            Pending::Authenticate => {
                self.snapshot.auth_required = false;
                self.snapshot.error.clear();
                self.complete_operation("auth", true, "Signed in");
                self.list(false);
            }
            Pending::Logout => {
                self.snapshot.error.clear();
                self.complete_operation("logout", true, "Signed out of this provider");
            }
            Pending::SetMode { session_id } => self.apply_options(&session_id, result),
            Pending::Close { session_id } => {
                self.loaded.remove(&session_id);
                self.finish_prompt(&session_id);
                self.complete_operation("close", true, "Session closed");
            }
            Pending::Delete { session_id } => {
                self.snapshot
                    .sessions
                    .retain(|session| session.id != session_id);
                self.cached_responses.remove(&session_id);
                self.loaded.remove(&session_id);
                if self.snapshot.selected_id == session_id {
                    self.snapshot.selected_id.clear();
                    self.snapshot.response = None;
                }
                self.complete_operation("delete", true, "Session removed");
            }
            Pending::Fork {
                source_id,
                directory,
            } => {
                if let Some(id) = result["sessionId"].as_str() {
                    let title = self
                        .snapshot
                        .sessions
                        .iter()
                        .find(|session| session.id == source_id)
                        .map(|session| format!("Fork of {}", session.title))
                        .unwrap_or_else(|| "Forked task".into());
                    let additional_directories = self
                        .snapshot
                        .sessions
                        .iter()
                        .find(|session| session.id == source_id)
                        .map(|session| session.additional_directories.clone())
                        .unwrap_or_default();
                    self.snapshot.sessions.insert(
                        0,
                        Session {
                            id: id.into(),
                            title,
                            directory,
                            additional_directories,
                            ready: true,
                            config_options: result["configOptions"]
                                .as_array()
                                .cloned()
                                .unwrap_or_default(),
                            modes: result.get("modes").cloned(),
                            ..Session::default()
                        },
                    );
                    self.loaded.insert(id.into());
                    self.snapshot.selected_id = id.into();
                    self.snapshot.response = None;
                    self.complete_operation("fork", true, "Fork ready");
                }
            }
        }
        self.dirty = true;
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "Carries the existing ACP continuation fields without changing the protocol state machine"
    )]
    fn configure_then_prompt(
        &mut self,
        session_id: String,
        text: String,
        mut config: Vec<(String, String)>,
        files: Vec<String>,
        prepare: bool,
        system_prompt: String,
        hybrid_whiteboard: bool,
    ) {
        while !config.is_empty() {
            let (id, value) = config.remove(0);
            if let Some(session) = self
                .snapshot
                .sessions
                .iter()
                .find(|session| session.id == session_id)
            {
                if let Some(option) = session
                    .config_options
                    .iter()
                    .find(|option| option["id"] == id && valid_config_value(option, &value))
                {
                    let params = config_params(&session_id, &id, &value, option);
                    self.request(
                        "session/set_config_option",
                        params,
                        Pending::ConfigureStart {
                            prepare,
                            session_id,
                            text,
                            rest: config,
                            files,
                            system_prompt,
                            hybrid_whiteboard,
                        },
                    );
                    return;
                }
                if system_prompt.starts_with("Pluto response mode: meeting-optimize\n") {
                    self.fail_operation(
                        "start",
                        &format!("The saved {id} choice is unavailable. Choose a supported model in Agents, then retry."),
                    );
                    return;
                }
                self.snapshot.notice = format!(
                    "Saved {id} choice is unavailable in this project; using the provider default."
                );
                self.dirty = true;
            }
        }
        if !prepare {
            self.prompt(session_id, text, files, system_prompt, hybrid_whiteboard);
        }
    }

    fn apply_options(&mut self, session_id: &str, result: &Value) {
        if let Some(session) = self
            .snapshot
            .sessions
            .iter_mut()
            .find(|session| session.id == session_id)
        {
            if let Some(options) = result["configOptions"].as_array() {
                session.config_options = options.clone();
            }
            if !result["modes"].is_null() {
                session.modes = Some(result["modes"].clone());
            }
        }
    }

    fn finish_prompt(&mut self, session_id: &str) {
        if let Some(session) = self
            .snapshot
            .sessions
            .iter_mut()
            .find(|session| session.id == session_id)
        {
            session.active = false;
        }
        self.snapshot.active_count = self
            .snapshot
            .sessions
            .iter()
            .filter(|session| session.active)
            .count()
            + self.active_subagent_count();
        if let Some(response) = &mut self.snapshot.response {
            let response = Arc::make_mut(response);
            if response.session_id == session_id {
                response.waiting = false;
                project_presentation(response);
            }
        }
        if let Some(response) = self.cached_responses.get_mut(session_id).map(Arc::make_mut) {
            response.waiting = false;
            project_presentation(response);
        }
        self.snapshot.last_finished_response = self.cached_responses.get(session_id).cloned();
        self.snapshot
            .permissions
            .retain(|permission| permission.session_id != session_id);
        let questions: Vec<_> = self
            .snapshot
            .questions
            .iter()
            .filter(|item| item.session_id == session_id)
            .map(|item| item.id.clone())
            .collect();
        for id in questions {
            if let Ok(raw) = serde_json::from_str::<Value>(&id) {
                self.write(json!({"jsonrpc":"2.0","id":raw,"result":{"action":"cancel"}}));
            }
        }
        self.snapshot
            .questions
            .retain(|item| item.session_id != session_id);
        let child_count = self
            .active_children
            .keys()
            .filter(|child| {
                self.root_session(child) == session_id
                    && is_active_subagent_state(
                        self.child_states
                            .get(*child)
                            .map(String::as_str)
                            .unwrap_or("unknown"),
                    )
            })
            .count();
        if let Some(session) = self
            .snapshot
            .sessions
            .iter_mut()
            .find(|session| session.id == session_id)
        {
            session.child_count = child_count;
        }
        self.refresh_subagents();
        self.dirty = true;
    }

    fn update(&mut self, params: &Value) {
        let Some(session_id) = params["sessionId"].as_str() else {
            return;
        };
        let update = &params["update"];
        let kind = update["sessionUpdate"].as_str().unwrap_or("");
        if kind == "subagent_update" {
            if let Some(child) = update["sessionId"]
                .as_str()
                .filter(|id| !id.is_empty() && id.len() <= 256)
            {
                self.active_children.insert(child.into(), session_id.into());
                if let Some(name) = update["title"]
                    .as_str()
                    .or(update["name"].as_str())
                    .or(update["description"].as_str())
                {
                    self.child_names
                        .insert(child.into(), name.chars().take(100).collect());
                }
                let state = update["state"]["state"]
                    .as_str()
                    .or(update["state"].as_str())
                    .unwrap_or("unknown");
                self.child_states
                    .insert(child.into(), normalize_subagent_state(state));
            }
        } else if (kind == "tool_call" || kind == "tool_call_update")
            && let Some(child) = update["subagent"]["childSessionId"]
                .as_str()
                .filter(|id| !id.is_empty() && id.len() <= 256)
        {
            self.active_children
                .entry(child.into())
                .or_insert_with(|| session_id.into());
            self.child_states
                .entry(child.into())
                .or_insert_with(|| "running".into());
            if let Some(name) = update["subagent"]["displayName"].as_str() {
                self.child_names
                    .insert(child.into(), name.chars().take(100).collect());
            }
        }
        if self.provider == "opencode" {
            let child = &update["_meta"]["opencode/child-session"];
            if let (Some(id), Some(parent)) = (child["id"].as_str(), child["parentID"].as_str())
                && id.len() <= 256
            {
                self.active_children.insert(id.into(), parent.into());
                if let Some(title) = child["title"].as_str() {
                    self.child_names
                        .insert(id.into(), title.chars().take(100).collect());
                }
                self.child_states
                    .entry(id.into())
                    .or_insert_with(|| "running".into());
            }
        }
        let source_child = update["_meta"]["opencode/child-session"]["id"]
            .as_str()
            .or_else(|| {
                self.active_children
                    .contains_key(session_id)
                    .then_some(session_id)
            });
        let owner_id = source_child
            .map(|child| self.root_session(child))
            .unwrap_or_else(|| self.root_session(session_id));
        let child_name = source_child
            .and_then(|child| self.child_names.get(child))
            .cloned()
            .unwrap_or_else(|| "Subagent".into());
        match kind {
            "agent_message_chunk" => {
                if let Some(text) = update["content"]["text"].as_str() {
                    let response = self
                        .cached_responses
                        .entry(owner_id.clone())
                        .or_insert_with(|| {
                            Arc::new(Response {
                                session_id: owner_id.clone(),
                                waiting: true,
                                ..Response::default()
                            })
                        });
                    let response = Arc::make_mut(response);
                    if let Some(child) = source_child {
                        let id = format!("child:{child}");
                        let detail = text.chars().take(220).collect::<String>();
                        if let Some(step) = response.steps.iter_mut().find(|step| step.id == id) {
                            step.detail = detail;
                        } else if response.steps.len() < 100 {
                            response.steps.push(Step {
                                id,
                                title: child_name.clone(),
                                detail,
                                status: "in_progress".into(),
                                ..Step::default()
                            });
                        }
                    } else {
                        if let Some(message_id) = update["messageId"].as_str() {
                            if self
                                .message_ids
                                .get(session_id)
                                .is_some_and(|previous| previous != message_id)
                            {
                                response.text.clear();
                                response.truncated = false;
                                response.presentation = None;
                            }
                            self.message_ids
                                .insert(session_id.into(), message_id.into());
                        }
                        let remaining = 400_000usize.saturating_sub(response.text.len());
                        if text.len() > remaining {
                            response.truncated = true;
                        }
                        response
                            .text
                            .push_str(&text[..text.floor_char_boundary(remaining.min(text.len()))]);
                    }
                }
            }
            "tool_call" | "tool_call_update" => {
                let mut content = self.safe_tool_content(&update["content"]);
                if content.is_empty() {
                    content = self.safe_tool_content(&update["rawOutput"]);
                }
                let meta = if update["_meta"].to_string().len() <= 32_000 {
                    update["_meta"].clone()
                } else {
                    Value::Null
                };
                if let Some(response) = self.cached_responses.get_mut(&owner_id).map(Arc::make_mut)
                {
                    let raw_id = update["toolCallId"].as_str().unwrap_or("");
                    let id = if let Some(child) = source_child {
                        if raw_id.starts_with(child) {
                            raw_id.to_owned()
                        } else {
                            format!("{child}:{raw_id}")
                        }
                    } else {
                        raw_id.to_owned()
                    };
                    if !id.is_empty() {
                        let step = response.steps.iter_mut().find(|step| step.id == id);
                        if let Some(step) = step {
                            if let Some(title) = update["title"].as_str() {
                                step.title = if source_child.is_some() {
                                    format!("{child_name}: {title}")
                                } else {
                                    title.into()
                                }
                                .chars()
                                .take(120)
                                .collect();
                            }
                            if let Some(status) = update["status"].as_str() {
                                step.status = status.into();
                            }
                            if let Some(detail) = tool_locations(update) {
                                step.detail = detail;
                            }
                            if !content.is_empty() {
                                step.content = content;
                            }
                            if !meta.is_null() {
                                step.meta = meta;
                            }
                        } else if response.steps.len() < 100 {
                            let title = update["title"].as_str().unwrap_or("Tool action");
                            response.steps.push(Step {
                                id,
                                title: if source_child.is_some() {
                                    format!("{child_name}: {title}")
                                } else {
                                    title.into()
                                }
                                .chars()
                                .take(120)
                                .collect(),
                                detail: tool_locations(update).unwrap_or_default(),
                                status: update["status"].as_str().unwrap_or("pending").into(),
                                kind: update["kind"]
                                    .as_str()
                                    .unwrap_or("")
                                    .chars()
                                    .take(40)
                                    .collect(),
                                content,
                                meta,
                            });
                        }
                    }
                }
            }
            "plan" if source_child.is_none() => {
                if let Some(response) = self.cached_responses.get_mut(session_id).map(Arc::make_mut)
                {
                    response.plan = update["entries"]
                        .as_array()
                        .map(|entries| entries.iter().take(30).cloned().collect())
                        .unwrap_or_default();
                }
            }
            "compaction_update" if source_child.is_none() => {
                let id = update["compactionId"].as_str().unwrap_or("");
                if !id.is_empty() {
                    let summary = self.safe_tool_content(&update["summary"]);
                    let summary_text = content_blocks_text(&summary);
                    let status = update["status"]
                        .as_str()
                        .unwrap_or("in_progress")
                        .to_owned();
                    let compaction = json!({"compactionId":id,"status":status,"error":update["error"],"summary":summary,"summaryText":summary_text,"meta":update["_meta"]});
                    let response = self
                        .cached_responses
                        .entry(owner_id.clone())
                        .or_insert_with(|| {
                            Arc::new(Response {
                                session_id: owner_id.clone(),
                                waiting: true,
                                ..Response::default()
                            })
                        });
                    let response = Arc::make_mut(response);
                    if let Some(item) = response
                        .compactions
                        .iter_mut()
                        .find(|item| item["compactionId"] == id)
                    {
                        *item = compaction;
                    } else if response.compactions.len() < 16 {
                        response.compactions.push(compaction);
                    }
                    let step_id = format!("compaction:{id}");
                    if let Some(step) = response.steps.iter_mut().find(|step| step.id == step_id) {
                        step.status = status.clone();
                        step.detail = update["error"]
                            .as_str()
                            .unwrap_or("Context summary refreshed")
                            .chars()
                            .take(240)
                            .collect();
                    } else if response.steps.len() < 100 {
                        response.steps.push(Step {
                            id: step_id,
                            title: "Updating context".into(),
                            detail: update["error"]
                                .as_str()
                                .unwrap_or("Context summary refreshed")
                                .chars()
                                .take(240)
                                .collect(),
                            status,
                            kind: "compaction".into(),
                            ..Step::default()
                        });
                    }
                }
            }
            "compaction_summary_chunk" if source_child.is_none() => {
                let id = update["compactionId"].as_str().unwrap_or("");
                if let Some(response) = self.cached_responses.get_mut(&owner_id).map(Arc::make_mut)
                    && let Some(compaction) = response
                        .compactions
                        .iter_mut()
                        .find(|item| item["compactionId"] == id)
                {
                    let content = safe_content_block(&update["content"]);
                    if let Some(content) = content {
                        let summary = compaction["summary"].as_array_mut();
                        if let Some(summary) = summary
                            && summary.len() < 200
                        {
                            summary.push(content.clone());
                        }
                        let text = content_block_text(&content);
                        if !text.is_empty() {
                            let combined = format!(
                                "{}{}",
                                compaction["summaryText"].as_str().unwrap_or(""),
                                text
                            );
                            compaction["summaryText"] =
                                json!(combined.chars().take(40_000).collect::<String>());
                        }
                    }
                }
            }
            "usage_update" if source_child.is_none() => {
                if let Some(response) = self.cached_responses.get_mut(session_id).map(Arc::make_mut)
                {
                    response.usage = Some(
                        json!({"used":update["used"],"size":update["size"],"cost":update["cost"]}),
                    );
                }
            }
            "notice" => {
                let title = update["title"].as_str().unwrap_or("");
                let detail = update["description"].as_str().unwrap_or("");
                self.snapshot.notice_severity = update["severity"]
                    .as_str()
                    .unwrap_or("info")
                    .chars()
                    .take(16)
                    .collect();
                self.snapshot.notice = if detail.is_empty() {
                    title.to_owned()
                } else {
                    format!("{title} — {detail}")
                }
                .chars()
                .take(500)
                .collect();
            }
            "subagent_spawned"
                if !self.snapshot.capabilities["sessionCapabilities"]["subagents"].is_null() =>
            {
                if let Some(child) = update["subagentSessionId"].as_str() {
                    self.active_children.insert(child.into(), session_id.into());
                    self.child_states.insert(child.into(), "running".into());
                    if let Some(name) = update["name"].as_str().or(update["title"].as_str()) {
                        self.child_names
                            .insert(child.into(), name.chars().take(100).collect());
                    }
                }
            }
            "subagent_state_update"
                if !self.snapshot.capabilities["sessionCapabilities"]["subagents"].is_null() =>
            {
                if let Some(child) = update["subagentSessionId"].as_str() {
                    let state = update["state"]
                        .as_str()
                        .or(update["status"].as_str())
                        .unwrap_or("running");
                    let normalized = normalize_subagent_state(state);
                    self.child_states.insert(child.into(), normalized.clone());
                    if let Some(response) =
                        self.cached_responses.get_mut(&owner_id).map(Arc::make_mut)
                        && let Some(step) = response
                            .steps
                            .iter_mut()
                            .find(|step| step.id == format!("child:{child}"))
                    {
                        step.status = normalized;
                    }
                    self.active_children.insert(child.into(), session_id.into());
                }
            }
            "config_option_update" => {
                if let Some(options) = update["configOptions"].as_array()
                    && let Some(session) = self
                        .snapshot
                        .sessions
                        .iter_mut()
                        .find(|session| session.id == session_id)
                {
                    session.config_options = options.clone();
                }
            }
            "current_mode_update" => {
                if let Some(session) = self
                    .snapshot
                    .sessions
                    .iter_mut()
                    .find(|session| session.id == session_id)
                    && let Some(modes) = &mut session.modes
                {
                    modes["currentModeId"] = update["currentModeId"].clone();
                }
            }
            "available_commands_update" => {
                if let Some(session) = self
                    .snapshot
                    .sessions
                    .iter_mut()
                    .find(|session| session.id == session_id)
                {
                    session.commands = update["availableCommands"]
                        .as_array()
                        .cloned()
                        .unwrap_or_default();
                }
            }
            "session_info_update" => {
                if let Some(session) = self
                    .snapshot
                    .sessions
                    .iter_mut()
                    .find(|session| session.id == session_id)
                {
                    if let Some(title) = update.get("title") {
                        session.title = title.as_str().unwrap_or("").chars().take(160).collect();
                    }
                    if let Some(updated) = update.get("updatedAt") {
                        session.updated = updated.as_str().unwrap_or("").chars().take(64).collect();
                    }
                }
            }
            _ => {}
        }
        let child_count = self
            .active_children
            .keys()
            .filter(|child| {
                self.root_session(child) == owner_id
                    && is_active_subagent_state(
                        self.child_states
                            .get(*child)
                            .map(String::as_str)
                            .unwrap_or("unknown"),
                    )
            })
            .count();
        if let Some(session) = self
            .snapshot
            .sessions
            .iter_mut()
            .find(|session| session.id == owner_id)
        {
            session.child_count = child_count;
        }
        self.snapshot.active_count = self
            .snapshot
            .sessions
            .iter()
            .filter(|session| session.active)
            .count()
            + self.active_subagent_count();
        self.refresh_subagents();
        if self.snapshot.selected_id == owner_id
            && !self.load_fallback.contains_key(&owner_id)
            && let Some(response) = self.cached_responses.get(&owner_id)
        {
            self.snapshot.response = Some(response.clone());
        }
        self.dirty = true;
    }

    fn complete_operation(&mut self, kind: &str, ok: bool, notice: &str) {
        if ok {
            self.snapshot.error.clear();
        }
        self.snapshot.operation_revision = self.snapshot.operation_revision.wrapping_add(1);
        self.snapshot.operation_kind = kind.into();
        self.snapshot.operation_ok = ok;
        self.snapshot.notice = notice.into();
        self.snapshot.notice_severity = if ok { "success" } else { "error" }.into();
        self.dirty = true;
    }

    fn refresh_subagents(&mut self) {
        self.snapshot.subagents = self
            .active_children
            .iter()
            .take(32)
            .map(|(id, parent_id)| Subagent {
                id: id.clone(),
                parent_id: parent_id.clone(),
                name: self
                    .child_names
                    .get(id)
                    .cloned()
                    .unwrap_or_else(|| "Subagent".into()),
                state: self
                    .child_states
                    .get(id)
                    .cloned()
                    .unwrap_or_else(|| "running".into()),
            })
            .collect();
    }
    fn active_subagent_count(&self) -> usize {
        self.child_states
            .values()
            .filter(|state| is_active_subagent_state(state))
            .count()
    }
    fn fail_operation(&mut self, kind: &str, error: &str) {
        self.snapshot.error = error.chars().take(500).collect();
        self.complete_operation(kind, false, "Action was not sent");
    }
    fn publish(&mut self, force: bool) {
        if self.dirty && (force || self.last_publish.elapsed() >= Duration::from_millis(55)) {
            self.dirty = false;
            self.last_publish = Instant::now();
            let _ = self.outgoing.send(self.snapshot.clone());
        }
    }
}

fn valid_config_value(option: &Value, value: &str) -> bool {
    if option["type"] == "boolean" {
        return value == "true" || value == "false";
    }
    option["options"].as_array().is_some_and(|options| {
        options.iter().any(|item| {
            item["value"] == value
                || item["options"]
                    .as_array()
                    .is_some_and(|group| group.iter().any(|entry| entry["value"] == value))
        })
    })
}

fn compose_prompt_text(system_prompt: &str, user_text: &str, hybrid_whiteboard: bool) -> String {
    if let Some(instructions) =
        system_prompt.strip_prefix("Pluto response mode: meeting-optimize\n")
    {
        return format!("Meeting transcript editing request. {instructions}\n\n{user_text}");
    }
    if let Some(style) = system_prompt.strip_prefix("Pluto response mode: wake-answer\n") {
        let style: String = style.chars().take(6_000).collect();
        return format!(
            "This turn is a quick voice-assistant request in a compact desktop overlay. Answer the current request directly in concise natural plain text. Do not generate island-ui JSON, visual components, tables, diagrams, or code blocks. Do not perform coding-task startup checks: no git status, repository inspection, uncommitted-file checks, project discovery, plans, or edits unless the user explicitly requests code/project work. For a live desktop question, use only the minimal relevant app/process check needed; never inspect a repository to determine whether an app is running. Do not claim live status without evidence. Use tools only when needed to answer or perform the actual request. Follow all approval and safety requirements. Usually answer in 1–3 short sentences, with the result first.\n\nAnswer style:\n{style}\n\nUser request:\n{user_text}"
        );
    }
    // Ask supplies a per-request mode header outside its editable instructions.
    // Do not append the visual-generation catalog to a teleprompter request.
    if let Some(style) = system_prompt.strip_prefix("Pluto response mode: spoken-script\n") {
        let style: String = style.chars().take(14_000).collect();
        return format!(
            "This turn is for Ask, a compact spoken-answer view. Follow these answer instructions and conversation context:\n{style}\n\nUser question:\n{user_text}"
        );
    }
    let instructions: String = system_prompt.trim().chars().take(6_000).collect();
    let mut prompt = String::new();
    if !instructions.is_empty() {
        prompt.push_str(&format!(
            "Additional instructions configured in Pluto:\n{instructions}\n\n"
        ));
    }
    prompt.push_str(r###"Pluto displays this answer in a narrow desktop panel with an optional full-answer view. Give the useful result first. Use ordinary Markdown prose for a simple answer. If verified information benefits from a visual, add at most one fenced `island-ui` JSON block after the prose. The JSON root is {"version":1,"title":"optional","elements":[...]}. Use 1–6 elements and short labels. The app chooses all layout, color, and animation; never emit HTML, CSS, JavaScript, URLs, actions, or image assets inside the JSON.

Presentation priority (apply by meaning, not decoration):
1. Text for simple answers, fictional human movement, character descriptions and unsupported physical subjects. Superman flying, Iron Man, skeletons, poses and human figures must NOT automatically trigger visuals. Human rigs are unavailable, even when explicitly requested. An explicit request to visualize these may use a simple conceptual flow/relationship diagram, never a replacement figure assembled from blocks, curves or scribbles. Respect text-only requests.
2. Relationships, processes and functional explanations -> system_map, timeline, comparison or table. A heart explanation should show blood-flow relationships, not invented anatomical geometry. Use plain labels; distinguish function from physical anatomy.
3. Simple spatial explainers where a few clear strokes/shapes suffice -> whiteboard_sketch ONLY when enabled below. Prefer familiar silhouettes or bounded geometric shapes; skip drawings that need artistic or anatomical skill.
4. Known physical object structure -> space_station for ISS, assembly for TVs, remotes, computers and other devices expressible as panels/blocks/cylinders. Do NOT use these for people, organic anatomy or fictional costumes. A device's logical connections still use system_map. An explicit visual request does not make an inaccurate representation acceptable.
5. Charts only for actual numeric data. Raw visual_scene is a last resort for a simple abstract mechanism, never for people or a detailed object.
Use at most ONE main explanatory visual, selected by the question's actual purpose. Do not show a visual merely because a topic is scientific. Retain useful existing tables and other semantic components. If no reliable representation fits, explain in text. Do not print this routing decision.

Preferred visual components, designed to work with small models:
- assembly(label,parts:[{label,detail,shape,position?,size?}]): 1–12 selectable isometric parts. shape is block, cylinder, sphere, panel, solar_array, truss, module, or radiator. The app draws shaded faces, depth, panel grids, and module seams, and handles all labels outside the drawing. position is [x,y,z] in a small world grid (-8..8); size is [width,depth,height] (0.15..8). Omit position and size for an automatically spaced exploded parts view. Supply positions for every part or omit positions for every part; never mix the two. Use positions only when the spatial arrangement is known. Cylinders/modules stand along z. Label simplified or exploded views as such; never imply CAD precision or manufacture connections. This is the preferred component for machines, devices, layered objects, and physical parts. Use system_map for logical links instead. Keep sizes within a similar order of magnitude so all parts remain visible.
- space_station: no fields. Interactive ISS illustration with recognizable solar wings, truss, radiators, and module forms. Use for explanations of station structure, modules, or power. It is simplified, not a complete docking map or current configuration. Do not use for unrelated space topics or news.
- system_map(label,nodes:[{id,label,detail,symbol?}],links:[{from,to,label}]): 2–8 nodes and 1–12 directed links. Unique IDs; links must reference existing nodes; no self-links. Node label <=60 characters, detail <=400, link label <=80. symbol is module, storage, processor, person, energy, or cloud. The app handles placement, readable labels, selectable details, and connection highlighting. Use for architecture, workflows, cause/effect, biological systems, and dependencies. Supply only supported relationships, no coordinates.
- line_chart(label,unit?,points:[{label,value}]): 2–24 real numeric observations in meaningful order. Categories are evenly spaced, not a proportional time axis. State units. Never invent a series; use bar_chart for magnitude comparisons.

Examples of automatic choices: 'How do ISS modules fit together?' -> space_station; 'How does a browser load a page?' -> system_map; 'Compare these monthly readings' -> line_chart with the actual readings; 'When was the ISS launched?' -> short text. Use one main visual and a concise takeaway. Do not repeat all of its details in extra cards.
Minimal valid examples:
```island-ui
{"version":1,"elements":[{"type":"assembly","label":"Exploded device layers · schematic","parts":[{"label":"Base","detail":"Supports the assembly.","shape":"block","position":[0,0,0],"size":[3,2,0.3]},{"label":"Circuit board","detail":"Carries the electronic components.","shape":"panel","position":[0,0,1],"size":[2.7,1.7,0.15]},{"label":"Cover","detail":"Protects the internals.","shape":"block","position":[0,0,2],"size":[3,2,0.3]}]}]}
```
```island-ui
{"version":1,"elements":[{"type":"space_station"}]}
```
```island-ui
{"version":1,"elements":[{"type":"system_map","label":"Request and response","nodes":[{"id":"client","label":"Browser","detail":"Sends an HTTP request.","symbol":"person"},{"id":"server","label":"Server","detail":"Handles the request and returns a response.","symbol":"processor"}],"links":[{"from":"client","to":"server","label":"HTTP request"},{"from":"server","to":"client","label":"HTTP response"}]}]}
```
Before sending, check required fields, valid component names, unique IDs, valid link targets, numeric ranges, and whether the visual answers the question. Prefer a small valid semantic component over a complicated scene. Do not print these checks or your presentation decision.

Choose a component by the information, not for decoration:
- Current work: phase_card(title,detail?,state?); state is todo, in_progress, or done. Its subtle motion is app-owned.
- Outcome states: status_strip(items:[{label,tone?}]); tone is primary, neutral, info, success, warning, or danger.
- Parallel facts: metric_grid(items:[{label,value,detail?}]); use 2–4 items.
- Two alternatives or before/after: comparison(left:{label,text,tone?},right:{label,text,tone?}).
- Task progress: checklist(items:[{text,state}]) or timeline(items:[{label,detail?,state?}]).
- Important warning or next action: callout(text,title?,tone?).
- Exact small facts: key_value(items:[{key,value}]); code(code,language?) only when code is useful.
Other supported types, only when better suited: heading(text,level? 1–3), text(text,tone?), badge(label,tone?), stat(label,value,detail?), list(items:[text],ordered?), table(columns:[text],rows:[[text]]), quote(text,attribution?), progress(label,value,max?,detail?), bar_chart(label,items:[{label,value}]), file_change(path,status,summary?), section(title?,elements:[...]), columns(columns:[[...],[...]]), divider. Visual types: black_hole has no fields; waveform(label,values:[-1..1], 8–64 samples); heatmap(label,columns:[2–8 labels],rows:[{label,values:[0..1]}]); cross_section(label,layers:[{label,detail?}], 2–6 layers); visual_scene(label,frames:[{label,caption?,marks:[...]}]) for a mechanism that changes across 1–4 stages. File status is added, modified, deleted, or renamed.

For visual_scene the app draws marks on a 100 wide by 70 high canvas. A frame has 2–18 marks and at least one geometric mark. Mark kinds: dot(x,y,r), ring(x,y,r), ellipse(x,y,rx,ry), line(x1,y1,x2,y2), curve(x1,y1,cx,cy,x2,y2), label(x,y,text). Geometry coordinates must be in x=0..100 and y=0..70; radius 0.5..35, ellipse rx 0.5..48 and ry 0.5..34. Geometric marks may include tone: primary, neutral, info, success, warning, or danger. Use the same coordinates for the same object in each stage so it does not jump. Draw only what explains the mechanism; labels are short and limited to five per frame. Curves are quadratic, with the control point (cx,cy) shaping the bend. A scene is a conceptual illustration, never a measurement or simulation. The app supplies the color, motion, stage controls, and reduced-motion behavior.

Rules: strict JSON with double quotes and no trailing commas; use only facts actually observed. Do not invent counts, statuses, progress, or changed files. Omit missing data. Do not repeat a visual's facts in prose. If the structure is uncertain, omit the visual and write prose. Never retry a task merely to create a visual.

Schema examples only; replace every value with observed facts and never copy the example counts or statuses into a real answer.
Example for a completed task:
```island-ui
{"version":1,"title":"Result","elements":[{"type":"status_strip","items":[{"label":"Completed","tone":"success"}]},{"type":"metric_grid","items":[{"label":"Files changed","value":"3"},{"label":"Build","value":"Passed"}]}]}
```
Example for ongoing work:
```island-ui
{"version":1,"elements":[{"type":"phase_card","title":"Updating the interface","detail":"Adjusting the response layout","state":"in_progress"}]}
```
Example of a two-stage visual mechanism:
```island-ui
{"version":1,"elements":[{"type":"visual_scene","label":"A path bends near a mass","frames":[{"label":"Approach","caption":"A ray approaches the mass.","marks":[{"kind":"ring","x":52,"y":35,"r":12,"tone":"neutral"},{"kind":"line","x1":8,"y1":24,"x2":43,"y2":24,"tone":"primary"},{"kind":"label","x":52,"y":56,"text":"Mass"}]},{"label":"Deflection","caption":"The path curves as it passes.","marks":[{"kind":"ring","x":52,"y":35,"r":12,"tone":"neutral"},{"kind":"curve","x1":8,"y1":24,"cx":52,"cy":18,"x2":92,"y2":40,"tone":"primary"},{"kind":"label","x":52,"y":56,"text":"Mass"}]}]}]}
```

"###);
    if hybrid_whiteboard {
        prompt.push_str(r###"

Whiteboard explainers are enabled. Use whiteboard_sketch when the position, containment, shape, or attachment of parts is itself the explanation. Use system_map for signal paths, causal chains, processing steps, and other purely functional relationships, even when the user says "visually." Human figures, skeletons, armor and anatomical drawings are unavailable. Do not fabricate a recognizable object from unrelated rectangles or arrows. A sketch must answer ONE visual question: show how parts fit OR show one short physical interaction. Do not combine an exterior silhouette, an inventory of every internal part, and a complete process flow in the same board.

Canvas rules: automatic layout uses a fixed 160 by 70 SVG viewBox, scaled uniformly to the available panel width (including Full view); pixel size is not fixed. Keep captions outside the drawing. Item outline coordinates and strokes with on:<id> use a local 0..100 by 0..100 frame, never global pixels. Standalone strokes use x=0..100,y=0..70, mapped horizontally to the canvas width. Keep standalone marks inside x=4..96,y=4..66 for padding. Avoid combining automatic placement with global pen coordinates.
Paint rules: background and item fill accept none, surface, primary, neutral, info, success, warning, danger or a six-digit #RRGGBB color. A stroke may use fill only with closed:true and at least three points. Filled outlines support arbitrary polygon silhouettes; smooth:true interpolates curved edges. No CSS, URLs or gradients in paint values. Prefer one background and 1–3 contrasting fills; the app chooses readable label ink. Use circle/aspect for spatial subjects; a row of oval boxes is not an illustration.
Whiteboard contract: whiteboard_sketch(label,background?,items?,links?,strokes?,caption?). The app owns responsive layout, grouping, boundary attachment, labels, color and routing. You do NOT need to calculate canvas positions. Provide meaningful nouns and connections:
- items: 1–12 {id,label,shape?,aspect?,fill?,outline?,smooth?,parent?,place?,scale?,emphasis?,detail?}. Unique short ids. shape is rectangle (default), ellipse, circle or diamond. circle preserves equal width and height. aspect is optional width/height, from 0.2 to 5; use it for recognizable proportions instead of stretching shapes to fill their cells. fill sets the interior color. For a recognizable silhouette that these cannot express, outline:[[x,y],...] may give 3–24 contour points in the item's OWN 0–100 frame; it replaces the basic shape and moves with the item. Start with a few meaningful corners; set smooth:true only for a curved contour. The app closes the outline. There is no supported-object list. parent refers to another id. A parent may contain children (place:inside or omitted) one level deep. An attached part may instead use place:left|right|above|below relative to its parent. scale:small|medium|large is a hint. emphasis:true marks the key part. The app writes short labels inside shapes when legible and otherwise uses a numbered legend; detail appears on selection.
- links: up to 16 {from,to,label?}, referring to item ids. Links attach to shape edges and route around unrelated parts and drawn contours. Use links for real physical or causal connections. Do not draw an arrow with coordinates when ids exist. The app retains a readable relationship list if a crowded route cannot be shown.
- strokes: optional, up to 24 {points:[[x,y],...],on?,role?,closed?,smooth?,fill?}. With items, EVERY stroke must use on:<item id> and points in that item's 0..100 local frame. The app ignores loose canvas strokes in a structured sketch because they cannot follow responsive part placement. Without items, x=0..100,y=0..70 is the whole canvas. role:contour|motion|annotation. closed:true makes a silhouette; smooth:true is for organic curves. Keep straight corners straight. When items describe the drawing, strokes:[] is valid. Do not trace the same part twice or draw a wire as a freehand line when a link exists.

For lower-capability models, work in this order: (1) write the one spatial point the viewer must understand, (2) choose one main subject and only the parts needed for that point, (3) put true contained parts under its id; put external inputs beside it using place:left or right, (4) add only essential physical links, (5) use an item outline only when a few points improve recognition, (6) add only necessary pen marks anchored to an item, (7) write one sentence of interpretation. Before output, mentally inspect the sketch: no giant source shape, no arrows through words, no overlapping independent parts, no detached numbering, no redundant contour over structured shapes. A realistic object need not be represented by a generic pipeline of boxes. If the drawing would misrepresent the subject or needs too many competing links, use an existing semantic diagram or text.

Optional legacy x,y,width,height fields on items are accepted for older answers but should not be generated. The app corrects overlapping or partial geometry by laying out the parts again. The older base field may appear in saved answers but should not be generated for new sketches. No SVG, HTML, CSS, JavaScript, arbitrary colors, remote assets or image URLs.

Compact example of containment (generic spatial sketch, not an object template): {"type":"whiteboard_sketch","label":"How the layers fit","items":[{"id":"frame","label":"Outer frame","emphasis":true,"detail":"Holds the two fitted layers."},{"id":"front","label":"Front layer","parent":"frame"},{"id":"rear","label":"Rear layer","parent":"frame"}],"links":[],"strokes":[],"caption":"Both layers fit within the outer frame."}

"###);
    }
    if !user_text.trim().is_empty() {
        prompt.push_str("User's current task:\n");
        prompt.push_str(user_text);
    }
    prompt
}

fn project_presentation(response: &mut Response) {
    if let Some((presentation, prose)) = crate::agent_presentation::extract(&response.text) {
        response.presentation =
            crate::agent_presentation::apply_visual_policy(presentation, &response.visual_policy);
        response.text = prose;
    }
}

fn config_params(session_id: &str, id: &str, value: &str, option: &Value) -> Value {
    if option["type"] == "boolean" {
        json!({"sessionId":session_id,"configId":id,"type":"boolean","value":value == "true"})
    } else {
        json!({"sessionId":session_id,"configId":id,"value":value})
    }
}

fn wipe_mcp_secrets(message: &mut Value) {
    let Some(params) = message.get_mut("params").and_then(Value::as_object_mut) else {
        return;
    };
    let Some(servers) = params.get_mut("mcpServers").and_then(Value::as_array_mut) else {
        return;
    };
    for server in servers {
        for collection in ["env", "headers"] {
            if let Some(entries) = server.get_mut(collection).and_then(Value::as_array_mut) {
                for entry in entries {
                    if let Some(Value::String(value)) = entry.get_mut("value") {
                        unsafe {
                            for byte in value.as_mut_vec() {
                                std::ptr::write_volatile(byte, 0);
                            }
                        }
                    }
                }
            }
        }
    }
}

fn normalize_subagent_state(state: &str) -> String {
    match state {
        "in_progress" => "running".into(),
        "completed" | "complete" | "succeeded" => "idle".into(),
        "canceled" => "cancelled".into(),
        "running" | "requires_action" | "unknown" | "idle" | "failed" | "cancelled"
        | "disconnected" => state.into(),
        other => other.chars().take(32).collect(),
    }
}

fn is_active_subagent_state(state: &str) -> bool {
    matches!(
        state,
        "running" | "in_progress" | "requires_action" | "unknown"
    ) || !matches!(
        state,
        "idle"
            | "completed"
            | "complete"
            | "succeeded"
            | "failed"
            | "cancelled"
            | "canceled"
            | "disconnected"
    )
}

fn tool_locations(update: &Value) -> Option<String> {
    let paths: Vec<_> = update["locations"]
        .as_array()?
        .iter()
        .take(3)
        .filter_map(|item| item["path"].as_str().or_else(|| item.as_str()))
        .map(|path| path.chars().take(100).collect::<String>())
        .collect();
    (!paths.is_empty()).then(|| paths.join(" · "))
}

fn capture_pipe<R: Read + Send + 'static>(
    mut reader: R,
    capture: Arc<Mutex<TerminalCapture>>,
) -> Option<JoinHandle<()>> {
    thread::Builder::new()
        .name("island-acp-terminal-output".into())
        .spawn(move || {
            let mut bytes = [0_u8; 8192];
            while let Ok(count) = reader.read(&mut bytes) {
                if count == 0 {
                    break;
                }
                let chunk = String::from_utf8_lossy(&bytes[..count]);
                let Ok(mut output) = capture.lock() else {
                    break;
                };
                output.text.push_str(&chunk);
                if output.text.len() > output.byte_limit {
                    let excess = output.text.len() - output.byte_limit;
                    let boundary = output.text.floor_char_boundary(excess);
                    output.text.drain(..boundary);
                    output.truncated = true;
                }
            }
        })
        .ok()
}

fn read_acp_messages<R: Read>(stream: R, tx: Sender<Value>) {
    let mut reader = BufReader::new(stream);
    let mut line = Vec::with_capacity(4096);
    let mut oversized = false;
    loop {
        let (consumed, newline, chunk) = match reader.fill_buf() {
            Ok([]) => break,
            Ok(available) => {
                let newline = available.iter().position(|byte| *byte == b'\n');
                let consumed = newline.map_or(available.len(), |index| index + 1);
                let payload_len = newline.unwrap_or(consumed);
                let chunk =
                    if !oversized && line.len().saturating_add(payload_len) <= MAX_ACP_LINE_BYTES {
                        Some(available[..payload_len].to_vec())
                    } else {
                        None
                    };
                (consumed, newline.is_some(), chunk)
            }
            Err(_) => break,
        };
        if let Some(chunk) = chunk {
            line.extend_from_slice(&chunk);
        } else {
            oversized = true;
            line.clear();
        }
        reader.consume(consumed);
        if newline {
            if !oversized && let Ok(value) = serde_json::from_slice::<Value>(&line) {
                let _ = tx.send(value);
            }
            line.clear();
            oversized = false;
        }
    }
    if !oversized
        && !line.is_empty()
        && let Ok(value) = serde_json::from_slice::<Value>(&line)
    {
        let _ = tx.send(value);
    }
}

fn path_key(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_lowercase()
}

fn safe_elicitation_url(value: &str) -> bool {
    if value.is_empty()
        || value.len() > 2048
        || value.chars().any(char::is_control)
        || value.chars().any(char::is_whitespace)
    {
        return false;
    }
    let Some((scheme, rest)) = value.split_once("://") else {
        return false;
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if authority.is_empty() || authority.contains('@') {
        return false;
    }
    if scheme.eq_ignore_ascii_case("https") {
        return true;
    }
    if !scheme.eq_ignore_ascii_case("http") {
        return false;
    }
    let host = if authority.starts_with('[') {
        authority
            .split_once(']')
            .map(|(host, _)| host.trim_start_matches('['))
            .unwrap_or("")
    } else {
        authority.split(':').next().unwrap_or("")
    };
    host.eq_ignore_ascii_case("localhost") || host == "127.0.0.1" || host == "::1"
}

fn safe_content_block(block: &Value) -> Option<Value> {
    if block["type"] == "content" {
        return safe_content_block(&block["content"]);
    }
    match block["type"].as_str()? {
        "text" => block["text"].as_str().map(
            |text| json!({"type":"text","text":text.chars().take(24_000).collect::<String>()}),
        ),
        "image" | "audio" => block["data"]
            .as_str()
            .filter(|data| data.len() <= 1_000_000)
            .map(|data| json!({"type":block["type"],"mimeType":block["mimeType"],"data":data})),
        "resource" => {
            let resource = &block["resource"];
            if let Some(text) = resource["text"].as_str() {
                Some(
                    json!({"type":"resource","resource":{"uri":resource["uri"],"mimeType":resource["mimeType"],"text":text.chars().take(24_000).collect::<String>()}}),
                )
            } else {
                resource["blob"].as_str().filter(|blob| blob.len() <= 1_000_000).map(|blob| json!({"type":"resource","resource":{"uri":resource["uri"],"mimeType":resource["mimeType"],"blob":blob}}))
            }
        }
        "resource_link" => Some(
            json!({"type":"resource_link","uri":block["uri"],"name":block["name"],"title":block["title"],"mimeType":block["mimeType"],"size":block["size"]}),
        ),
        "diff" => {
            let old_text = block["oldText"]
                .as_str()
                .filter(|text| text.len() <= 250_000);
            let new_text = block["newText"]
                .as_str()
                .filter(|text| text.len() <= 250_000);
            if let Some(text) = block["text"].as_str().filter(|text| text.len() <= 500_000) {
                Some(json!({"type":"diff","path":block["path"],"text":text}))
            } else if old_text.is_some() || new_text.is_some() {
                Some(
                    json!({"type":"diff","path":block["path"],"oldText":old_text,"newText":new_text}),
                )
            } else {
                None
            }
        }
        _ => None,
    }
}

fn content_block_text(block: &Value) -> String {
    block["text"]
        .as_str()
        .or_else(|| block["resource"]["text"].as_str())
        .or_else(|| block["newText"].as_str())
        .unwrap_or("")
        .to_owned()
}

fn content_blocks_text(blocks: &[Value]) -> String {
    blocks
        .iter()
        .map(content_block_text)
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn mime_type(path: &Path) -> String {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "svg" => "image/svg+xml",
        "avif" => "image/avif",
        "wav" => "audio/wav",
        "mp3" => "audio/mpeg",
        "m4a" => "audio/mp4",
        "ogg" => "audio/ogg",
        "flac" => "audio/flac",
        "aac" => "audio/aac",
        "txt" => "text/plain",
        "md" => "text/markdown",
        "json" => "application/json",
        "toml" => "application/toml",
        "yaml" | "yml" => "text/yaml",
        "xml" => "application/xml",
        "csv" => "text/csv",
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "js" | "jsx" | "ts" | "tsx" => "text/javascript",
        "rs" => "text/x-rust",
        "py" => "text/x-python",
        "c" | "h" => "text/x-c",
        "cpp" | "hpp" => "text/x-c++",
        "pdf" => "application/pdf",
        "zip" => "application/zip",
        _ => "application/octet-stream",
    }
    .into()
}

impl Worker {
    fn safe_tool_content(&self, content: &Value) -> Vec<Value> {
        let blocks: Vec<&Value> = if let Some(items) = content.as_array() {
            items.iter().take(12).collect()
        } else if let Some(text) = content.as_str().filter(|text| text.len() <= 24_000) {
            return vec![json!({"type":"text","text":text})];
        } else {
            return Vec::new();
        };
        blocks.into_iter().filter_map(|block| {
        if block["type"] == "terminal" {
            let terminal_id = block["terminalId"].as_str()?;
            let text = self.terminals.get(terminal_id).and_then(|terminal| terminal.capture.lock().ok().map(|capture| capture.text.clone())).or_else(|| self.terminal_history.get(terminal_id).cloned()).unwrap_or_default();
            Some(json!({"type":"terminal_output","terminalId":terminal_id,"text":text.chars().take(24_000).collect::<String>()}))
        } else { safe_content_block(block) }
    }).collect()
    }
}

fn file_uri(path: &str) -> String {
    let normalized = path.replace('\\', "/");
    let mut encoded = String::with_capacity(normalized.len());
    for byte in normalized.bytes() {
        if byte.is_ascii_alphanumeric() || b"/-_.~:".contains(&byte) {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    if encoded.starts_with("//") {
        format!("file:{encoded}")
    } else {
        format!("file:///{encoded}")
    }
}

fn valid_form_content(schema: &Value, content: &Value) -> bool {
    let (Some(properties), Some(values)) = (schema["properties"].as_object(), content.as_object())
    else {
        return false;
    };
    if content.to_string().len() > 32_000 || values.len() > 8 {
        return false;
    }
    if schema["required"].as_array().is_some_and(|required| {
        required
            .iter()
            .any(|item| item.as_str().is_some_and(|key| !values.contains_key(key)))
    }) {
        return false;
    }
    values.iter().all(|(key, value)| {
        let Some(field) = properties.get(key) else {
            return false;
        };
        let choices = field["oneOf"]
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.get("const"))
                    .collect::<Vec<_>>()
            })
            .or_else(|| {
                field["enum"]
                    .as_array()
                    .map(|items| items.iter().collect::<Vec<_>>())
            });
        let check = |value: &Value, rule: &Value, choices: &Option<Vec<&Value>>| -> bool {
            let correct_type = match rule["type"].as_str().unwrap_or("string") {
                "string" => value.as_str().is_some_and(|text| {
                    text.chars().count() <= 4000
                        && rule["minLength"]
                            .as_u64()
                            .is_none_or(|min| text.chars().count() >= min as usize)
                        && rule["maxLength"]
                            .as_u64()
                            .is_none_or(|max| text.chars().count() <= max as usize)
                        && match rule["format"].as_str().unwrap_or("") {
                            "email" => {
                                text.contains('@') && !text.starts_with('@') && !text.ends_with('@')
                            }
                            "uri" => text.starts_with("https://") || text.starts_with("http://"),
                            _ => true,
                        }
                }),
                "boolean" => value.is_boolean(),
                "integer" => value.as_i64().is_some_and(|number| {
                    number as f64 >= rule["minimum"].as_f64().unwrap_or(f64::NEG_INFINITY)
                        && number as f64 <= rule["maximum"].as_f64().unwrap_or(f64::INFINITY)
                }),
                "number" => value.as_f64().is_some_and(|number| {
                    number >= rule["minimum"].as_f64().unwrap_or(f64::NEG_INFINITY)
                        && number <= rule["maximum"].as_f64().unwrap_or(f64::INFINITY)
                }),
                _ => false,
            };
            correct_type
                && choices
                    .as_ref()
                    .is_none_or(|items| items.iter().any(|candidate| **candidate == *value))
        };
        if field["type"] == "array" {
            value.as_array().is_some_and(|items| {
                items.len() <= field["maxItems"].as_u64().unwrap_or(16).min(16) as usize
                    && items.len() >= field["minItems"].as_u64().unwrap_or(0) as usize
                    && (!field["uniqueItems"].as_bool().unwrap_or(false)
                        || items
                            .iter()
                            .enumerate()
                            .all(|(index, item)| !items[..index].contains(item)))
                    && items.iter().all(|item| {
                        let item_rule = &field["items"];
                        let item_choices = item_rule["oneOf"]
                            .as_array()
                            .map(|options| {
                                options
                                    .iter()
                                    .filter_map(|option| option.get("const"))
                                    .collect::<Vec<_>>()
                            })
                            .or_else(|| {
                                item_rule["enum"]
                                    .as_array()
                                    .map(|values| values.iter().collect::<Vec<_>>())
                            });
                        check(item, item_rule, &item_choices)
                    })
            })
        } else {
            check(value, field, &choices)
        }
    })
}

fn safe_form_schema(schema: &Value) -> bool {
    schema["properties"].as_object().is_some_and(|properties| {
        properties.iter().all(|(key, field)| {
            let descriptor =
                format!("{} {}", key, field["title"].as_str().unwrap_or("")).to_ascii_lowercase();
            ![
                "password",
                "api_key",
                "apikey",
                "access_token",
                "refresh_token",
                "secret",
                "private_key",
                "recovery_code",
                "payment",
            ]
            .iter()
            .any(|sensitive| descriptor.contains(sensitive))
        })
    })
}

fn provider_command(provider: &str) -> Option<(PathBuf, Vec<String>)> {
    if provider == "opencode" {
        let npm = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .map(|path| path.join("npm").join("node_modules"));
        let managed = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .map(|path| path.join("Island/tools/opencode/opencode.exe"));
        let mut candidates = Vec::new();
        if let Some(path) = managed {
            candidates.push(path);
        }
        if let Some(npm) = npm {
            candidates.push(npm.join("@opencode/cli/bin/opencode.exe"));
            candidates.push(npm.join("opencode-ai/bin/opencode.exe"));
        }
        candidates
            .into_iter()
            .find(|path| path.is_file())
            .map(|path| (path, vec!["acp".into()]))
    } else {
        let node = codex_node()?;
        if let Some(root) = codex_install_root()
            && let Ok(folder) = std::fs::read_to_string(root.join("active.txt"))
        {
            let folder = folder.trim();
            if safe_install_folder(folder)
                && let Some(command) =
                    codex_command_in_dir(&node, &root.join("versions").join(folder))
            {
                return Some(command);
            }
        }
        let mut candidates = Vec::new();
        if let Some(app_data) = std::env::var_os("APPDATA") {
            candidates.push(
                PathBuf::from(app_data)
                    .join("npm/node_modules/@agentclientprotocol/codex-acp/dist/index.js"),
            );
        }
        candidates.push(
            node.parent()
                .unwrap_or(Path::new(""))
                .join("node_modules/@agentclientprotocol/codex-acp/dist/index.js"),
        );
        let script = candidates.into_iter().find(|path| path.is_file())?;
        Some((node, vec![script.to_string_lossy().into_owned()]))
    }
}

fn codex_node() -> Option<PathBuf> {
    std::env::var_os("PATH")
        .and_then(|paths| {
            std::env::split_paths(&paths)
                .map(|folder| folder.join("node.exe"))
                .find(|path| path.is_file())
        })
        .or_else(|| {
            let path = PathBuf::from(r"C:\Program Files\nodejs\node.exe");
            path.is_file().then_some(path)
        })
}

fn codex_install_root() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .map(|path| path.join("Island/tools/codex-acp"))
}

fn safe_install_folder(folder: &str) -> bool {
    !folder.is_empty()
        && folder.len() <= 128
        && folder
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        && folder != "."
        && folder != ".."
}

fn codex_command_in_dir(node: &Path, directory: &Path) -> Option<(PathBuf, Vec<String>)> {
    let script = directory.join("node_modules/@agentclientprotocol/codex-acp/dist/index.js");
    script
        .is_file()
        .then(|| (node.to_owned(), vec![script.to_string_lossy().into_owned()]))
}

fn codex_command_in(directory: &Path) -> Option<(PathBuf, Vec<String>)> {
    codex_command_in_dir(&codex_node()?, directory)
}

fn codex_version_from_script(script: &str) -> Option<String> {
    let package = Path::new(script).parent()?.parent()?.join("package.json");
    let value: Value = serde_json::from_slice(&std::fs::read(package).ok()?).ok()?;
    let version = value["version"].as_str()?.to_owned();
    valid_stable_version(&version).then_some(version)
}

fn install_codex_acp(latest: &str) -> Result<CodexCandidate, String> {
    let root =
        codex_install_root().ok_or("Windows local application data folder is unavailable")?;
    let versions = root.join("versions");
    std::fs::create_dir_all(&versions)
        .map_err(|error| format!("Could not prepare the Codex ACP update folder: {error}"))?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let folder = format!("stage-{}-{stamp}", std::process::id());
    let directory = versions.join(&folder);
    std::fs::create_dir(&directory)
        .map_err(|error| format!("Could not create a private Codex ACP staging folder: {error}"))?;
    let package = if valid_stable_version(latest) {
        format!("@agentclientprotocol/codex-acp@{latest}")
    } else {
        "@agentclientprotocol/codex-acp@latest".into()
    };
    let prefix = directory.to_string_lossy().into_owned();
    let mut command = Command::new("cmd.exe");
    command
        .args([
            "/d",
            "/c",
            "npm",
            "install",
            "--prefix",
            &prefix,
            "--no-audit",
            "--no-fund",
            &package,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags_no_window();
    let install = run_codex_installer(&mut command, Duration::from_secs(300));
    let result = install.and_then(|()| {
        let script = directory.join("node_modules/@agentclientprotocol/codex-acp/dist/index.js");
        let version_path =
            directory.join("node_modules/@agentclientprotocol/codex-acp/package.json");
        let metadata: Value =
            serde_json::from_slice(&std::fs::read(version_path).map_err(|error| {
                format!("npm completed, but Codex ACP metadata is missing: {error}")
            })?)
            .map_err(|error| format!("Codex ACP package metadata is invalid: {error}"))?;
        let version = metadata["version"].as_str().unwrap_or("");
        if !script.is_file() || !valid_stable_version(version) {
            return Err("npm did not install a stable Codex ACP executable.".into());
        }
        Ok(CodexCandidate {
            directory: directory.clone(),
            version: version.into(),
        })
    });
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&directory);
    }
    result
}

fn activate_codex_candidate(candidate: &CodexCandidate) -> Result<(), String> {
    let root =
        codex_install_root().ok_or("Windows local application data folder is unavailable")?;
    let versions = root
        .join("versions")
        .canonicalize()
        .map_err(|error| format!("Could not resolve staged versions folder: {error}"))?;
    let directory = candidate
        .directory
        .canonicalize()
        .map_err(|error| format!("Could not resolve staged adapter: {error}"))?;
    if !directory.starts_with(&versions) {
        return Err("Staged adapter is outside the managed versions folder.".into());
    }
    let folder = directory
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    if !safe_install_folder(folder) {
        return Err("Staged adapter folder name is invalid.".into());
    }
    std::fs::create_dir_all(&root)
        .map_err(|error| format!("Could not save active adapter version: {error}"))?;
    let active = root.join("active.txt");
    let temporary = root.join(format!("active-{}.tmp", std::process::id()));
    std::fs::write(&temporary, folder)
        .map_err(|error| format!("Could not write active adapter version: {error}"))?;
    replace_codex_active_pointer(&temporary, &active)?;
    prune_codex_versions(&versions, folder);
    Ok(())
}

#[cfg(windows)]
fn replace_codex_active_pointer(temporary: &Path, active: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };
    let source: Vec<u16> = temporary.as_os_str().encode_wide().chain([0]).collect();
    let destination: Vec<u16> = active.as_os_str().encode_wide().chain([0]).collect();
    if unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    } != 0
    {
        Ok(())
    } else {
        let _ = std::fs::remove_file(temporary);
        Err(
            "Could not activate staged adapter version; the previous version remains active."
                .into(),
        )
    }
}

#[cfg(not(windows))]
fn replace_codex_active_pointer(temporary: &Path, active: &Path) -> Result<(), String> {
    std::fs::rename(temporary, active).map_err(|error| format!("Could not activate staged adapter version; the previous version remains active: {error}"))
}

fn prune_codex_versions(versions: &Path, active: &str) {
    let Ok(entries) = std::fs::read_dir(versions) else {
        return;
    };
    let mut staged: Vec<(SystemTime, PathBuf)> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name == active || !name.starts_with("stage-") || !safe_install_folder(&name) {
                return None;
            }
            let metadata = entry.file_type().ok()?;
            if !metadata.is_dir() {
                return None;
            }
            let canonical = entry.path().canonicalize().ok()?;
            canonical.starts_with(versions).then(|| {
                (
                    entry
                        .metadata()
                        .and_then(|meta| meta.modified())
                        .unwrap_or(UNIX_EPOCH),
                    canonical,
                )
            })
        })
        .collect();
    staged.sort_by_key(|(modified, _)| std::cmp::Reverse(*modified));
    for (_, old) in staged.into_iter().skip(1) {
        let _ = std::fs::remove_dir_all(old);
    }
}

fn run_codex_installer(command: &mut Command, timeout: Duration) -> Result<(), String> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags_no_window()
        .spawn()
        .map_err(|error| format!("Could not start npm: {error}"))?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(status)) => {
                return Err(format!(
                    "Codex ACP install failed with {status}. The active version was kept."
                ));
            }
            Ok(None) if started.elapsed() < timeout => thread::sleep(Duration::from_millis(250)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(
                    "Codex ACP install timed out after five minutes. The active version was kept."
                        .into(),
                );
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "Codex ACP install process failed: {error}. The active version was kept."
                ));
            }
        }
    }
}

fn remove_codex_candidate(candidate: &CodexCandidate) {
    let Some(root) = codex_install_root() else {
        return;
    };
    let Ok(versions) = root.join("versions").canonicalize() else {
        return;
    };
    let Ok(directory) = candidate.directory.canonicalize() else {
        return;
    };
    if directory.starts_with(versions) {
        let _ = std::fs::remove_dir_all(directory);
    }
}

const ACP_PROTOCOL_VERSION: u64 = 1;

fn latest_codex_acp_version() -> Option<String> {
    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command",
            "$ErrorActionPreference='Stop'; try { $r = Invoke-RestMethod -Uri 'https://registry.npmjs.org/@agentclientprotocol%2fcodex-acp/latest' -TimeoutSec 12; if ($r.version -match '^\\d+\\.\\d+\\.\\d+$') { $r.version } } catch { '' }"])
        .stdin(Stdio::null()).creation_flags_no_window().output().ok()?;
    if !output.status.success() {
        return None;
    }
    let version = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    valid_stable_version(&version).then_some(version)
}

fn valid_stable_version(version: &str) -> bool {
    let parts: Vec<_> = version.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
}

fn newer_version(latest: &str, installed: &str) -> bool {
    fn parts(value: &str) -> Option<[u64; 3]> {
        if !valid_stable_version(value) {
            return None;
        }
        let mut values = value.split('.').map(str::parse::<u64>);
        Some([
            values.next()?.ok()?,
            values.next()?.ok()?,
            values.next()?.ok()?,
        ])
    }
    matches!((parts(latest), parts(installed)), (Some(new), Some(old)) if new > old)
}

trait NoWindow {
    fn creation_flags_no_window(&mut self) -> &mut Self;
}

impl NoWindow for Command {
    fn creation_flags_no_window(&mut self) -> &mut Self {
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            self.creation_flags(0x0800_0000);
        }
        self
    }
}

#[cfg(test)]
mod memory_tests {
    use super::*;

    #[test]
    fn published_response_shares_storage_and_updates_are_isolated() {
        let response = Arc::new(Response {
            session_id: "session".into(),
            text: "A substantial answer. ".repeat(50_000),
            waiting: true,
            ..Response::default()
        });
        let snapshot = Snapshot {
            response: Some(Arc::clone(&response)),
            ..Snapshot::default()
        };
        let published = snapshot.clone();
        assert!(Arc::ptr_eq(
            snapshot.response.as_ref().unwrap(),
            published.response.as_ref().unwrap()
        ));
        let mut cache = HashMap::from([("session", response)]);
        let updated = Arc::make_mut(cache.get_mut("session").unwrap());
        updated.text.push_str("Done.");
        updated.waiting = false;
        assert!(published.response.as_ref().unwrap().waiting);
        assert!(!published.response.as_ref().unwrap().text.ends_with("Done."));
        assert!(cache["session"].text.ends_with("Done."));
        assert_eq!(
            serde_json::to_value(&published).unwrap()["response"]["sessionId"],
            "session"
        );
    }
}
