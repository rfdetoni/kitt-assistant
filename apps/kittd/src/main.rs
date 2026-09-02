mod model_config;
mod settings_overlay;
mod settings_web;
mod voice;

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use kitt_application::AssistantService;
use kitt_domain::{ModelTier as DomainModelTier, RouteHint, RoutingPolicy};
use kitt_infrastructure::{
    AssistantMemory, OpenAiCompatibleModel, OpenAiCompatibleTranscriber, SystemTextToSpeech,
    SystemVoiceProfile,
};
use kitt_memory_core::{
    MemoryKind as CoreMemoryKind, MemoryRecord, MemoryScope as CoreMemoryScope, MemoryStore,
    NewMemory, RecallQuery, Sensitivity as CoreSensitivity,
};
use kitt_memory_sqlite::SqliteMemoryStore;
use kitt_protocol::{
    AskRequest, AskResponse, AssistantRememberRequest, AuthenticatedFrame, DeleteResponse,
    Envelope, HudEvent, HudImageRequest, HudState, IdResponse, MAX_FRAME_BYTES, MemoryDto,
    MemoryForgetRequest, MemoryKind, MemoryRecallRequest, MemoryRecallResponse,
    MemoryRememberRequest, MemoryScope, ModelRoute, ModelTier, RoutedAskRequest, RoutedAskResponse,
    Sensitivity, ShownResponse, StatusResponse, TranscribeRequest, TranscribeResponse, kinds,
};
use model_config::{ModelProfiles, api_key};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Child, Command},
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::Duration,
};
use uuid::Uuid;

const MAX_CONNECTIONS: usize = 64;
const MAX_HUD_SUBSCRIBERS: usize = 8;
const AUTH_READ_TIMEOUT: Duration = Duration::from_secs(5);
const WRITE_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct Config {
    listen: String,
    base_url: String,
    model: String,
    api_key_env: Option<String>,
    local_provider: bool,
    allow_personal_remote: bool,
    hud_ttl_ms: u64,
    tts_voice_name: Option<String>,
    tts_prefer_male: bool,
    tts_rate: i32,
    tts_pitch: i32,
    tts_volume: u8,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            listen: "127.0.0.1:41827".into(),
            base_url: "http://127.0.0.1:11434/v1".into(),
            model: std::env::var("KITT_MODEL").unwrap_or_default(),
            api_key_env: None,
            local_provider: true,
            allow_personal_remote: false,
            hud_ttl_ms: 8000,
            tts_voice_name: None,
            tts_prefer_male: true,
            tts_rate: -1,
            tts_pitch: -2,
            tts_volume: 95,
        }
    }
}

struct Runtime {
    token: String,
    service: Arc<AssistantService>,
    memory: Arc<SqliteMemoryStore>,
    hud: HudBroadcaster,
    hud_process: Mutex<Option<Child>>,
    stt_base_url: String,
    stt_worker_process: Mutex<Option<Child>>,
    config: Config,
    active_connections: AtomicUsize,
}

struct ConnectionGuard<'a>(&'a AtomicUsize);

impl Drop for ConnectionGuard<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

#[derive(Clone)]
struct HudBroadcaster {
    clients: Arc<Mutex<Vec<TcpStream>>>,
    last_event: Arc<Mutex<Option<String>>>,
}

impl HudBroadcaster {
    fn new() -> Self {
        Self {
            clients: Arc::new(Mutex::new(Vec::new())),
            last_event: Arc::new(Mutex::new(None)),
        }
    }

    fn subscribe(&self, mut stream: TcpStream, request_id: &str) -> Result<(), String> {
        let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
        let mut clients = self
            .clients
            .lock()
            .map_err(|_| "HUD subscriber lock poisoned".to_string())?;
        if clients.len() >= MAX_HUD_SUBSCRIBERS {
            return Err("HUD subscriber limit reached".into());
        }

        let ack = Envelope::response(
            kinds::HUD_SUBSCRIBE_RESPONSE,
            request_id,
            StatusResponse {
                status: "subscribed".into(),
            },
        )
        .map_err(|e| e.to_string())?;
        let ack_line = serde_json::to_string(&ack).map_err(|e| e.to_string())? + "\n";
        stream
            .write_all(ack_line.as_bytes())
            .map_err(|e| e.to_string())?;

        if let Ok(last) = self.last_event.lock() {
            if let Some(line) = last.as_ref() {
                stream
                    .write_all(line.as_bytes())
                    .map_err(|e| e.to_string())?;
            }
        }

        clients.push(stream);
        Ok(())
    }

