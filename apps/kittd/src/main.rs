use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use kitt_application::AssistantService;
use kitt_infrastructure::{AssistantMemory, OpenAiCompatibleModel};
use kitt_memory_core::{MemoryKind, MemoryScope, MemoryStore, NewMemory, RecallQuery, Sensitivity};
use kitt_memory_sqlite::SqliteMemoryStore;
use kitt_protocol::{HudEvent, HudState};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Child, Command},
    sync::{Arc, Mutex},
    thread,
};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Config {
    listen: String,
    base_url: String,
    model: String,
    api_key_env: Option<String>,
    local_provider: bool,
    allow_personal_remote: bool,
    hud_ttl_ms: u64,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            listen: "127.0.0.1:41827".into(),
            base_url: "http://127.0.0.1:11434/v1".into(),
            model: std::env::var("KITT_MODEL").unwrap_or_else(|_| "qwen3:4b".into()),
            api_key_env: None,
            local_provider: true,
            allow_personal_remote: false,
            hud_ttl_ms: 8000,
        }
    }
}

struct Runtime {
    token: String,
    service: Arc<AssistantService>,
    memory: Arc<SqliteMemoryStore>,
    hud: HudBroadcaster,
    hud_process: Mutex<Option<Child>>,
    config: Config,
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
    fn subscribe(&self, mut stream: TcpStream) {
        if let Ok(last) = self.last_event.lock() {
            if let Some(line) = last.as_ref() {
                let _ = stream.write_all(line.as_bytes());
            }
        }
        if let Ok(mut c) = self.clients.lock() {
            c.push(stream)
        }
    }
    fn send(&self, event: HudEvent) {
        let line =
            serde_json::to_string(&event).unwrap_or_else(|_| "{\"type\":\"hide\"}".into()) + "\n";
        if let Ok(mut last) = self.last_event.lock() {
            *last = Some(line.clone())
        }
        if let Ok(mut clients) = self.clients.lock() {
            clients.retain_mut(|s| s.write_all(line.as_bytes()).is_ok());
        }
    }
}

fn main() {
    let paths = Paths::load();
    let config = load_config(&paths).unwrap_or_else(fatal);
    validate_loopback(&config.listen).unwrap_or_else(fatal);
    validate_provider_locality(&config).unwrap_or_else(fatal);
    let token = load_or_create_token(&paths).unwrap_or_else(fatal);
    let memory = Arc::new(
        SqliteMemoryStore::open(&paths.memory_db).unwrap_or_else(|e| fatal(e.to_string())),
    );
    let key = config
        .api_key_env
        .as_ref()
        .and_then(|n| std::env::var(n).ok())
        .filter(|s| !s.is_empty());
    let model = Arc::new(
        OpenAiCompatibleModel::new(
            config.base_url.clone(),
            config.model.clone(),
            key,
            config.local_provider,
        )
        .unwrap_or_else(|e| fatal(e.to_string())),
    );
    let mem_adapter = Arc::new(AssistantMemory::new(
        memory.clone(),
        "global".into(),
        config.allow_personal_remote,
    ));
    let service = Arc::new(AssistantService::new(model, mem_adapter));
    let rt = Arc::new(Runtime {
        token,
        service,
        memory,
        hud: HudBroadcaster::new(),
        hud_process: Mutex::new(None),
        config: config.clone(),
    });
    let listener = TcpListener::bind(&config.listen)
        .unwrap_or_else(|e| fatal(format!("bind {}: {e}", config.listen)));
    eprintln!("kittd listening on {}", config.listen);
    for stream in listener.incoming() {
        match stream {
            Ok(s) => {
                let r = rt.clone();
                thread::spawn(move || handle_client(s, r));
            }
            Err(e) => eprintln!("accept: {e}"),
        }
    }
}

fn handle_client(mut stream: TcpStream, rt: Arc<Runtime>) {
    const MAX_REQUEST_BYTES: u64 = 1024 * 1024;
    let peer = stream.try_clone();
    if peer.is_err() {
        return;
    };
    let mut reader = BufReader::new(peer.unwrap()).take(MAX_REQUEST_BYTES + 1);
    let mut bytes = Vec::new();
    if reader.read_until(b'\n', &mut bytes).is_err() {
        return;
    };
    if bytes.len() as u64 > MAX_REQUEST_BYTES {
        reply(&mut stream, json!({"ok":false,"error":"request_too_large"}));
        return;
    }
    let req: Value = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(_) => {
            reply(&mut stream, json!({"ok":false,"error":"invalid_json"}));
            return;
        }
    };
    if req.get("token").and_then(Value::as_str) != Some(rt.token.as_str()) {
        reply(&mut stream, json!({"ok":false,"error":"unauthorized"}));
        return;
    }
    let cmd = req.get("command").and_then(Value::as_str).unwrap_or("");
    if cmd == "subscribe_hud" {
        reply(&mut stream, json!({"ok":true}));
        rt.hud.subscribe(stream);
        return;
    }
    let result = match cmd {
        "ping" => Ok(json!({"status":"ok"})),
        "ask" => ask(&rt, req.get("text").and_then(Value::as_str).unwrap_or("")),
        "remember" => remember(&rt, &req),
        "memory_recall" => memory_recall(&rt, &req),
        "memory_remember" => memory_remember(&rt, &req),
        "memory_forget" => memory_forget(&rt, &req),
        "image" => show_image(&rt, &req),
        _ => Err("unknown_command".to_string()),
    };
    match result {
        Ok(v) => reply(&mut stream, json!({"ok":true,"result":v})),
        Err(e) => reply(&mut stream, json!({"ok":false,"error":e})),
    }
}

