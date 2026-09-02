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
        if base_url.trim().is_empty() {
            return Err(AssistantError::Configuration(
                "model provider base_url is required".into(),
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
        if self.model.trim().is_empty() {
            return Err(AssistantError::Configuration(
                "no model is configured for this provider; select one in KITT Control Center or set the provider model explicitly"
                    .into(),
            ));
        }
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

fn normalize_discovery_base_url(base_url: &str) -> Result<String> {
    let raw = base_url.trim();
    if raw.is_empty() {
        return Err(AssistantError::Configuration(
            "model discovery base_url is required".into(),
        ));
    }
    let parsed = reqwest::Url::parse(raw)
        .map_err(|e| AssistantError::Configuration(format!("invalid discovery URL: {e}")))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(AssistantError::Configuration(
            "model discovery only permits http/https URLs".into(),
        ));
    }
    if parsed.host_str().is_none() {
        return Err(AssistantError::Configuration(
            "model discovery URL requires a host".into(),
        ));
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(AssistantError::Configuration(
            "credentials embedded in model discovery URLs are forbidden".into(),
        ));
    }
    if parsed.query().is_some() || parsed.fragment().is_some() {
        return Err(AssistantError::Configuration(
            "model discovery base_url cannot contain query or fragment".into(),
        ));
    }
    Ok(parsed.to_string().trim_end_matches('/').to_string())
}

pub fn discover_models_from_url(base_url: &str, api_key: Option<&str>) -> Result<Vec<String>> {
    let base = normalize_discovery_base_url(base_url)?;
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(2))
        .timeout(Duration::from_secs(5))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| AssistantError::Model(e.to_string()))?;

    let models_url = if base.ends_with("/v1") {
        format!("{base}/models")
    } else {
        format!("{base}/v1/models")
    };
    let mut req = client.get(&models_url);
    if let Some(key) = api_key.filter(|key| !key.trim().is_empty()) {
        req = req.bearer_auth(key);
    }
    if let Ok(resp) = req.send() {
        if let Ok(body) = bounded_json(resp, AssistantError::Model) {
            if let Some(data) = body.get("data").and_then(Value::as_array) {
                let mut list: Vec<String> = data
                    .iter()
                    .filter_map(|m| m.get("id").and_then(Value::as_str))
                    .map(str::to_string)
                    .collect();
                if !list.is_empty() {
                    list.sort();
                    list.dedup();
                    return Ok(list);
                }
            }
        }
    }

    let root_base = base
        .strip_suffix("/v1")
        .unwrap_or(&base)
        .trim_end_matches('/');
    let tags_url = format!("{root_base}/api/tags");
    if let Ok(resp) = client.get(&tags_url).send() {
        if let Ok(body) = bounded_json(resp, AssistantError::Model) {
            if let Some(models) = body.get("models").and_then(Value::as_array) {
                let mut list: Vec<String> = models
                    .iter()
                    .filter_map(|m| {
                        m.get("name")
                            .or_else(|| m.get("model"))
                            .and_then(Value::as_str)
                    })
                    .map(str::to_string)
                    .collect();
                if !list.is_empty() {
                    list.sort();
                    list.dedup();
                    return Ok(list);
                }
            }
        }
    }

    Ok(Vec::new())
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
        if base_url.trim().is_empty() {
            return Err(AssistantError::Configuration(
                "transcription base_url is required".into(),
            ));
        }
        if !local && model.trim().is_empty() {
            return Err(AssistantError::Configuration(
                "remote transcription requires an explicit model".into(),
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
    fn transcribe_rich(
        &self,
        path: &Path,
        locale: Option<&str>,
        prompt: Option<&str>,
    ) -> Result<kitt_domain::TranscriptionResult> {
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
        let mut form = multipart::Form::new().part("file", file);
        if !self.model.trim().is_empty() {
            form = form.text("model", self.model.clone());
        }
        if let Some(language) = normalize_language(locale) {
            form = form.text("language", language);
        }
        if let Some(prompt) = prompt.map(str::trim).filter(|value| !value.is_empty()) {
            form = form.text("prompt", prompt.chars().take(512).collect::<String>());
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
        let text = body
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let language = body
            .get("language")
            .and_then(Value::as_str)
            .map(str::to_string);
        let language_probability = body
            .get("language_probability")
            .and_then(Value::as_f64)
            .map(|v| v as f32);
        let avg_logprob = body
            .get("avg_logprob")
            .and_then(Value::as_f64)
            .map(|v| v as f32);
        let duration_ms = body.get("duration_ms").and_then(Value::as_u64).unwrap_or(0);
        Ok(kitt_domain::TranscriptionResult {
            text,
            language,
            language_probability,
            avg_logprob,
            duration_ms,
        })
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
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(45))
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

#[derive(Debug, Clone)]
pub struct SystemVoiceProfile {
    pub voice_name: Option<String>,
    pub prefer_male: bool,
    pub rate: i32,
    pub pitch: i32,
    pub volume: u8,
    pub timeout: Duration,
}

impl Default for SystemVoiceProfile {
    fn default() -> Self {
        Self {
            voice_name: None,
            prefer_male: true,
            rate: -1,
            pitch: -2,
            volume: 95,
            timeout: Duration::from_secs(30),
        }
    }
}

impl SystemVoiceProfile {
    fn normalized(&self) -> Self {
        Self {
            voice_name: self
                .voice_name
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string),
            prefer_male: self.prefer_male,
            rate: self.rate.clamp(-10, 10),
            pitch: self.pitch.clamp(-10, 10),
            volume: self.volume.min(100),
            timeout: self
                .timeout
                .clamp(Duration::from_millis(500), Duration::from_secs(120)),
        }
    }
}

pub struct SystemTextToSpeech {
    profile: SystemVoiceProfile,
}

impl Default for SystemTextToSpeech {
    fn default() -> Self {
        Self::new(SystemVoiceProfile::default())
    }
}

impl SystemTextToSpeech {
    pub fn new(profile: SystemVoiceProfile) -> Self {
        Self {
            profile: profile.normalized(),
        }
    }
}

impl SpeechOutputPort for SystemTextToSpeech {
    fn speak(&self, text: &str, locale: Option<&str>) -> Result<()> {
        if text.trim().is_empty() {
            return Ok(());
        }
        speak_system(text, locale, &self.profile)
    }
}

#[derive(Debug, Clone)]
pub struct PiperVoiceProfile {
    pub base_url: String,
    pub voice: Option<String>,
    pub speaker: Option<i32>,
    pub length_scale: Option<f32>,
    pub noise_scale: Option<f32>,
    pub noise_w_scale: Option<f32>,
    pub timeout: Duration,
    pub fallback_system: Option<SystemVoiceProfile>,
}

pub struct PiperTextToSpeech {
    profile: PiperVoiceProfile,
    client: Client,
}

impl PiperTextToSpeech {
    pub fn new(profile: PiperVoiceProfile) -> Result<Self> {
        let client = Client::builder()
            .timeout(profile.timeout)
            .build()
            .map_err(|e| AssistantError::Configuration(format!("piper http client: {e}")))?;
        Ok(Self { profile, client })
    }
}

impl SpeechOutputPort for PiperTextToSpeech {
    fn speak(&self, text: &str, locale: Option<&str>) -> Result<()> {
        if text.trim().is_empty() {
            return Ok(());
        }
        let piper_res = (|| -> Result<()> {
            let mut req = self.client.post(&self.profile.base_url);
            let mut payload = json!({
                "text": text,
            });
            if let Some(voice) = &self.profile.voice {
                payload["voice"] = json!(voice);
            }
            if let Some(speaker) = self.profile.speaker {
                payload["speaker_id"] = json!(speaker);
            }
            if let Some(ls) = self.profile.length_scale {
                payload["length_scale"] = json!(ls);
            }
            if let Some(ns) = self.profile.noise_scale {
                payload["noise_scale"] = json!(ns);
            }
            if let Some(nws) = self.profile.noise_w_scale {
                payload["noise_w_scale"] = json!(nws);
            }
            req = req.json(&payload);
            let res = req
                .send()
                .map_err(|e| AssistantError::SpeechOutput(format!("piper send: {e}")))?;
            if !res.status().is_success() {
                return Err(AssistantError::SpeechOutput(format!(
                    "piper returned HTTP {}",
                    res.status()
                )));
            }
            // If Piper returned wav audio directly, write to temp and play
            let bytes = res
                .bytes()
                .map_err(|e| AssistantError::SpeechOutput(format!("piper read: {e}")))?;
            if !bytes.is_empty() {
                let temp_path =
                    std::env::temp_dir().join(format!("kitt-piper-{}.wav", std::process::id()));
                fs::write(&temp_path, &bytes)
                    .map_err(|e| AssistantError::Io(format!("save piper audio: {e}")))?;
                let _guard = TempFileGuard(temp_path.clone());
                #[cfg(all(unix, not(target_os = "macos")))]
                {
                    let _ = Command::new("aplay").arg(&temp_path).status();
                }
                #[cfg(target_os = "macos")]
                {
                    let _ = Command::new("afplay").arg(&temp_path).status();
                }
                #[cfg(target_os = "windows")]
                {
                    let script = format!(
                        "(New-Object System.Media.SoundPlayer '{}').PlaySync();",
                        temp_path.display()
                    );
                    let _ = Command::new("powershell.exe")
                        .args(["-NoProfile", "-Command", &script])
                        .status();
                }
            }
            Ok(())
        })();

        match piper_res {
            Ok(()) => Ok(()),
            Err(e) => {
                if let Some(fallback) = &self.profile.fallback_system {
                    speak_system(text, locale, fallback)
                } else {
                    Err(e)
                }
            }
        }
    }
}

fn wait_child_with_timeout(mut child: std::process::Child, timeout: Duration) -> Result<()> {
    let start = std::time::Instant::now();
    let poll_interval = Duration::from_millis(25);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if status.success() {
                    return Ok(());
                } else {
                    return Err(AssistantError::SpeechOutput(format!(
                        "TTS process exited with status {status}"
                    )));
                }
            }
            Ok(None) => {
                if start.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(AssistantError::SpeechOutput("TTS process timed out".into()));
                }
                std::thread::sleep(poll_interval);
            }
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(AssistantError::SpeechOutput(format!(
                    "wait TTS process: {e}"
                )));
            }
        }
    }
}

