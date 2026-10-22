//! On-demand speech transcription. Audio is held for one request and sent only
//! to the endpoint the user configured; credentials are protected with DPAPI.

use serde::{Deserialize, Serialize};
use windows_startup::Win32::Networking::WinHttp::{
    WINHTTP_ACCESS_TYPE_DEFAULT_PROXY, WINHTTP_ADDREQ_FLAG_ADD, WINHTTP_FLAG_SECURE,
    WINHTTP_OPEN_REQUEST_FLAGS, WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_STATUS_CODE,
    WinHttpAddRequestHeaders, WinHttpCloseHandle, WinHttpConnect, WinHttpOpen, WinHttpOpenRequest,
    WinHttpQueryDataAvailable, WinHttpQueryHeaders, WinHttpReadData, WinHttpReceiveResponse,
    WinHttpSendRequest, WinHttpSetTimeouts,
};
use windows_startup::core::PCWSTR;

const MAX_AUDIO_BYTES: usize = 5 * 1024 * 1024;
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;
const MAX_API_KEY_BYTES: usize = 4096;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct SpeechSettings {
    pub base_url: String,
    pub model: String,
    pub language: String,
    encrypted_key: Option<String>,
}

impl Default for SpeechSettings {
    fn default() -> Self {
        Self {
            base_url: "https://api.openai.com/v1".into(),
            model: "gpt-4o-mini-transcribe".into(),
            language: String::new(),
            encrypted_key: None,
        }
    }
}

impl SpeechSettings {
    pub fn public_state(&self) -> serde_json::Value {
        let configured = self.encrypted_key.is_some()
            || parse_base_url(&self.base_url).is_ok_and(|endpoint| !endpoint.secure);
        serde_json::json!({
            "baseUrl": self.base_url,
            "model": self.model,
            "language": self.language,
            "configured": configured,
            "hasKey": self.encrypted_key.is_some(),
        })
    }

    pub fn save(
        &self,
        base_url: &str,
        model: &str,
        language: &str,
        api_key: &str,
    ) -> Result<Self, String> {
        let base_url = base_url.trim().trim_end_matches('/');
        let model = model.trim();
        let language = language.trim();
        parse_base_url(base_url)?;
        if model.is_empty() || model.len() > 200 || !model.bytes().all(valid_model_byte) {
            return Err("Choose a valid transcription model name.".into());
        }
        if language.len() > 32
            || !language
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        {
            return Err("Use a language code such as en or en-US, or leave it blank.".into());
        }

        let mut next = self.clone();
        let endpoint_changed = !self.base_url.eq_ignore_ascii_case(base_url);
        next.base_url = base_url.to_owned();
        next.model = model.to_owned();
        next.language = language.to_owned();
        // Never silently forward a saved provider key to a newly selected
        // endpoint. Require the user to enter the key again for that host.
        if endpoint_changed {
            next.encrypted_key = None;
        }
        if !api_key.trim().is_empty() {
            let key = api_key.trim();
            if key.len() > MAX_API_KEY_BYTES || !key.bytes().all(|byte| byte.is_ascii_graphic()) {
                return Err("The API key is invalid or too long.".into());
            }
            use base64::Engine as _;
            let encrypted =
                crate::platform::windows::secrets::protect(key.as_bytes()).map_err(|_| {
                    "Windows could not securely store the transcription key.".to_owned()
                })?;
            next.encrypted_key = Some(base64::engine::general_purpose::STANDARD.encode(encrypted));
        }
        next.persist()?;
        Ok(next)
    }

    pub fn remove_key(&self) -> Result<Self, String> {
        let mut next = self.clone();
        next.encrypted_key = None;
        next.persist()?;
        Ok(next)
    }

