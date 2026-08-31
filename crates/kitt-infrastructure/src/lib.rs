use kitt_domain::{
    AssistantError, MemoryPort, ModelAnswer, ModelPort, ModelRequest, Result, SpeechOutputPort,
    TranscriptionPort,
};
use kitt_memory_core::{
    EgressPolicy, MemoryKind, MemoryScope, MemoryStore, NewMemory, RecallQuery, Sensitivity,
};
use kitt_memory_sqlite::SqliteMemoryStore;
use reqwest::blocking::{Client, Response, multipart};
use serde_json::{Value, json};
#[cfg(target_os = "windows")]
use std::io::Write;
use std::{
    fs,
    io::Read,
    path::Path,
    process::{Command, Stdio},
    sync::Arc,
    time::Duration,
};

const MAX_PROVIDER_RESPONSE_BYTES: u64 = 8 * 1024 * 1024;
const MAX_AUDIO_BYTES: u64 = 25 * 1024 * 1024;

pub struct OpenAiCompatibleModel {
    client: Client,
    base_url: String,
    model: String,
    api_key: Option<String>,
    local: bool,
}

impl OpenAiCompatibleModel {
    pub fn new(
        base_url: String,
        model: String,
        api_key: Option<String>,
        local: bool,
    ) -> Result<Self> {
        if base_url.trim().is_empty() || model.trim().is_empty() {
            return Err(AssistantError::Configuration(
                "base_url and model are required".into(),
            ));
        }
        Ok(Self {
            client: build_client()?,
            base_url: base_url.trim_end_matches('/').to_string(),
            model,
            api_key,
            local,
        })
    }
}

impl ModelPort for OpenAiCompatibleModel {
    fn complete(&self, request: &ModelRequest) -> Result<ModelAnswer> {
        let mut req = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .json(&json!({
                "model": &self.model,
                "messages": [
                    {"role":"system","content":&request.system},
                    {"role":"user","content":&request.user}
                ],
                "stream": false
            }));
        if let Some(key) = &self.api_key {
            req = req.bearer_auth(key);
        }
        let body = bounded_json(
            req.send()
                .map_err(|e| AssistantError::Model(e.to_string()))?,
            AssistantError::Model,
        )?;
        let text = body
            .pointer("/choices/0/message/content")
            .and_then(Value::as_str)
            .ok_or_else(|| AssistantError::Model("missing choices[0].message.content".into()))?;
        Ok(ModelAnswer { text: text.into() })
    }

    fn is_local(&self) -> bool {
        self.local
    }
}

pub struct OpenAiCompatibleTranscriber {
    client: Client,
    base_url: String,
    model: String,
    api_key: Option<String>,
    local: bool,
    allow_remote: bool,
}

impl OpenAiCompatibleTranscriber {
    pub fn new(
        base_url: String,
        model: String,
        api_key: Option<String>,
        local: bool,
        allow_remote: bool,
    ) -> Result<Self> {
        if base_url.trim().is_empty() || model.trim().is_empty() {
            return Err(AssistantError::Configuration(
                "transcription base_url and model are required".into(),
            ));
        }
        Ok(Self {
            client: build_client()?,
            base_url: base_url.trim_end_matches('/').into(),
            model,
            api_key,
            local,
            allow_remote,
        })
    }
}

impl TranscriptionPort for OpenAiCompatibleTranscriber {
    fn transcribe(&self, path: &Path, locale: Option<&str>) -> Result<String> {
        if !self.local && !self.allow_remote {
            return Err(AssistantError::Transcription(
                "remote voice transcription is disabled; set speech_to_text.allow_remote=true explicitly"
                    .into(),
            ));
        }
        let metadata =
            fs::metadata(path).map_err(|e| AssistantError::Io(format!("audio metadata: {e}")))?;
        if !metadata.is_file() {
            return Err(AssistantError::Transcription(
                "audio path is not a file".into(),
            ));
        }
        if metadata.len() > MAX_AUDIO_BYTES {
            return Err(AssistantError::Transcription(format!(
                "audio exceeds {} MiB limit",
                MAX_AUDIO_BYTES / 1024 / 1024
            )));
        }

        let bytes = fs::read(path).map_err(|e| AssistantError::Io(format!("audio read: {e}")))?;
        let filename = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("audio.bin")
            .to_string();
        let file = multipart::Part::bytes(bytes).file_name(filename);
        let mut form = multipart::Form::new()
            .text("model", self.model.clone())
            .part("file", file);
        if let Some(language) = normalize_language(locale) {
            form = form.text("language", language);
        }

        let mut req = self
            .client
            .post(format!("{}/audio/transcriptions", self.base_url))
            .multipart(form);
        if let Some(key) = &self.api_key {
            req = req.bearer_auth(key);
        }
        let body = bounded_json(
            req.send()
                .map_err(|e| AssistantError::Transcription(e.to_string()))?,
            AssistantError::Transcription,
        )?;
        body.get("text")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| AssistantError::Transcription("response missing text".into()))
    }

    fn is_local(&self) -> bool {
        self.local
    }
}