fn normalized_tts_locale(locale: Option<&str>) -> Option<String> {
    let value = locale?.trim();
    if value.is_empty() {
        return None;
    }
    Some(value.replace('_', "-").to_ascii_lowercase())
}

fn macos_speech_rate(rate: i32) -> i32 {
    (175 + rate.clamp(-10, 10) * 8).clamp(95, 255)
}

fn espeak_pitch(pitch: i32) -> i32 {
    (50 + pitch.clamp(-10, 10) * 4).clamp(0, 99)
}

#[cfg(target_os = "windows")]
fn speak_system(text: &str, locale: Option<&str>, profile: &SystemVoiceProfile) -> Result<()> {
    let script = concat!(
        "$text=[Console]::In.ReadToEnd();",
        "Add-Type -AssemblyName System.Speech;",
        "$speaker=New-Object System.Speech.Synthesis.SpeechSynthesizer;",
        "$voice=$env:KITT_TTS_VOICE_NAME;",
        "$preferMale=$env:KITT_TTS_PREFER_MALE -eq '1';",
        "if($voice){try{$speaker.SelectVoice($voice)}catch{}}",
        "elseif($preferMale){try{",
        "$culture=[System.Globalization.CultureInfo]::GetCultureInfo($env:KITT_TTS_LOCALE);",
        "$speaker.SelectVoiceByHints([System.Speech.Synthesis.VoiceGender]::Male,[System.Speech.Synthesis.VoiceAge]::Adult,0,$culture)",
        "}catch{try{$speaker.SelectVoiceByHints([System.Speech.Synthesis.VoiceGender]::Male)}catch{}}};",
        "$speaker.Rate=[int]$env:KITT_TTS_RATE;",
        "$speaker.Volume=[int]$env:KITT_TTS_VOLUME;",
        "$speaker.Speak($text);"
    );
    let mut child = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .env(
            "KITT_TTS_VOICE_NAME",
            profile.voice_name.as_deref().unwrap_or_default(),
        )
        .env(
            "KITT_TTS_PREFER_MALE",
            if profile.prefer_male { "1" } else { "0" },
        )
        .env("KITT_TTS_LOCALE", locale.unwrap_or("pt-BR"))
        .env("KITT_TTS_RATE", profile.rate.to_string())
        .env("KITT_TTS_VOLUME", profile.volume.to_string())
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
    wait_child_with_timeout(child, profile.timeout)
}