    fn send(&self, event: HudEvent) {
        let envelope = match Envelope::new(kinds::HUD_EVENT, event) {
            Ok(envelope) => envelope,
            Err(_) => return,
        };
        let line = match serde_json::to_string(&envelope) {
            Ok(line) => line + "\n",
            Err(_) => return,
        };
        if let Ok(mut last) = self.last_event.lock() {
            *last = Some(line.clone());
        }
        if let Ok(mut clients) = self.clients.lock() {
            clients.retain_mut(|stream| stream.write_all(line.as_bytes()).is_ok());
        }
    }
}

fn main() {
    let paths = Paths::load();
    let mut config = load_config(&paths).unwrap_or_else(|e| fatal(e));
    settings_overlay::apply_core(&paths.dir, &mut config).unwrap_or_else(|e| fatal(e));
    validate_loopback(&config.listen).unwrap_or_else(|e| fatal(e));
    validate_provider_locality(&config).unwrap_or_else(|e| fatal(e));
    let token = load_or_create_token(&paths).unwrap_or_else(|e| fatal(e));
    let memory = Arc::new(
        SqliteMemoryStore::open(&paths.memory_db).unwrap_or_else(|e| fatal(e.to_string())),
    );
    let mut profiles = ModelProfiles::load_or_create(
        &paths.dir,
        &config.base_url,
        &config.model,
        config.api_key_env.clone(),
        config.local_provider,
    )
    .unwrap_or_else(|e| fatal(e));
    settings_overlay::apply_models(&paths.dir, &mut profiles).unwrap_or_else(|e| fatal(e));
    profiles.validate().unwrap_or_else(|e| fatal(e));
    let stt_base_url = profiles.speech_to_text.base_url.clone();
    let fast_model = Arc::new(
        OpenAiCompatibleModel::new(
            profiles.fast.base_url.clone(),
            profiles.fast.model.clone(),
            api_key(profiles.fast.api_key_env.as_ref()),
            profiles.fast.local_provider,
        )
        .unwrap_or_else(|e| fatal(e.to_string())),
    );
    let heavy_model = Arc::new(
        OpenAiCompatibleModel::new(
            profiles.heavy.base_url.clone(),
            profiles.heavy.model.clone(),
            api_key(profiles.heavy.api_key_env.as_ref()),
            profiles.heavy.local_provider,
        )
        .unwrap_or_else(|e| fatal(e.to_string())),
    );
    let transcriber = Arc::new(
        OpenAiCompatibleTranscriber::new(
            profiles.speech_to_text.base_url.clone(),
            profiles.speech_to_text.model.clone(),
            api_key(profiles.speech_to_text.api_key_env.as_ref()),
            profiles.speech_to_text.local_provider,
            profiles.speech_to_text.allow_remote,
        )
        .unwrap_or_else(|e| fatal(e.to_string())),
    );
    let speaker = Arc::new(SystemTextToSpeech::new(SystemVoiceProfile {
        voice_name: config.tts_voice_name.clone(),
        prefer_male: config.tts_prefer_male,
        rate: config.tts_rate,
        pitch: config.tts_pitch,
        volume: config.tts_volume,
        timeout: Duration::from_secs(30),
    }));
    let memory_adapter = Arc::new(AssistantMemory::new(
        memory.clone(),
        "global".into(),
        config.allow_personal_remote,
    ));
    let service = Arc::new(AssistantService::new(
        fast_model,
        heavy_model,
        transcriber,
        speaker,
        memory_adapter,
        RoutingPolicy {
            fast_max_chars: profiles.fast_max_chars,
            fast_max_lines: profiles.fast_max_lines,
        },
    ));
    let runtime = Arc::new(Runtime {
        token,
        service,
        memory,
        hud: HudBroadcaster::new(),
        hud_process: Mutex::new(None),
        stt_base_url,
        stt_worker_process: Mutex::new(None),
        config: config.clone(),
        active_connections: AtomicUsize::new(0),
    });

    let listener = TcpListener::bind(&config.listen)
        .unwrap_or_else(|e| fatal(format!("bind {}: {e}", config.listen)));
    eprintln!("kittd listening on {}", config.listen);

    if let Err(error) = voice::start(runtime.clone(), &paths.dir) {
        eprintln!("kitt voice startup: {error}");
    }
    let kitt_root = paths.dir.parent().unwrap_or(&paths.dir);
    if let Err(error) = settings_web::start(kitt_root) {
        eprintln!("KITT Control Center startup: {error}");
    }

    for incoming in listener.incoming() {
        match incoming {
            Ok(stream) => {
                let previous = runtime.active_connections.fetch_add(1, Ordering::AcqRel);
                if previous >= MAX_CONNECTIONS {
                    runtime.active_connections.fetch_sub(1, Ordering::AcqRel);
                    let _ = stream.shutdown(std::net::Shutdown::Both);
                    continue;
                }
                let runtime = runtime.clone();
                thread::spawn(move || {
                    let _guard = ConnectionGuard(&runtime.active_connections);
                    handle_client(stream, runtime.clone());
                });
            }
            Err(error) => eprintln!("accept: {error}"),
        }
    }
}