    fn persist(&self) -> Result<(), String> {
        let local = std::env::var_os("LOCALAPPDATA")
            .ok_or_else(|| "Island cannot find the local settings folder.".to_owned())?;
        let path = std::path::PathBuf::from(local)
            .join("Island")
            .join("speech.json");
        let parent = path
            .parent()
            .ok_or_else(|| "Island could not save speech settings.".to_owned())?;
        std::fs::create_dir_all(parent)
            .map_err(|_| "Island could not save speech settings.".to_owned())?;
        let bytes = serde_json::to_vec(self)
            .map_err(|_| "Island could not encode speech settings.".to_owned())?;
        let temp = path.with_extension("json.tmp");
        std::fs::write(&temp, bytes)
            .map_err(|_| "Island could not save speech settings.".to_owned())?;
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{
            MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
        };
        let source: Vec<u16> = temp.as_os_str().encode_wide().chain([0]).collect();
        let target: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
        (unsafe {
            MoveFileExW(
                source.as_ptr(),
                target.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        } != 0)
            .then_some(())
            .ok_or_else(|| {
                let _ = std::fs::remove_file(&temp);
                "Island could not finish saving speech settings.".to_owned()
            })
    }

    pub fn load() -> Self {
        let loaded = std::env::var_os("LOCALAPPDATA")
            .map(std::path::PathBuf::from)
            .map(|local| local.join("Island").join("speech.json"))
            .and_then(|path| std::fs::read(path).ok())
            .filter(|bytes| bytes.len() <= 16 * 1024)
            .and_then(|bytes| serde_json::from_slice::<Self>(&bytes).ok());
        loaded.filter(valid_loaded_settings).unwrap_or_default()
    }

    pub fn transcribe(&self, audio: Vec<u8>, content_type: &str) -> Result<String, String> {
        if audio.is_empty() || audio.len() > MAX_AUDIO_BYTES {
            return Err(
                "The recording is empty or exceeds five megabytes. Record a shorter clip.".into(),
            );
        }
        if !matches!(content_type, "audio/webm" | "audio/mp4") {
            return Err("This recording format is not supported. Try WebM or MP4 audio.".into());
        }
        let endpoint = parse_base_url(&self.base_url)?;
        let secret = if let Some(encrypted) = self.encrypted_key.as_deref() {
            use base64::Engine as _;
            let ciphertext = base64::engine::general_purpose::STANDARD
                .decode(encrypted)
                .map_err(|_| {
                    "The saved transcription key is invalid. Re-enter it in speech settings."
                        .to_owned()
                })?;
            Some(crate::platform::windows::secrets::unprotect(&ciphertext)
                .map_err(|_| "Windows could not unlock the transcription key. Re-enter it in speech settings.".to_owned())?)
        } else {
            None
        };
        let key = secret.as_ref().map(|secret| secret.as_bytes());
        if let Some(key) = key
            && (key.is_empty()
                || key.len() > MAX_API_KEY_BYTES
                || !key.iter().all(u8::is_ascii_graphic))
        {
            return Err(
                "The saved transcription key is invalid. Re-enter it in speech settings.".into(),
            );
        }
        if key.is_none() && endpoint.secure {
            return Err(
                "Set up a speech transcription key before using this remote provider.".into(),
            );
        }
        let audio = SensitiveBytes(audio);
        let boundary = format!(
            "island-{:x}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        );
        let mut body = SensitiveBytes::default();
        push_field(&mut body.0, &boundary, "model", &self.model);
        push_field(&mut body.0, &boundary, "response_format", "json");
        if !self.language.is_empty() {
            // OpenAI's current gpt-transcribe model uses the plural array;
            // compatible older models and OpenAI-style local servers use the
            // conventional singular field.
            let field = if self.model == "gpt-transcribe" {
                "languages[]"
            } else {
                "language"
            };
            push_field(&mut body.0, &boundary, field, &self.language);
        }
        body.0.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"dictation.{}\"\r\nContent-Type: {content_type}\r\n\r\n", if content_type == "audio/webm" { "webm" } else { "mp4" }).as_bytes());
        body.0.extend_from_slice(&audio.0);
        body.0
            .extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
        if body.0.len() > MAX_AUDIO_BYTES + 64 * 1024 {
            return Err("The recording is too large to send. Record a shorter clip.".into());
        }
        let mut headers = SensitiveWide::from_str(&format!(
            "Content-Type: multipart/form-data; boundary={boundary}\r\n"
        ));
        if let Some(key) = key {
            let key_text = std::str::from_utf8(key)
                .map_err(|_| "The saved transcription key is invalid.".to_owned())?;
            headers.0.extend("Authorization: Bearer ".encode_utf16());
            headers.0.extend(key_text.encode_utf16());
            headers.0.extend("\r\n".encode_utf16());
        }
        let request_path = format!(
            "{}/audio/transcriptions",
            endpoint.path.trim_end_matches('/')
        );
        let response = send_multipart(&endpoint, &request_path, &headers.0, &body.0)?;
        let (status, response) = response;
        if !(200..300).contains(&status) {
            return Err(match status {
                401 | 403 => "The transcription provider rejected the API key.".into(),
                429 => "The transcription provider is rate limited. Try again shortly.".into(),
                400 => {
                    "The provider rejected this model or recording format. Check speech settings."
                        .into()
                }
                _ => format!("The transcription provider returned HTTP {status}."),
            });
        }
        let json: serde_json::Value = serde_json::from_slice(&response).map_err(|_| {
            "The transcription provider returned an unreadable response.".to_owned()
        })?;
        let text = json["text"].as_str().unwrap_or("").trim();
        if text.is_empty() {
            return Err("No speech was recognized in that recording.".into());
        }
        Ok(text.chars().take(32_000).collect())
    }
}