fn normalize_language(locale: Option<&str>) -> Option<String> {
    let raw = locale?.trim();
    if raw.is_empty() {
        return None;
    }
    let primary = raw.split(['-', '_']).next()?.trim().to_ascii_lowercase();
    if (2..=3).contains(&primary.len()) && primary.chars().all(|ch| ch.is_ascii_alphabetic()) {
        Some(primary)
    } else {
        None
    }
}

fn build_client() -> Result<Client> {
    Client::builder()
        .connect_timeout(Duration::from_secs(30))
        .timeout(Duration::from_secs(600))
        .build()
        .map_err(|e| AssistantError::Configuration(format!("http client: {e}")))
}

fn bounded_json(response: Response, wrap: fn(String) -> AssistantError) -> Result<Value> {
    let status = response.status();
    if response
        .content_length()
        .is_some_and(|size| size > MAX_PROVIDER_RESPONSE_BYTES)
    {
        return Err(wrap("provider response too large".into()));
    }
    let mut bytes = Vec::new();
    response
        .take(MAX_PROVIDER_RESPONSE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| wrap(format!("provider response read failed: {e}")))?;
    if bytes.len() as u64 > MAX_PROVIDER_RESPONSE_BYTES {
        return Err(wrap("provider response too large".into()));
    }
    let body: Value =
        serde_json::from_slice(&bytes).map_err(|e| wrap(format!("invalid JSON: {e}")))?;
    if !status.is_success() {
        return Err(wrap(format!(
            "HTTP {status}: {}",
            body.get("error").unwrap_or(&body)
        )));
    }
    Ok(body)
}

#[derive(Default)]
pub struct SystemTextToSpeech;

impl SystemTextToSpeech {
    pub fn new() -> Self {
        Self
    }
}

impl SpeechOutputPort for SystemTextToSpeech {
    fn speak(&self, text: &str, _locale: Option<&str>) -> Result<()> {
        if text.trim().is_empty() {
            return Ok(());
        }
        speak_system(text)
    }
}

#[cfg(target_os = "windows")]
fn speak_system(text: &str) -> Result<()> {
    let script = concat!(
        "$text=[Console]::In.ReadToEnd();",
        "Add-Type -AssemblyName System.Speech;",
        "$speaker=New-Object System.Speech.Synthesis.SpeechSynthesizer;",
        "$speaker.Speak($text);"
    );
    let mut child = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| AssistantError::SpeechOutput(format!("start PowerShell TTS: {e}")))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(text.as_bytes())
            .map_err(|e| AssistantError::SpeechOutput(format!("write TTS input: {e}")))?;
    }
    let status = child
        .wait()
        .map_err(|e| AssistantError::SpeechOutput(format!("wait PowerShell TTS: {e}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(AssistantError::SpeechOutput(format!(
            "PowerShell TTS exited with {status}"
        )))
    }
}

#[cfg(target_os = "macos")]
fn speak_system(text: &str) -> Result<()> {
    let status = Command::new("say")
        .arg(text)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|e| AssistantError::SpeechOutput(format!("start macOS say: {e}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(AssistantError::SpeechOutput(format!(
            "macOS say exited with {status}"
        )))
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
fn speak_system(text: &str) -> Result<()> {
    match Command::new("spd-say")
        .args(["-w", "--"])
        .arg(text)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
    {
        Ok(status) if status.success() => return Ok(()),
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(AssistantError::SpeechOutput(format!(
                "start spd-say: {error}"
            )));
        }
    }

    let path = temporary_text_path();
    write_private_tts_text(&path, text)?;
    let _guard = TempFileGuard(path.clone());
    for program in ["espeak-ng", "espeak"] {
        match Command::new(program)
            .args(["-f"])
            .arg(&path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
        {
            Ok(status) if status.success() => return Ok(()),
            Ok(_) => continue,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(AssistantError::SpeechOutput(format!(
                    "start {program}: {error}"
                )));
            }
        }
    }
    Err(AssistantError::SpeechOutput(
        "no system TTS backend found; install speech-dispatcher or espeak-ng".into(),
    ))
}