fn handle_client(mut stream: TcpStream, runtime: Arc<Runtime>) {
    let _ = stream.set_read_timeout(Some(AUTH_READ_TIMEOUT));
    let _ = stream.set_write_timeout(Some(WRITE_TIMEOUT));

    let bytes = match read_frame(&stream) {
        Ok(bytes) => bytes,
        Err(code) => {
            reply_error(&mut stream, None, code, code);
            return;
        }
    };

    let frame = match AuthenticatedFrame::decode(&bytes) {
        Ok(frame) => frame,
        Err(error) => {
            reply_error(&mut stream, None, "invalid_frame", &error);
            return;
        }
    };

    if !secure_eq(frame.token.as_bytes(), runtime.token.as_bytes()) {
        reply_error(
            &mut stream,
            Some(&frame.envelope.id),
            "unauthorized",
            "invalid authentication token",
        );
        return;
    }

    let request = frame.envelope;
    let request_id = request.id.clone();
    if let Err((code, message)) = dispatch(&mut stream, &runtime, request) {
        reply_error(&mut stream, Some(&request_id), code, &message);
    }
}

fn read_frame(stream: &TcpStream) -> Result<Vec<u8>, &'static str> {
    let clone = stream.try_clone().map_err(|_| "stream_clone_failed")?;
    let mut reader = BufReader::new(clone).take(MAX_FRAME_BYTES as u64 + 1);
    let mut bytes = Vec::new();
    reader
        .read_until(b'\n', &mut bytes)
        .map_err(|_| "frame_read_failed")?;
    if bytes.len() > MAX_FRAME_BYTES {
        return Err("request_too_large");
    }
    if bytes.is_empty() {
        return Err("empty_request");
    }
    if !bytes.ends_with(b"\n") {
        return Err("frame_not_terminated");
    }
    while bytes.last().is_some_and(|b| *b == b'\n' || *b == b'\r') {
        bytes.pop();
    }
    Ok(bytes)
}