fn ask(rt: &Arc<Runtime>, text: &str) -> std::result::Result<Value, String> {
    if text.trim().is_empty() {
        return Err("empty_text".into());
    }
    ensure_hud(rt);
    rt.hud.send(HudEvent::Status {
        state: HudState::Thinking,
        message: Some("K.I.T.T.".into()),
    });
    match rt.service.ask(text) {
        Ok(answer) => {
            rt.hud.send(HudEvent::Text {
                content: answer.clone(),
                ttl_ms: rt.config.hud_ttl_ms,
            });
            Ok(json!({"text":answer}))
        }
        Err(e) => {
            rt.hud.send(HudEvent::Status {
                state: HudState::Error,
                message: Some(e.to_string()),
            });
            Err(e.to_string())
        }
    }
}
fn remember(rt: &Arc<Runtime>, req: &Value) -> std::result::Result<Value, String> {
    let text = req.get("text").and_then(Value::as_str).unwrap_or("");
    rt.service
        .remember(text)
        .map(|id| json!({"id":id}))
        .map_err(|e| e.to_string())
}
fn show_image(rt: &Arc<Runtime>, req: &Value) -> std::result::Result<Value, String> {
    let raw = req
        .get("src")
        .and_then(Value::as_str)
        .ok_or("missing_src")?;
    let src =
        if raw.starts_with("http://") || raw.starts_with("https://") || raw.starts_with("data:") {
            raw.to_string()
        } else {
            image_data_uri(Path::new(raw))?
        };
    ensure_hud(rt);
    rt.hud.send(HudEvent::Image {
        src,
        alt: req.get("alt").and_then(Value::as_str).map(str::to_string),
        ttl_ms: rt.config.hud_ttl_ms,
    });
    Ok(json!({"shown":true}))
}
fn image_data_uri(path: &Path) -> std::result::Result<String, String> {
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
        .and_then(|v| v.to_str())
        .map(|v| v.to_ascii_lowercase())
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

fn memory_recall(rt: &Arc<Runtime>, req: &Value) -> std::result::Result<Value, String> {
    let q = RecallQuery {
        namespace: s(req, "namespace", "agent-cli"),
        workspace_id: s(req, "workspace_id", "default"),
        text: s(req, "query", ""),
        limit: req.get("limit").and_then(Value::as_u64).unwrap_or(8) as usize,
        allow_private: req
            .get("allow_private")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        allow_secret: false,
    };
    rt.memory
        .recall(&q)
        .map(|m| serde_json::to_value(m).unwrap_or(Value::Array(vec![])))
        .map_err(|e| e.to_string())
}
fn memory_remember(rt: &Arc<Runtime>, req: &Value) -> std::result::Result<Value, String> {
    let kind = MemoryKind::from_db(
        req.get("kind")
            .and_then(Value::as_str)
            .unwrap_or("PROJECT_RULE"),
    );
    let sensitivity = Sensitivity::from_db(
        req.get("sensitivity")
            .and_then(Value::as_str)
            .unwrap_or("private"),
    );
    let scope = MemoryScope::from_db(
        req.get("scope")
            .and_then(Value::as_str)
            .unwrap_or("workspace"),
    );
    let m = NewMemory {
        namespace: s(req, "namespace", "agent-cli"),
        workspace_id: s(req, "workspace_id", "default"),
        kind,
        content: s(req, "content", ""),
        sensitivity,
        scope,
        importance: req.get("importance").and_then(Value::as_f64).unwrap_or(0.8) as f32,
        confidence: req.get("confidence").and_then(Value::as_f64).unwrap_or(1.0) as f32,
        pinned: req.get("pinned").and_then(Value::as_bool).unwrap_or(false),
        ttl_seconds: req.get("ttl_seconds").and_then(Value::as_u64),
        metadata_json: "{}".into(),
    };
    rt.memory
        .remember(m)
        .map(|m| json!({"id":m.id}))
        .map_err(|e| e.to_string())
}
fn memory_forget(rt: &Arc<Runtime>, req: &Value) -> std::result::Result<Value, String> {
    let id = req.get("id").and_then(Value::as_str).ok_or("missing_id")?;
    rt.memory
        .forget(id)
        .map(|deleted| json!({"deleted":deleted}))
        .map_err(|e| e.to_string())
}
fn s(req: &Value, key: &str, default: &str) -> String {
    req.get(key)
        .and_then(Value::as_str)
        .unwrap_or(default)
        .to_string()
}

fn ensure_hud(rt: &Arc<Runtime>) {
    let mut guard = match rt.hud_process.lock() {
        Ok(g) => g,
        Err(_) => return,
    };
    if let Some(child) = guard.as_mut() {
        if matches!(child.try_wait(), Ok(None)) {
            return;
        }
    }
    let exe = std::env::var_os("KITT_HUD_BIN")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::current_exe().ok().and_then(|p| {
                p.parent().map(|d| {
                    d.join(if cfg!(windows) {
                        "kitt-hud.exe"
                    } else {
                        "kitt-hud"
                    })
                })
            })
        });
    if let Some(exe) = exe {
        match Command::new(exe)
            .env("KITT_DAEMON_ADDR", &rt.config.listen)
            .env("KITT_DAEMON_TOKEN", &rt.token)
            .spawn()
        {
            Ok(c) => *guard = Some(c),
            Err(e) => eprintln!("HUD spawn: {e}"),
        }
    }
}
fn reply(stream: &mut TcpStream, value: Value) {
    let _ = writeln!(
        stream,
        "{}",
        serde_json::to_string(&value).unwrap_or_else(|_| "{\"ok\":false}".into())
    );
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
fn load_config(paths: &Paths) -> std::result::Result<Config, String> {
    fs::create_dir_all(&paths.dir).map_err(|e| e.to_string())?;
    if !paths.config.exists() {
        let c = Config::default();
        fs::write(&paths.config, serde_json::to_string_pretty(&c).unwrap())
            .map_err(|e| e.to_string())?;
        return Ok(c);
    }
    serde_json::from_str(&fs::read_to_string(&paths.config).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}
fn load_or_create_token(paths: &Paths) -> std::result::Result<String, String> {
    if paths.token.exists() {
        return Ok(fs::read_to_string(&paths.token)
            .map_err(|e| e.to_string())?
            .trim()
            .into());
    }
    let token = Uuid::new_v4().to_string() + &Uuid::new_v4().simple().to_string();
    fs::write(&paths.token, &token).map_err(|e| e.to_string())?;
    set_private_permissions(&paths.token)?;
    Ok(token)
}
#[cfg(unix)]
fn set_private_permissions(path: &Path) -> std::result::Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(|e| e.to_string())
}
#[cfg(not(unix))]
fn set_private_permissions(_path: &Path) -> std::result::Result<(), String> {
    Ok(())
}
fn validate_loopback(listen: &str) -> std::result::Result<(), String> {
    let addr: SocketAddr = listen
        .parse()
        .map_err(|e| format!("invalid listen address: {e}"))?;
    if !addr.ip().is_loopback() {
        return Err("kittd v0.1 only permits loopback listen addresses".into());
    }
    Ok(())
}
fn validate_provider_locality(config: &Config) -> std::result::Result<(), String> {
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
    Err("local_provider=true requires a loopback provider URL; set local_provider=false for remote endpoints".into())
}
fn fatal<T: std::fmt::Display, R>(e: T) -> R {
    eprintln!("fatal: {e}");
    std::process::exit(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_loopback_accepts_127_0_0_1() {
        assert!(validate_loopback("127.0.0.1:41827").is_ok());
        assert!(validate_loopback("127.0.0.2:8080").is_ok());
        assert!(validate_loopback("[::1]:41827").is_ok());
    }

    #[test]
    fn test_validate_loopback_rejects_non_loopback() {
        assert!(validate_loopback("0.0.0.0:41827").is_err());
        assert!(validate_loopback("192.168.1.100:41827").is_err());
        assert!(validate_loopback("8.8.8.8:41827").is_err());
    }

    #[test]
    fn test_validate_provider_locality() {
        let mut cfg = Config {
            local_provider: true,
            base_url: "http://127.0.0.1:11434/v1".into(),
            ..Default::default()
        };
        assert!(validate_provider_locality(&cfg).is_ok());

        cfg.base_url = "http://localhost:11434/v1".into();
        assert!(validate_provider_locality(&cfg).is_ok());

        cfg.base_url = "https://api.openai.com/v1".into();
        assert!(validate_provider_locality(&cfg).is_err());

        // If local_provider is false, remote base_url is accepted:
        cfg.local_provider = false;
        assert!(validate_provider_locality(&cfg).is_ok());
    }
}