fn valid_loaded_settings(settings: &SpeechSettings) -> bool {
    parse_base_url(&settings.base_url).is_ok()
        && !settings.model.is_empty()
        && settings.model.len() <= 200
        && settings.model.bytes().all(valid_model_byte)
        && settings.language.len() <= 32
        && settings
            .language
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

fn valid_model_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.')
}

struct Endpoint {
    host: String,
    port: u16,
    path: String,
    secure: bool,
}

fn parse_base_url(value: &str) -> Result<Endpoint, String> {
    if value.len() > 512
        || value
            .bytes()
            .any(|byte| byte.is_ascii_whitespace() || byte.is_ascii_control())
    {
        return Err("Enter a valid HTTPS transcription endpoint.".into());
    }
    let (scheme, rest) = value
        .split_once("://")
        .ok_or_else(|| "Enter an HTTPS transcription endpoint.".to_owned())?;
    let secure = match scheme {
        "https" => true,
        "http" => false,
        _ => return Err("Use HTTPS for remote speech providers.".into()),
    };
    let split = rest.find('/').unwrap_or(rest.len());
    let authority = &rest[..split];
    let path = &rest[split..];
    if authority.is_empty()
        || authority
            .chars()
            .any(|c| matches!(c, '@' | '?' | '#' | '\\'))
        || path.chars().any(|c| matches!(c, '?' | '#' | '\\'))
    {
        return Err("Enter a valid HTTPS transcription endpoint.".into());
    }
    let (host, port) = authority
        .rsplit_once(':')
        .and_then(|(host, port)| port.parse::<u16>().ok().map(|port| (host, port)))
        .unwrap_or((authority, if secure { 443 } else { 80 }));
    let host = host.to_ascii_lowercase();
    let local_http = matches!(host.as_str(), "localhost" | "127.0.0.1" | "[::1]");
    if (!secure && !local_http)
        || host.is_empty()
        || host.len() > 253
        || (!local_http || !host.contains(':'))
            && !host.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'[' | b']')
            })
        || port == 0
    {
        return Err(
            "Remote speech providers must use HTTPS; plain HTTP is allowed only for localhost."
                .into(),
        );
    }
    if !path.is_empty()
        && !path.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'-' | b'_' | b'.' | b'~' | b'%')
        })
    {
        return Err("The endpoint path contains unsupported characters.".into());
    }
    Ok(Endpoint {
        host,
        port,
        path: path.to_owned(),
        secure,
    })
}

fn push_field(body: &mut Vec<u8>, boundary: &str, name: &str, value: &str) {
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n"
        )
        .as_bytes(),
    );
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[derive(Default)]
struct SensitiveBytes(Vec<u8>);
impl Drop for SensitiveBytes {
    fn drop(&mut self) {
        self.0.fill(0);
    }
}