#[cfg(not(any(unix, target_os = "windows")))]
fn speak_system(_text: &str) -> Result<()> {
    Err(AssistantError::SpeechOutput(
        "system TTS is not implemented for this platform".into(),
    ))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn temporary_text_path() -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    std::env::temp_dir().join(format!("kitt-tts-{}-{nanos}.txt", std::process::id()))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn write_private_tts_text(path: &Path, text: &str) -> Result<()> {
    use std::io::Write as _;
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(|e| AssistantError::Io(format!("create private TTS text: {e}")))?;
    file.write_all(text.as_bytes())
        .map_err(|e| AssistantError::Io(format!("write TTS text: {e}")))?;
    file.flush()
        .map_err(|e| AssistantError::Io(format!("flush TTS text: {e}")))
}

#[cfg(all(unix, not(target_os = "macos")))]
struct TempFileGuard(std::path::PathBuf);

#[cfg(all(unix, not(target_os = "macos")))]
impl Drop for TempFileGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

pub struct AssistantMemory {
    store: Arc<SqliteMemoryStore>,
    workspace_id: String,
    allow_personal_remote: bool,
}

impl AssistantMemory {
    pub fn new(
        store: Arc<SqliteMemoryStore>,
        workspace_id: String,
        allow_personal_remote: bool,
    ) -> Self {
        Self {
            store,
            workspace_id,
            allow_personal_remote,
        }
    }
}

impl MemoryPort for AssistantMemory {
    fn recall_for_model(
        &self,
        query: &str,
        is_local_provider: bool,
    ) -> Result<Vec<kitt_memory_core::MemoryRecord>> {
        let policy = EgressPolicy {
            is_local_provider,
            allow_personal_remote: self.allow_personal_remote,
        };
        let mut rows = self
            .store
            .recall(&RecallQuery {
                namespace: "assistant".into(),
                workspace_id: self.workspace_id.clone(),
                text: query.into(),
                limit: 6,
                allow_private: is_local_provider,
                allow_secret: is_local_provider,
            })
            .map_err(|e| AssistantError::Memory(e.to_string()))?;
        rows.retain(|memory| policy.allows(memory.sensitivity));
        Ok(rows)
    }

    fn remember_episode(&self, text: &str) -> Result<()> {
        self.store
            .remember(NewMemory {
                namespace: "assistant".into(),
                workspace_id: self.workspace_id.clone(),
                kind: MemoryKind::Episodic,
                content: text.into(),
                sensitivity: Sensitivity::Private,
                scope: MemoryScope::Global,
                importance: 0.25,
                confidence: 1.0,
                pinned: false,
                ttl_seconds: Some(30 * 24 * 3600),
                metadata_json: "{\"source\":\"assistant_session\"}".into(),
            })
            .map_err(|e| AssistantError::Memory(e.to_string()))?;
        Ok(())
    }

    fn remember_explicit(&self, text: &str) -> Result<String> {
        let memory = self
            .store
            .remember(NewMemory {
                namespace: "assistant".into(),
                workspace_id: self.workspace_id.clone(),
                kind: MemoryKind::PersonalFact,
                content: text.into(),
                sensitivity: Sensitivity::Private,
                scope: MemoryScope::Global,
                importance: 0.8,
                confidence: 1.0,
                pinned: true,
                ttl_seconds: None,
                metadata_json: "{\"source\":\"explicit_remember\"}".into(),
            })
            .map_err(|e| AssistantError::Memory(e.to_string()))?;
        Ok(memory.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_assistant_memory_egress_filter() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let temp = std::env::temp_dir().join(format!("test-mem-{nanos}.db"));
        let store = Arc::new(SqliteMemoryStore::open(&temp).unwrap());
        store
            .remember(NewMemory {
                namespace: "assistant".into(),
                workspace_id: "ws".into(),
                kind: MemoryKind::TechnicalFact,
                content: "Local Only Secret Fact".into(),
                sensitivity: Sensitivity::Private,
                scope: MemoryScope::Global,
                importance: 0.9,
                confidence: 1.0,
                pinned: true,
                ttl_seconds: None,
                metadata_json: "{}".into(),
            })
            .unwrap();

        let memory = AssistantMemory::new(store, "ws".into(), false);
        let local_recalled = memory.recall_for_model("Local", true).unwrap();
        assert_eq!(local_recalled.len(), 1);

        let remote_recalled = memory.recall_for_model("Local", false).unwrap();
        assert_eq!(remote_recalled.len(), 0);
        let _ = std::fs::remove_file(&temp);
    }

    #[test]
    fn test_remote_stt_disabled_by_default() {
        let transcriber = OpenAiCompatibleTranscriber::new(
            "http://example.com/v1".into(),
            "whisper-1".into(),
            None,
            false,
            false,
        )
        .unwrap();

        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let temp = std::env::temp_dir().join(format!("test-audio-{nanos}.wav"));
        std::fs::write(&temp, b"dummy audio content").unwrap();
        let result = transcriber.transcribe(&temp, None);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("remote voice transcription is disabled")
        );
        let _ = std::fs::remove_file(&temp);
    }

    #[test]
    fn locale_is_reduced_to_iso_639_primary_language() {
        assert_eq!(normalize_language(Some("pt-BR")), Some("pt".into()));
        assert_eq!(normalize_language(Some("en_US")), Some("en".into()));
        assert_eq!(normalize_language(Some("de")), Some("de".into()));
        assert_eq!(normalize_language(Some("")), None);
        assert_eq!(normalize_language(Some("invalid-locale-name")), None);
    }
}