fn dispatch(
    stream: &mut TcpStream,
    runtime: &Arc<Runtime>,
    request: Envelope,
) -> Result<(), (&'static str, String)> {
    if request.correlation_id.is_some() {
        return Err((
            "invalid_request",
            "request envelopes cannot contain correlation_id".into(),
        ));
    }

    match request.kind.as_str() {
        kinds::SYSTEM_PING_REQUEST => {
            ensure_empty_payload(&request.payload)?;
            reply_response(
                stream,
                kinds::SYSTEM_PING_RESPONSE,
                &request.id,
                StatusResponse {
                    status: "ok".into(),
                },
            )
        }
        kinds::ASSISTANT_ASK_REQUEST => {
            let payload: AskRequest = payload_as(&request.payload)?;
            let response = ask(runtime, payload)?;
            reply_response(stream, kinds::ASSISTANT_ASK_RESPONSE, &request.id, response)
        }
        kinds::ASSISTANT_ASK_ROUTED_REQUEST => {
            let payload: RoutedAskRequest = payload_as(&request.payload)?;
            let response = ask_routed(runtime, payload)?;
            reply_response(
                stream,
                kinds::ASSISTANT_ASK_ROUTED_RESPONSE,
                &request.id,
                response,
            )
        }
        kinds::ASSISTANT_TRANSCRIBE_REQUEST => {
            let payload: TranscribeRequest = payload_as(&request.payload)?;
            let response = transcribe(runtime, payload)?;
            reply_response(
                stream,
                kinds::ASSISTANT_TRANSCRIBE_RESPONSE,
                &request.id,
                response,
            )
        }
        kinds::ASSISTANT_REMEMBER_REQUEST => {
            let payload: AssistantRememberRequest = payload_as(&request.payload)?;
            let id = runtime
                .service
                .remember(payload.text.trim())
                .map_err(|e| ("assistant_error", e.to_string()))?;
            reply_response(
                stream,
                kinds::ASSISTANT_REMEMBER_RESPONSE,
                &request.id,
                IdResponse { id },
            )
        }
        kinds::MEMORY_REMEMBER_REQUEST => {
            let payload: MemoryRememberRequest = payload_as(&request.payload)?;
            let id = memory_remember(runtime, payload)?;
            reply_response(
                stream,
                kinds::MEMORY_REMEMBER_RESPONSE,
                &request.id,
                IdResponse { id },
            )
        }
        kinds::MEMORY_RECALL_REQUEST => {
            let payload: MemoryRecallRequest = payload_as(&request.payload)?;
            let records = memory_recall(runtime, payload)?;
            reply_response(
                stream,
                kinds::MEMORY_RECALL_RESPONSE,
                &request.id,
                MemoryRecallResponse { records },
            )
        }
        kinds::MEMORY_FORGET_REQUEST => {
            let payload: MemoryForgetRequest = payload_as(&request.payload)?;
            let deleted = runtime
                .memory
                .forget(&payload.id)
                .map_err(|e| ("memory_error", e.to_string()))?;
            reply_response(
                stream,
                kinds::MEMORY_FORGET_RESPONSE,
                &request.id,
                DeleteResponse { deleted },
            )
        }
        kinds::HUD_IMAGE_REQUEST => {
            let payload: HudImageRequest = payload_as(&request.payload)?;
            show_image(runtime, payload)?;
            reply_response(
                stream,
                kinds::HUD_IMAGE_RESPONSE,
                &request.id,
                ShownResponse { shown: true },
            )
        }
        kinds::HUD_SUBSCRIBE_REQUEST => {
            ensure_empty_payload(&request.payload)?;
            let subscriber = stream
                .try_clone()
                .map_err(|e| ("stream_clone_failed", e.to_string()))?;
            runtime
                .hud
                .subscribe(subscriber, &request.id)
                .map_err(|e| ("hud_subscriber_error", e))
        }
        _ => Err((
            "unknown_kind",
            format!("unknown envelope kind: {}", request.kind),
        )),
    }
}

fn payload_as<T: DeserializeOwned>(value: &Value) -> Result<T, (&'static str, String)> {
    serde_json::from_value(value.clone()).map_err(|e| ("invalid_payload", e.to_string()))
}

fn ensure_empty_payload(value: &Value) -> Result<(), (&'static str, String)> {
    if value.as_object().is_some_and(|object| object.is_empty()) {
        Ok(())
    } else {
        Err(("invalid_payload", "expected empty object payload".into()))
    }
}