#[cfg(target_os = "macos")]
fn speak_system(text: &str, _locale: Option<&str>, profile: &SystemVoiceProfile) -> Result<()> {
    let mut command = Command::new("say");
    if let Some(voice) = profile.voice_name.as_deref() {
        command.arg("-v").arg(voice);
    }
    let child = command
        .arg("-r")
        .arg(macos_speech_rate(profile.rate).to_string())
        .arg(text)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| AssistantError::SpeechOutput(format!("start macOS say: {e}")))?;
    wait_child_with_timeout(child, profile.timeout)
}

#[cfg(all(unix, not(target_os = "macos")))]
fn speak_system(text: &str, locale: Option<&str>, profile: &SystemVoiceProfile) -> Result<()> {
    let mut speech_dispatcher = Command::new("spd-say");
    speech_dispatcher
        .arg("-w")
        .arg("-r")
        .arg((profile.rate * 10).to_string())
        .arg("-p")
        .arg((profile.pitch * 10).to_string())
        .arg("-i")
        .arg((i32::from(profile.volume) * 2 - 100).to_string());
    if let Some(locale) = normalized_tts_locale(locale) {
        speech_dispatcher.arg("-l").arg(locale);
    }
    if let Some(voice) = profile.voice_name.as_deref() {
        speech_dispatcher.arg("-y").arg(voice);
    } else if profile.prefer_male {
        speech_dispatcher.arg("-t").arg("male1");
    }
    if let Ok(child) = speech_dispatcher
        .arg("--")
        .arg(text)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        if wait_child_with_timeout(child, profile.timeout).is_ok() {
            return Ok(());
        }
    }

    let path = temporary_text_path();
    write_private_tts_text(&path, text)?;
    let _guard = TempFileGuard(path.clone());
    let fallback_voice = profile
        .voice_name
        .clone()
        .or_else(|| normalized_tts_locale(locale));
    for program in ["espeak-ng", "espeak"] {
        let mut command = Command::new(program);
        if let Some(voice) = fallback_voice.as_deref() {
            command.arg("-v").arg(voice);
        }
        command
            .arg("-s")
            .arg(macos_speech_rate(profile.rate).to_string())
            .arg("-p")
            .arg(espeak_pitch(profile.pitch).to_string())
            .arg("-a")
            .arg((u32::from(profile.volume) * 2).min(200).to_string())
            .arg("-f")
            .arg(&path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if let Ok(child) = command.spawn() {
            if wait_child_with_timeout(child, profile.timeout).is_ok() {
                return Ok(());
            }
        }
    }
    Err(AssistantError::SpeechOutput(
        "no system TTS backend found; install speech-dispatcher or espeak-ng".into(),
    ))
}

