//! User-level ACP MCP configuration and shared Agent Skills storage.
//! MCP configuration is DPAPI protected and never projected to the WebView
//! with secret values. Shared skills live in the documented ~/.agents/skills
//! directory, which both Codex and OpenCode discover.

use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use windows_sys::Win32::Storage::FileSystem::{
    MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
};

use super::secrets;

const MAX_MCP_SERVERS: usize = 16;
const MAX_MCP_STORE_BYTES: usize = 1_000_000;
const MAX_SKILL_BYTES: usize = 128_000;
static NEXT_TEMP_FILE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Pair {
    name: String,
    value: String,
}

impl Drop for Pair {
    fn drop(&mut self) {
        // Secret strings are copied only for serialization/session launch.
        // Keep clearing visible before their allocation is released.
        unsafe {
            for byte in self.value.as_mut_vec() {
                std::ptr::write_volatile(byte, 0);
            }
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct McpServer {
    name: String,
    transport: String,
    #[serde(default)]
    enabled: bool,
    #[serde(default)]
    command: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    env: Vec<Pair>,
    #[serde(default)]
    url: String,
    #[serde(default)]
    headers: Vec<Pair>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct McpStore {
    schema_version: u32,
    servers: Vec<McpServer>,
}

fn local_root() -> Result<PathBuf, String> {
    let local = std::env::var_os("LOCALAPPDATA").ok_or("Windows local app data is unavailable.")?;
    Ok(PathBuf::from(local).join("Island"))
}

fn mcp_store_path() -> Result<PathBuf, String> {
    Ok(local_root()?.join("agent-mcp.bin"))
}

fn read_mcp_store() -> Result<Vec<McpServer>, String> {
    let path = mcp_store_path()?;
    let encrypted = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("Could not read protected MCP settings: {error}")),
    };
    if encrypted.len() > MAX_MCP_STORE_BYTES + 64_000 {
        return Err("The protected MCP settings file is larger than expected.".into());
    }
    let plaintext = secrets::unprotect(&encrypted).map_err(|_| {
        "Windows could not unlock the protected MCP settings. No secrets were loaded.".to_owned()
    })?;
    let store: McpStore = serde_json::from_slice(plaintext.as_bytes())
        .map_err(|_| "The protected MCP settings file is invalid.".to_owned())?;
    if store.schema_version != 1 {
        return Err("The protected MCP settings use an unsupported schema version.".into());
    }
    validate_store(&store.servers)?;
    Ok(store.servers)
}

fn write_mcp_store(servers: &[McpServer]) -> Result<(), String> {
    validate_store(servers)?;
    let mut plaintext = serde_json::to_vec(&McpStore {
        schema_version: 1,
        servers: servers.to_vec(),
    })
    .map_err(|_| "Could not encode MCP settings.".to_owned())?;
    if plaintext.len() > MAX_MCP_STORE_BYTES {
        wipe_bytes(&mut plaintext);
        return Err("MCP settings exceed the 1 MB limit.".into());
    }
    let protected = secrets::protect(&plaintext);
    wipe_bytes(&mut plaintext);
    let encrypted = protected
        .map_err(|_| "Windows could not protect MCP credentials. Nothing was saved.".to_owned())?;
    let path = mcp_store_path()?;
    fs::create_dir_all(path.parent().ok_or("MCP settings path is invalid.")?)
        .map_err(|error| format!("Could not create the Island settings folder: {error}"))?;
    let suffix = NEXT_TEMP_FILE.fetch_add(1, Ordering::Relaxed);
    let temp = path.with_extension(format!("{}.{}.tmp", std::process::id(), suffix));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .map_err(|error| format!("Could not stage protected MCP settings: {error}"))?;
    if let Err(error) = file.write_all(&encrypted).and_then(|_| file.sync_all()) {
        let _ = fs::remove_file(&temp);
        return Err(format!("Could not write protected MCP settings: {error}"));
    }
    drop(file);
    let from: Vec<u16> = temp.as_os_str().encode_wide().chain(Some(0)).collect();
    let to: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let moved = unsafe {
        MoveFileExW(
            from.as_ptr(),
            to.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if moved == 0 {
        let _ = fs::remove_file(&temp);
        return Err("Could not atomically replace the protected MCP settings file.".into());
    }
    Ok(())
}

fn validate_store(servers: &[McpServer]) -> Result<(), String> {
    if servers.len() > MAX_MCP_SERVERS {
        return Err(format!("Keep at most {MAX_MCP_SERVERS} MCP servers."));
    }
    let mut names = std::collections::HashSet::new();
    for server in servers {
        if !valid_slug(&server.name) || !names.insert(server.name.to_ascii_lowercase()) {
            return Err("MCP server names must be unique lowercase names using letters, numbers, and single hyphens.".into());
        }
        if !matches!(server.transport.as_str(), "stdio" | "http") {
            return Err("Choose stdio or HTTP for an MCP server.".into());
        }
        if server.transport == "stdio" {
            if server.command.trim().is_empty()
                || server.command.len() > 2048
                || has_control(&server.command)
            {
                return Err(format!("Enter a valid executable for {}.", server.name));
            }
            if server.args.len() > 64
                || server
                    .args
                    .iter()
                    .any(|value| value.len() > 8192 || has_control(value))
            {
                return Err(format!(
                    "Arguments for {} exceed the supported limits.",
                    server.name
                ));
            }
            validate_pairs(&server.env, "environment variables")?;
            if server.env.iter().any(|pair| {
                !pair
                    .name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
            }) {
                return Err(
                    "Environment variable names may use only letters, numbers, and underscores."
                        .into(),
                );
            }
        } else {
            if server.url.len() > 4096 || has_control(&server.url) || !valid_http_url(&server.url) {
                return Err(format!(
                    "Enter an absolute HTTP or HTTPS endpoint for {}.",
                    server.name
                ));
            }
            validate_pairs(&server.headers, "HTTP headers")?;
            if server
                .headers
                .iter()
                .any(|pair| !pair.name.bytes().all(is_header_name_byte))
            {
                return Err("HTTP header names must use standard token characters.".into());
            }
        }
    }
    Ok(())
}

fn validate_pairs(pairs: &[Pair], label: &str) -> Result<(), String> {
    if pairs.len() > 32 {
        return Err(format!("A server can have at most 32 {label}."));
    }
    let mut keys = std::collections::HashSet::new();
    for pair in pairs {
        if pair.name.is_empty()
            || pair.name.len() > 128
            || has_control(&pair.name)
            || pair.value.len() > 8192
            || has_control(&pair.value)
            || !keys.insert(pair.name.to_ascii_lowercase())
        {
            return Err(format!(
                "Check the {label}; names must be unique and values must fit the size limit."
            ));
        }
    }
    Ok(())
}

fn has_control(value: &str) -> bool {
    value.chars().any(char::is_control)
}

fn valid_http_url(value: &str) -> bool {
    let rest = value
        .strip_prefix("https://")
        .or_else(|| value.strip_prefix("http://"));
    rest.is_some_and(|rest| {
        let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
        !authority.is_empty() && !authority.starts_with(':') && !authority.ends_with(':')
    })
}

fn is_header_name_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(
            byte,
            b'!' | b'#'
                | b'$'
                | b'%'
                | b'&'
                | b'\''
                | b'*'
                | b'+'
                | b'-'
                | b'.'
                | b'^'
                | b'_'
                | b'`'
                | b'|'
                | b'~'
        )
}

fn valid_slug(value: &str) -> bool {
    if value.is_empty()
        || value.len() > 64
        || value.starts_with('-')
        || value.ends_with('-')
        || value.contains("--")
    {
        return false;
    }
    value
        .bytes()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn parse_pairs(value: &Value, key: &str, previous: &[Pair]) -> Result<Vec<Pair>, String> {
    let Some(values) = value[key].as_array() else {
        return Ok(Vec::new());
    };
    if values.len() > 32 {
        return Err("A server can have at most 32 values in each key list.".into());
    }
    let mut result = Vec::new();
    for item in values {
        let name = item["name"].as_str().unwrap_or("").trim();
        let value = item["value"].as_str().unwrap_or("");
        if name.is_empty() && value.is_empty() {
            continue;
        }
        let resolved = if value.is_empty() {
            previous
                .iter()
                .find(|pair| pair.name.eq_ignore_ascii_case(name))
                .map(|pair| pair.value.clone())
                .unwrap_or_default()
        } else {
            value.to_owned()
        };
        result.push(Pair {
            name: name.to_owned(),
            value: resolved,
        });
    }
    Ok(result)
}

pub fn list_mcp_servers() -> Result<Value, String> {
    let servers = read_mcp_store()?;
    Ok(Value::Array(servers.iter().map(|server| json!({
        "name": server.name,
        "transport": server.transport,
        "enabled": server.enabled,
        "command": server.command,
        "args": server.args,
        "url": server.url,
        "environment": server.env.iter().map(|pair| json!({"name": pair.name, "stored": !pair.value.is_empty()})).collect::<Vec<_>>(),
        "headers": server.headers.iter().map(|pair| json!({"name": pair.name, "stored": !pair.value.is_empty()})).collect::<Vec<_>>(),
    })).collect()))
}

pub fn save_mcp_server(value: &Value) -> Result<(), String> {
    let name = value["name"]
        .as_str()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    if !valid_slug(&name) {
        return Err(
            "Use a lowercase server name with letters, numbers, and single hyphens.".into(),
        );
    }
    let transport = value["transport"].as_str().unwrap_or("stdio").to_owned();
    let mut servers = read_mcp_store()?;
    let original_name = value["originalName"].as_str().unwrap_or("");
    if let Some(existing) = servers.iter().find(|server| server.name == name)
        && original_name.is_empty()
        && existing.name == name
    {
        return Err("That MCP server already exists. Edit its row to change it.".into());
    }
    if !original_name.is_empty() && original_name != name {
        return Err("MCP server names cannot be changed after creation.".into());
    }
    let previous = servers.iter().find(|server| server.name == name).cloned();
    let env = parse_pairs(
        value,
        "env",
        previous
            .as_ref()
            .map_or(&[], |server| server.env.as_slice()),
    )?;
    let headers = parse_pairs(
        value,
        "headers",
        previous
            .as_ref()
            .map_or(&[], |server| server.headers.as_slice()),
    )?;
    let args = match value["args"].as_array() {
        Some(items) if items.len() <= 64 => items
            .iter()
            .map(|item| {
                item.as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| "Each MCP argument must be text.".to_owned())
            })
            .collect::<Result<Vec<_>, _>>()?,
        Some(_) => return Err("An MCP server can have at most 64 arguments.".into()),
        None => Vec::new(),
    };
    let server = McpServer {
        name: name.clone(),
        transport,
        enabled: value["enabled"].as_bool().unwrap_or(true),
        command: value["command"].as_str().unwrap_or("").trim().to_owned(),
        args,
        env,
        headers,
        url: value["url"].as_str().unwrap_or("").trim().to_owned(),
    };
    if let Some(index) = servers.iter().position(|item| item.name == name) {
        servers[index] = server;
    } else {
        servers.push(server);
    }
    write_mcp_store(&servers)
}

pub fn clear_mcp_input(value: &mut Value) {
    let Some(server) = value.get_mut("server") else {
        return;
    };
    for collection in ["env", "headers"] {
        if let Some(entries) = server.get_mut(collection).and_then(Value::as_array_mut) {
            for entry in entries {
                if let Some(Value::String(value)) = entry.get_mut("value") {
                    wipe_string(value);
                }
            }
        }
    }
}

pub fn delete_mcp_server(name: &str) -> Result<(), String> {
    let mut servers = read_mcp_store()?;
    let original = servers.len();
    servers.retain(|server| server.name != name);
    if original == servers.len() {
        return Err("That MCP server no longer exists.".into());
    }
    write_mcp_store(&servers)
}

/// Returns ACP v1 MCP payloads for this provider. stdio is part of ACP v1;
/// HTTP is sent only after the agent advertises `mcpCapabilities.http`.
pub fn acp_mcp_servers(http_supported: bool) -> Result<Vec<Value>, String> {
    let servers = read_mcp_store()?;
    let mut result = Vec::new();
    for server in servers.into_iter().filter(|server| server.enabled) {
        match server.transport.as_str() {
            "stdio" => {
                let command = resolve_command(&server.command).ok_or_else(|| format!("MCP command for '{}' must resolve to a Windows executable (.exe or .com). Use node.exe or python.exe with the script path as an argument; shell shims are not launched.", server.name))?;
                result.push(json!({"name":server.name,"command":command,"args":server.args,"env":server.env}));
            }
            "http" if http_supported => result.push(
                json!({"type":"http","name":server.name,"url":server.url,"headers":server.headers}),
            ),
            "http" => {}
            _ => {}
        }
    }
    Ok(result)
}

fn resolve_command(command: &str) -> Option<String> {
    let candidate = Path::new(command);
    if candidate.is_absolute() {
        return candidate
            .is_file()
            .then(|| candidate.to_string_lossy().into_owned())
            .filter(|path| is_windows_executable(Path::new(path)));
    }
    let path = std::env::var_os("PATH")?;
    for directory in std::env::split_paths(&path) {
        let base = directory.join(command);
        let mut candidates = vec![base.clone()];
        if base.extension().is_none() {
            for extension in ["exe", "com"] {
                candidates.push(base.with_extension(extension));
            }
        }
        if let Some(found) = candidates
            .into_iter()
            .find(|path| path.is_file() && is_windows_executable(path))
        {
            return Some(found.to_string_lossy().into_owned());
        }
    }
    None
}

fn is_windows_executable(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| matches!(extension.to_ascii_lowercase().as_str(), "exe" | "com"))
}

fn skills_root() -> Result<PathBuf, String> {
    let profile = std::env::var_os("USERPROFILE")
        .ok_or("The Windows user profile directory is unavailable.")?;
    Ok(PathBuf::from(profile).join(".agents").join("skills"))
}

pub fn list_skills() -> Result<Value, String> {
    let root = skills_root()?;
    if !root.exists() {
        return Ok(json!([]));
    }
    let entries =
        fs::read_dir(&root).map_err(|error| format!("Could not read shared skills: {error}"))?;
    let mut skills = Vec::new();
    for entry in entries.flatten().take(128) {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if !kind.is_dir() {
            continue;
        }
        let path = entry.path();
        let skill_file = path.join("SKILL.md");
        let Ok(skill_kind) = fs::symlink_metadata(&skill_file) else {
            continue;
        };
        if !skill_kind.is_file() || skill_kind.file_type().is_symlink() {
            continue;
        }
        let Ok(file) = fs::File::open(&skill_file) else {
            continue;
        };
        let mut bytes = Vec::new();
        if file.take(16_384).read_to_end(&mut bytes).is_err() {
            continue;
        }
        let text = String::from_utf8_lossy(&bytes);
        if text.len() > MAX_SKILL_BYTES {
            continue;
        }
        let (name, description) = skill_metadata(&text);
        if !valid_slug(&name) {
            continue;
        }
        skills.push(json!({"name":name,"description":description,"managed":is_regular_file(&path.join(".island-managed"))}));
    }
    skills.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    Ok(Value::Array(skills))
}

pub fn list_project_entries(root: &str, relative: &str, query: &str) -> Result<Value, String> {
    if root.len() > 32_768 || relative.len() > 2_048 {
        return Err("Project path is too long.".into());
    }
    let root = PathBuf::from(root)
        .canonicalize()
        .map_err(|error| format!("Could not open the project folder: {error}"))?;
    if !root.is_dir() {
        return Err("Choose an existing project folder first.".into());
    }
    let mut directory = root.clone();
    for component in Path::new(relative).components() {
        match component {
            std::path::Component::Normal(name) => directory.push(name),
            std::path::Component::CurDir => {}
            _ => return Err("Project path must stay inside the selected folder.".into()),
        }
    }
    let directory = directory
        .canonicalize()
        .map_err(|error| format!("Could not open that project folder: {error}"))?;
    if !directory.starts_with(&root) || !directory.is_dir() {
        return Err("Project path must stay inside the selected folder.".into());
    }

    let mut entries = Vec::new();
    let mut has_more = false;
    let query = query.to_lowercase();
    for entry in fs::read_dir(&directory)
        .map_err(|error| format!("Could not list project contents: {error}"))?
    {
        let Ok(entry) = entry else {
            continue;
        };
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_symlink() || (!kind.is_dir() && !kind.is_file()) {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == ".git" {
            continue;
        }
        if !query.is_empty() && !name.to_lowercase().contains(&query) {
            continue;
        }
        if entries.len() == 100 {
            has_more = true;
            break;
        }
        let path = entry.path();
        let relative_path = path
            .strip_prefix(&root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        entries.push(json!({
            "name": name,
            "path": path.to_string_lossy(),
            "relative": relative_path,
            "kind": if kind.is_dir() { "directory" } else { "file" },
        }));
    }
    entries.sort_by(|left, right| {
        let left_dir = left["kind"] == "directory";
        let right_dir = right["kind"] == "directory";
        right_dir
            .cmp(&left_dir)
            .then_with(|| left["name"].as_str().cmp(&right["name"].as_str()))
    });
    Ok(json!({ "entries": entries, "relative": relative, "hasMore": has_more }))
}

pub fn read_skill(name: &str) -> Result<String, String> {
    if !valid_slug(name) {
        return Err("Invalid skill name.".into());
    }
    let folder = skills_root()?.join(name);
    let folder_metadata =
        fs::symlink_metadata(&folder).map_err(|error| format!("Could not read skill: {error}"))?;
    if !folder_metadata.is_dir() || folder_metadata.file_type().is_symlink() {
        return Err("Skill folder is not a regular directory.".into());
    }
    let file = folder.join("SKILL.md");
    let metadata =
        fs::symlink_metadata(&file).map_err(|error| format!("Could not read skill: {error}"))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err("Skill file is not a regular file.".into());
    }
    let text =
        fs::read_to_string(file).map_err(|error| format!("Could not read skill: {error}"))?;
    if text.len() > MAX_SKILL_BYTES {
        return Err("Skill instructions exceed the 128 KB editor limit.".into());
    }
    Ok(text)
}

pub fn save_skill(name: &str, description: &str, instructions: &str) -> Result<(), String> {
    let name = name.trim().to_ascii_lowercase();
    let description = description.trim();
    if !valid_slug(&name) {
        return Err("Use a lowercase skill name with letters, numbers, and single hyphens.".into());
    }
    if description.is_empty() || description.len() > 1024 || has_control(description) {
        return Err("Add a description up to 1,024 characters.".into());
    }
    if instructions.trim().is_empty() || instructions.len() > MAX_SKILL_BYTES - 2048 {
        return Err("Add skill instructions up to 126 KB.".into());
    }
    let root = skills_root()?;
    fs::create_dir_all(&root)
        .map_err(|error| format!("Could not create shared skills folder: {error}"))?;
    let folder = root.join(&name);
    if folder.exists() {
        let metadata = fs::symlink_metadata(&folder)
            .map_err(|error| format!("Could not inspect the skill folder: {error}"))?;
        if !metadata.is_dir()
            || metadata.file_type().is_symlink()
            || !is_regular_file(&folder.join(".island-managed"))
        {
            return Err(
                "A skill with that name already exists outside Island. Choose a different name."
                    .into(),
            );
        }
    }
    fs::create_dir_all(&folder)
        .map_err(|error| format!("Could not create skill folder: {error}"))?;
    let yaml_quote = |value: &str| {
        format!(
            "\"{}\"",
            value
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('\n', " ")
        )
    };
    let content = format!(
        "---\nname: {name}\ndescription: {}\n---\n\n{}\n",
        yaml_quote(description),
        instructions.trim()
    );
    let file = folder.join("SKILL.md");
    if file.exists() {
        let metadata = fs::symlink_metadata(&file)
            .map_err(|error| format!("Could not inspect skill instructions: {error}"))?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err("Skill instructions must be a regular file.".into());
        }
    }
    let temp = folder.join("SKILL.md.island-tmp");
    fs::write(&temp, content)
        .map_err(|error| format!("Could not save skill instructions: {error}"))?;
    let marker = folder.join(".island-managed");
    if !marker.exists() {
        fs::write(&marker, b"Island shared skill\n")
            .map_err(|error| format!("Could not write the skill management marker: {error}"))?;
    }
    let from: Vec<u16> = temp.as_os_str().encode_wide().chain(Some(0)).collect();
    let to: Vec<u16> = file.as_os_str().encode_wide().chain(Some(0)).collect();
    let moved = unsafe {
        MoveFileExW(
            from.as_ptr(),
            to.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if moved == 0 {
        let _ = fs::remove_file(&temp);
        return Err("Could not atomically publish the skill instructions.".into());
    }
    Ok(())
}

pub fn delete_skill(name: &str) -> Result<(), String> {
    if !valid_slug(name) {
        return Err("Invalid skill name.".into());
    }
    let folder = skills_root()?.join(name);
    let metadata =
        fs::symlink_metadata(&folder).map_err(|_| "Skill folder not found.".to_owned())?;
    if !metadata.is_dir()
        || metadata.file_type().is_symlink()
        || !is_regular_file(&folder.join(".island-managed"))
    {
        return Err("Only skills created by Island can be removed here.".into());
    }
    for file in ["SKILL.md", ".island-managed"] {
        let path = folder.join(file);
        if path.exists() {
            fs::remove_file(path)
                .map_err(|error| format!("Could not remove skill file: {error}"))?;
        }
    }
    let _ = fs::remove_dir(folder);
    Ok(())
}

fn skill_metadata(text: &str) -> (String, String) {
    let mut lines = text.lines();
    if lines.next() != Some("---") {
        return (String::new(), String::new());
    }
    let mut name = String::new();
    let mut description = String::new();
    for line in lines.take(80) {
        if line.trim() == "---" {
            break;
        }
        if let Some(value) = line.strip_prefix("name:") {
            name = value.trim().trim_matches(['"', '\'']).to_owned();
        }
        if let Some(value) = line.strip_prefix("description:") {
            description = value.trim().trim_matches(['"', '\'']).to_owned();
        }
    }
    (name, description)
}

fn is_regular_file(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .is_ok_and(|metadata| metadata.is_file() && !metadata.file_type().is_symlink())
}

fn wipe_bytes(bytes: &mut [u8]) {
    for byte in bytes {
        unsafe { std::ptr::write_volatile(byte, 0) };
    }
}

fn wipe_string(value: &mut String) {
    unsafe {
        for byte in value.as_mut_vec() {
            std::ptr::write_volatile(byte, 0);
        }
    }
}