struct SensitiveWide(Vec<u16>);
impl SensitiveWide {
    fn from_str(value: &str) -> Self {
        Self(value.encode_utf16().collect())
    }
}
impl Drop for SensitiveWide {
    fn drop(&mut self) {
        self.0.fill(0);
    }
}

struct HttpHandle(*mut core::ffi::c_void);
impl HttpHandle {
    fn new(handle: *mut core::ffi::c_void) -> Result<Self, String> {
        if handle.is_null() {
            Err("Could not connect to the transcription provider.".into())
        } else {
            Ok(Self(handle))
        }
    }
}
impl Drop for HttpHandle {
    fn drop(&mut self) {
        let _ = unsafe { WinHttpCloseHandle(self.0) };
    }
}

fn send_multipart(
    endpoint: &Endpoint,
    path: &str,
    headers: &[u16],
    body: &[u8],
) -> Result<(u32, Vec<u8>), String> {
    let agent = wide("Island Speech/1.0");
    let host = wide(&endpoint.host);
    let request_path = wide(path);
    let post = wide("POST");
    let http11 = wide("HTTP/1.1");
    let session = HttpHandle::new(unsafe {
        WinHttpOpen(
            PCWSTR(agent.as_ptr()),
            WINHTTP_ACCESS_TYPE_DEFAULT_PROXY,
            PCWSTR::null(),
            PCWSTR::null(),
            0,
        )
    })?;
    unsafe { WinHttpSetTimeouts(session.0, 10_000, 10_000, 15_000, 60_000) }
        .map_err(|_| "Could not configure the transcription connection.".to_owned())?;
    let connect = HttpHandle::new(unsafe {
        WinHttpConnect(session.0, PCWSTR(host.as_ptr()), endpoint.port, 0)
    })?;
    let flags = if endpoint.secure {
        WINHTTP_FLAG_SECURE
    } else {
        WINHTTP_OPEN_REQUEST_FLAGS(0)
    };
    let request = HttpHandle::new(unsafe {
        WinHttpOpenRequest(
            connect.0,
            PCWSTR(post.as_ptr()),
            PCWSTR(request_path.as_ptr()),
            PCWSTR(http11.as_ptr()),
            PCWSTR::null(),
            std::ptr::null(),
            flags,
        )
    })?;
    unsafe { WinHttpAddRequestHeaders(request.0, headers, WINHTTP_ADDREQ_FLAG_ADD) }
        .map_err(|_| "Could not prepare the transcription request.".to_owned())?;
    unsafe {
        WinHttpSendRequest(
            request.0,
            None,
            Some(body.as_ptr().cast()),
            body.len() as u32,
            body.len() as u32,
            0,
        )
    }
    .map_err(|_| "Could not send audio to the transcription provider.".to_owned())?;
    unsafe { WinHttpReceiveResponse(request.0, std::ptr::null_mut()) }
        .map_err(|_| "The transcription provider did not respond.".to_owned())?;
    let mut status = 0u32;
    let mut status_size = std::mem::size_of::<u32>() as u32;
    unsafe {
        WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            PCWSTR::null(),
            Some((&mut status as *mut u32).cast()),
            &mut status_size,
            std::ptr::null_mut(),
        )
    }
    .map_err(|_| "The transcription provider returned no status.".to_owned())?;
    let mut response = Vec::new();
    loop {
        let mut available = 0u32;
        unsafe { WinHttpQueryDataAvailable(request.0, &mut available) }
            .map_err(|_| "Could not read the transcription response.".to_owned())?;
        if available == 0 {
            break;
        }
        if response.len().saturating_add(available as usize) > MAX_RESPONSE_BYTES {
            return Err("The transcription provider response was too large.".into());
        }
        let mut chunk = vec![0u8; available as usize];
        let mut read = 0u32;
        unsafe { WinHttpReadData(request.0, chunk.as_mut_ptr().cast(), available, &mut read) }
            .map_err(|_| "Could not read the transcription response.".to_owned())?;
        chunk.truncate(read as usize);
        response.extend_from_slice(&chunk);
    }
    Ok((status, response))
}