fn ask(runtime: &Arc<Runtime>, request: AskRequest) -> Result<AskResponse, (&'static str, String)> {
    let routed = run_ask(
        runtime,
        request.text.trim(),
        RouteHint::Auto,
        request.show_hud,
    )?;
    Ok(AskResponse { text: routed.text })
}

fn ask_routed(
    runtime: &Arc<Runtime>,
    request: RoutedAskRequest,
) -> Result<RoutedAskResponse, (&'static str, String)> {
    let hint = match request.route {
        ModelRoute::Auto => RouteHint::Auto,
        ModelRoute::Fast => RouteHint::Fast,
        ModelRoute::Heavy => RouteHint::Heavy,
    };
    let routed = run_ask(runtime, request.text.trim(), hint, request.show_hud)?;
    let tier = match routed.tier {
        DomainModelTier::Fast => ModelTier::Fast,
        DomainModelTier::Heavy => ModelTier::Heavy,
    };
    Ok(RoutedAskResponse {
        text: routed.text,
        tier,
        fallback_used: routed.fallback_used,
    })
}

fn run_ask(
    runtime: &Arc<Runtime>,
    text: &str,
    hint: RouteHint,
    show_hud: bool,
) -> Result<kitt_domain::RoutedAnswer, (&'static str, String)> {
    if text.is_empty() {
        return Err(("empty_text", "text cannot be empty".into()));
    }
    if show_hud {
        ensure_hud(runtime);
        runtime.hud.send(HudEvent::Status {
            state: HudState::Thinking,
            message: Some("K.I.T.T.".into()),
        });
    }
    match runtime.service.ask(text, hint) {
        Ok(answer) => {
            if show_hud {
                runtime.hud.send(HudEvent::Text {
                    content: answer.text.clone(),
                    ttl_ms: runtime.config.hud_ttl_ms,
                });
            }
            Ok(answer)
        }
        Err(error) => {
            if show_hud {
                runtime.hud.send(HudEvent::Status {
                    state: HudState::Error,
                    message: Some(error.to_string()),
                });
            }
            Err(("assistant_error", error.to_string()))
        }
    }
}

fn transcribe(
    runtime: &Arc<Runtime>,
    request: TranscribeRequest,
) -> Result<TranscribeResponse, (&'static str, String)> {
    let path = Path::new(request.path.trim());
    if request.path.trim().is_empty() {
        return Err(("empty_path", "audio path cannot be empty".into()));
    }
    if request.show_hud {
        ensure_hud(runtime);
        runtime.hud.send(HudEvent::Status {
            state: HudState::Listening,
            message: Some("Transcrevendo…".into()),
        });
    }
    let text = runtime
        .service
        .transcribe(path, request.locale.as_deref(), None)
        .map_err(|e| ("transcription_error", e.to_string()))?;
    if request.show_hud {
        runtime.hud.send(HudEvent::Text {
            content: text.clone(),
            ttl_ms: runtime.config.hud_ttl_ms,
        });
    }
    Ok(TranscribeResponse { text })
}

fn memory_recall(
    runtime: &Arc<Runtime>,
    request: MemoryRecallRequest,
) -> Result<Vec<MemoryDto>, (&'static str, String)> {
    let query = RecallQuery {
        namespace: request.namespace,
        workspace_id: request.workspace_id,
        text: request.query,
        limit: request.limit.clamp(1, 50),
        allow_private: request.allow_private,
        allow_secret: request.allow_secret,
    };
    runtime
        .memory
        .recall(&query)
        .map(|records| records.into_iter().map(memory_to_dto).collect())
        .map_err(|e| ("memory_error", e.to_string()))
}