#[cfg(not(any(unix, target_os = "windows")))]
fn speak_system(_text: &str, _locale: Option<&str>, _profile: &SystemVoiceProfile) -> Result<()> {
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

struct TempFileGuard(std::path::PathBuf);

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
    fn voice_profile_is_bounded_and_has_no_named_voice_fallback() {
        let profile = SystemVoiceProfile {
            voice_name: Some("   ".into()),
            prefer_male: true,
            rate: -99,
            pitch: 99,
            volume: 255,
            timeout: Duration::from_secs(30),
        }
        .normalized();
        assert_eq!(profile.voice_name, None);
        assert_eq!(profile.rate, -10);
        assert_eq!(profile.pitch, 10);
        assert_eq!(profile.volume, 100);
        assert_eq!(profile.timeout, Duration::from_secs(30));
    }

    #[test]
    fn discovery_url_validation_rejects_credential_and_non_http_urls() {
        assert!(normalize_discovery_base_url("http://127.0.0.1:11434/v1").is_ok());
        assert!(normalize_discovery_base_url("https://api.example.com/v1").is_ok());
        assert!(normalize_discovery_base_url("file:///tmp/models").is_err());
        assert!(normalize_discovery_base_url("https://user:secret@example.com/v1").is_err());
        assert!(normalize_discovery_base_url("https://example.com/v1?token=secret").is_err());
    }

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
        let result = transcriber.transcribe(&temp, None, None);
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