fn memory_remember(
    runtime: &Arc<Runtime>,
    request: MemoryRememberRequest,
) -> Result<String, (&'static str, String)> {
    let memory = NewMemory {
        namespace: request.namespace,
        workspace_id: request.workspace_id,
        kind: protocol_kind_to_core(request.kind),
        content: request.content,
        sensitivity: protocol_sensitivity_to_core(request.sensitivity),
        scope: protocol_scope_to_core(request.scope),
        importance: request.importance,
        confidence: request.confidence,
        pinned: request.pinned,
        ttl_seconds: request.ttl_seconds,
        metadata_json: "{}".into(),
    };
    runtime
        .memory
        .remember(memory)
        .map(|record| record.id)
        .map_err(|e| ("memory_error", e.to_string()))
}

fn memory_to_dto(record: MemoryRecord) -> MemoryDto {
    MemoryDto {
        id: record.id,
        namespace: record.namespace,
        workspace_id: record.workspace_id,
        kind: core_kind_to_protocol(&record.kind),
        content: record.content,
        sensitivity: core_sensitivity_to_protocol(record.sensitivity),
        scope: core_scope_to_protocol(&record.scope),
        importance: record.importance,
        confidence: record.confidence,
        pinned: record.pinned,
        created_at: record.created_at,
        updated_at: record.updated_at,
    }
}

fn protocol_kind_to_core(value: MemoryKind) -> CoreMemoryKind {
    match value {
        MemoryKind::UserPreference => CoreMemoryKind::UserPreference,
        MemoryKind::ProjectRule => CoreMemoryKind::ProjectRule,
        MemoryKind::ArchitectureDecision => CoreMemoryKind::ArchitectureDecision,
        MemoryKind::TechnicalFact => CoreMemoryKind::TechnicalFact,
        MemoryKind::WorkingPattern => CoreMemoryKind::WorkingPattern,
        MemoryKind::FailedApproach => CoreMemoryKind::FailedApproach,
        MemoryKind::OpenIssue => CoreMemoryKind::OpenIssue,
        MemoryKind::ProjectState => CoreMemoryKind::ProjectState,
        MemoryKind::Episodic => CoreMemoryKind::Episodic,
        MemoryKind::PersonalFact => CoreMemoryKind::PersonalFact,
        MemoryKind::Routine => CoreMemoryKind::Routine,
    }
}

fn core_kind_to_protocol(value: &CoreMemoryKind) -> MemoryKind {
    match value {
        CoreMemoryKind::UserPreference => MemoryKind::UserPreference,
        CoreMemoryKind::ProjectRule => MemoryKind::ProjectRule,
        CoreMemoryKind::ArchitectureDecision => MemoryKind::ArchitectureDecision,
        CoreMemoryKind::TechnicalFact => MemoryKind::TechnicalFact,
        CoreMemoryKind::WorkingPattern => MemoryKind::WorkingPattern,
        CoreMemoryKind::FailedApproach => MemoryKind::FailedApproach,
        CoreMemoryKind::OpenIssue => MemoryKind::OpenIssue,
        CoreMemoryKind::ProjectState => MemoryKind::ProjectState,
        CoreMemoryKind::Episodic => MemoryKind::Episodic,
        CoreMemoryKind::PersonalFact => MemoryKind::PersonalFact,
        CoreMemoryKind::Routine => MemoryKind::Routine,
    }
}

fn protocol_sensitivity_to_core(value: Sensitivity) -> CoreSensitivity {
    match value {
        Sensitivity::Public => CoreSensitivity::Public,
        Sensitivity::Personal => CoreSensitivity::Personal,
        Sensitivity::Private => CoreSensitivity::Private,
        Sensitivity::Secret => CoreSensitivity::Secret,
        Sensitivity::Ephemeral => CoreSensitivity::Ephemeral,
    }
}

fn core_sensitivity_to_protocol(value: CoreSensitivity) -> Sensitivity {
    match value {
        CoreSensitivity::Public => Sensitivity::Public,
        CoreSensitivity::Personal => Sensitivity::Personal,
        CoreSensitivity::Private => Sensitivity::Private,
        CoreSensitivity::Secret => Sensitivity::Secret,
        CoreSensitivity::Ephemeral => Sensitivity::Ephemeral,
    }
}

fn protocol_scope_to_core(value: MemoryScope) -> CoreMemoryScope {
    match value {
        MemoryScope::Global => CoreMemoryScope::Global,
        MemoryScope::Workspace => CoreMemoryScope::Workspace,
        MemoryScope::Conversation => CoreMemoryScope::Conversation,
    }
}

fn core_scope_to_protocol(value: &CoreMemoryScope) -> MemoryScope {
    match value {
        CoreMemoryScope::Global => MemoryScope::Global,
        CoreMemoryScope::Workspace => MemoryScope::Workspace,
        CoreMemoryScope::Conversation => MemoryScope::Conversation,
    }
}

fn show_image(
    runtime: &Arc<Runtime>,
    request: HudImageRequest,
) -> Result<(), (&'static str, String)> {
    let raw = request.src;
    let src =
        if raw.starts_with("http://") || raw.starts_with("https://") || raw.starts_with("data:") {
            raw
        } else {
            image_data_uri(Path::new(&raw)).map_err(|e| ("image_error", e))?
        };
    ensure_hud(runtime);
    runtime.hud.send(HudEvent::Image {
        src,
        alt: request.alt,
        ttl_ms: runtime.config.hud_ttl_ms,
    });
    Ok(())
}

fn image_data_uri(path: &Path) -> Result<String, String> {
    const MAX_IMAGE_BYTES: u64 = 8 * 1024 * 1024;
    let meta = fs::metadata(path).map_err(|e| format!("image metadata: {e}"))?;
    if !meta.is_file() {
        return Err("image_not_file".into());
    }
    if meta.len() > MAX_IMAGE_BYTES {
        return Err("image_too_large".into());
    }
    let mime = match path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .as_deref()
    {
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("gif") => "image/gif",
        _ => return Err("unsupported_image_type".into()),
    };
    let bytes = fs::read(path).map_err(|e| format!("image read: {e}"))?;
    Ok(format!("data:{mime};base64,{}", BASE64.encode(bytes)))
}

fn reply_response<T: Serialize>(
    stream: &mut TcpStream,
    kind: &str,
    request_id: &str,
    payload: T,
) -> Result<(), (&'static str, String)> {
    let envelope = Envelope::response(kind, request_id, payload)
        .map_err(|e| ("serialization_error", e.to_string()))?;
    reply(stream, &envelope).map_err(|e| ("write_failed", e))
}

fn reply_error(stream: &mut TcpStream, request_id: Option<&str>, code: &str, message: &str) {
    let _ = reply(stream, &Envelope::error(request_id, code, message));
}

fn reply(stream: &mut TcpStream, envelope: &Envelope) -> Result<(), String> {
    envelope.validate()?;
    let line = serde_json::to_string(envelope).map_err(|e| e.to_string())?;
    if line.len() > MAX_FRAME_BYTES {
        return Err("response_too_large".into());
    }
    writeln!(stream, "{line}").map_err(|e| e.to_string())
}

fn secure_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right.iter())
        .fold(0u8, |acc, (a, b)| acc | (a ^ b))
        == 0
}

fn ensure_hud(runtime: &Arc<Runtime>) {
    let mut guard = match runtime.hud_process.lock() {
        Ok(guard) => guard,
        Err(_) => return,
    };
    if let Some(child) = guard.as_mut() {
        if matches!(child.try_wait(), Ok(None)) {
            return;
        }
    }
    let executable = std::env::var_os("KITT_HUD_BIN")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::current_exe().ok().and_then(|path| {
                path.parent().map(|dir| {
                    dir.join(if cfg!(windows) {
                        "kitt-hud.exe"
                    } else {
                        "kitt-hud"
                    })
                })
            })
        });
    if let Some(executable) = executable {
        match Command::new(executable)
            .env("KITT_DAEMON_ADDR", &runtime.config.listen)
            .env("KITT_DAEMON_TOKEN", &runtime.token)
            .spawn()
        {
            Ok(child) => *guard = Some(child),
            Err(error) => eprintln!("HUD spawn: {error}"),
        }
    }
}

struct Paths {
    dir: PathBuf,
    config: PathBuf,
    token: PathBuf,
    memory_db: PathBuf,
}

impl Paths {
    fn load() -> Self {
        let base = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("kitt")
            .join("assistant");
        Self {
            config: base.join("config.json"),
            token: base.join("auth.token"),
            memory_db: base.join("memory.db"),
            dir: base,
        }
    }
}

fn load_config(paths: &Paths) -> Result<Config, String> {
    fs::create_dir_all(&paths.dir).map_err(|e| e.to_string())?;
    if !paths.config.exists() {
        let config = Config::default();
        fs::write(
            &paths.config,
            serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        return Ok(config);
    }
    serde_json::from_str(&fs::read_to_string(&paths.config).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}

fn load_or_create_token(paths: &Paths) -> Result<String, String> {
    fs::create_dir_all(&paths.dir).map_err(|e| e.to_string())?;
    if paths.token.exists() {
        let current = fs::read_to_string(&paths.token).map_err(|e| e.to_string())?;
        let current = current.trim();
        if valid_token(current) {
            set_private_permissions(&paths.token)?;
            return Ok(current.to_string());
        }
    }

    let token = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    fs::write(&paths.token, &token).map_err(|e| e.to_string())?;
    set_private_permissions(&paths.token)?;
    if !valid_token(&token) {
        return Err("generated authentication token is invalid".into());
    }
    Ok(token)
}

fn valid_token(token: &str) -> bool {
    token.len() >= 48 && token.bytes().all(|b| b.is_ascii_hexdigit())
}

#[cfg(unix)]
fn set_private_permissions(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(|e| e.to_string())
}

#[cfg(not(unix))]
fn set_private_permissions(_path: &Path) -> Result<(), String> {
    Ok(())
}

fn validate_loopback(listen: &str) -> Result<(), String> {
    let addr: SocketAddr = listen
        .parse()
        .map_err(|e| format!("invalid listen address: {e}"))?;
    if !addr.ip().is_loopback() {
        return Err("kittd only permits loopback listen addresses".into());
    }
    Ok(())
}

fn validate_provider_locality(config: &Config) -> Result<(), String> {
    if !config.local_provider {
        return Ok(());
    }
    let parsed =
        url::Url::parse(&config.base_url).map_err(|e| format!("invalid provider base_url: {e}"))?;
    let host = parsed.host_str().ok_or("provider base_url has no host")?;
    if host.eq_ignore_ascii_case("localhost") {
        return Ok(());
    }
    if host
        .parse::<std::net::IpAddr>()
        .map(|ip| ip.is_loopback())
        .unwrap_or(false)
    {
        return Ok(());
    }
    Err(
        "local_provider=true requires a loopback provider URL; set local_provider=false for remote endpoints"
            .into(),
    )
}

fn fatal<T: std::fmt::Display>(error: T) -> ! {
    eprintln!("fatal: {error}");
    std::process::exit(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_validation() {
        assert!(validate_loopback("127.0.0.1:41827").is_ok());
        assert!(validate_loopback("[::1]:41827").is_ok());
        assert!(validate_loopback("0.0.0.0:41827").is_err());
    }

    #[test]
    fn legacy_config_without_tts_fields_uses_voice_defaults() {
        let config: Config = serde_json::from_str(
            r#"{
                "listen":"127.0.0.1:41827",
                "base_url":"http://127.0.0.1:11434/v1",
                "model":"",
                "api_key_env":null,
                "local_provider":true,
                "allow_personal_remote":false,
                "hud_ttl_ms":8000
            }"#,
        )
        .unwrap();
        assert!(config.tts_prefer_male);
        assert_eq!(config.tts_rate, -1);
        assert_eq!(config.tts_pitch, -2);
        assert_eq!(config.tts_volume, 95);
    }

    #[test]
    fn token_validation_and_constant_time_compare() {
        let token = "a".repeat(64);
        assert!(valid_token(&token));
        assert!(secure_eq(token.as_bytes(), token.as_bytes()));
        assert!(!secure_eq(token.as_bytes(), b"wrong"));
    }
}
