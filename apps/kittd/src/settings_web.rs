//! KITT Control Center local web server.
//!
//! Integration target: `apps/kittd/src/settings_web.rs`.
//! Uses only dependencies already present in kittd (serde_json + uuid) and std.

use serde_json::{Map, Value, json};
use std::{
    collections::HashMap,
    fs::{self, OpenOptions},
    io::{BufRead, BufReader, Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

const DEFAULT_BIND: &str = "127.0.0.1:41828";
const MAX_HEADER_BYTES: usize = 32 * 1024;
const MAX_BODY_BYTES: usize = 256 * 1024;
const MAX_CONNECTIONS: usize = 32;

const INDEX: &str = include_str!("../control-center-web/index.html");
const APP_CSS: &str = include_str!("../control-center-web/app.css");
const APP_JS: &str = include_str!("../control-center-web/app.js");
const CATALOG: &str = include_str!("../control-center-web/catalog.json");

#[derive(Clone)]
struct State {
    bind: SocketAddr,
    config_root: PathBuf,
    overlay_path: PathBuf,
    csrf: Arc<String>,
    catalog: Arc<Value>,
    started_at: Instant,
    reverse_proxy: Arc<Mutex<ManagedReverseProxy>>,
}

#[derive(Default)]
struct ManagedReverseProxy {
    child: Option<Child>,
    preset: Option<String>,
    target_url: Option<String>,
    started_at: Option<Instant>,
    last_exit: Option<String>,
}

#[derive(Debug)]
struct Request {
    method: String,
    path: String,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

pub fn start(config_root: &Path) -> Result<(), String> {
    let bind = std::env::var("KITT_CONTROL_CENTER_ADDR")
        .unwrap_or_else(|_| DEFAULT_BIND.to_string())
        .parse::<SocketAddr>()
        .map_err(|e| format!("invalid KITT_CONTROL_CENTER_ADDR: {e}"))?;
    if !bind.ip().is_loopback() {
        return Err("KITT Control Center only permits loopback bind addresses".into());
    }

    let catalog: Value = serde_json::from_str(CATALOG)
        .map_err(|e| format!("embedded Control Center catalog is invalid: {e}"))?;
    validate_catalog(&catalog)?;

    let dir = config_root.join("control-center");
    ensure_private_dir(&dir)?;
    let overlay_path = dir.join("overrides.json");
    ensure_overlay(&overlay_path)?;

    let state = State {
        bind,
        config_root: config_root.to_path_buf(),
        overlay_path,
        csrf: Arc::new(format!(
            "{}{}",
            Uuid::new_v4().simple(),
            Uuid::new_v4().simple()
        )),
        catalog: Arc::new(catalog),
        started_at: Instant::now(),
        reverse_proxy: Arc::new(Mutex::new(ManagedReverseProxy::default())),
    };
    let listener =
        TcpListener::bind(bind).map_err(|e| format!("Control Center bind {bind}: {e}"))?;
    listener
        .set_nonblocking(false)
        .map_err(|e| format!("Control Center listener: {e}"))?;

    thread::Builder::new()
        .name("kitt-control-center".into())
        .spawn(move || serve(listener, state))
        .map_err(|e| format!("spawn Control Center: {e}"))?;
    eprintln!("KITT Control Center: http://{bind}/");
    Ok(())
}

fn serve(listener: TcpListener, state: State) {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static ACTIVE: AtomicUsize = AtomicUsize::new(0);
    for incoming in listener.incoming() {
        let Ok(stream) = incoming else { continue };
        let Ok(peer) = stream.peer_addr() else {
            continue;
        };
        if !peer.ip().is_loopback() {
            let _ = stream.shutdown(std::net::Shutdown::Both);
            continue;
        }
        if ACTIVE.fetch_add(1, Ordering::AcqRel) >= MAX_CONNECTIONS {
            ACTIVE.fetch_sub(1, Ordering::AcqRel);
            let _ = stream.shutdown(std::net::Shutdown::Both);
            continue;
        }
        let state = state.clone();
        thread::spawn(move || {
            let _guard = ActiveGuard(&ACTIVE);
            if let Err(error) = handle(stream, &state) {
                eprintln!("Control Center request: {error}");
            }
        });
    }
}

struct ActiveGuard<'a>(&'a std::sync::atomic::AtomicUsize);
impl Drop for ActiveGuard<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
    }
}

fn handle(mut stream: TcpStream, state: &State) -> Result<(), String> {
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok();
    stream.set_write_timeout(Some(Duration::from_secs(5))).ok();
    let request = read_request(&stream)?;

    if !valid_host(
        request.headers.get("host").map(String::as_str),
        state.bind.port(),
    ) {
        return write_json(
            &mut stream,
            400,
            json!({"error":"invalid Host header"}),
            None,
        );
    }

    if matches!(request.method.as_str(), "POST" | "PUT" | "DELETE" | "PATCH") {
        if !same_origin(
            request.headers.get("origin").map(String::as_str),
            state.bind.port(),
        ) {
            return write_json(
                &mut stream,
                403,
                json!({"error":"cross-origin request blocked"}),
                None,
            );
        }
        if request.headers.get("x-kitt-csrf").map(String::as_str) != Some(state.csrf.as_str()) {
            return write_json(
                &mut stream,
                403,
                json!({"error":"invalid CSRF token"}),
                None,
            );
        }
    }

    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/") | ("GET", "/index.html") => {
            write_asset(&mut stream, "text/html; charset=utf-8", INDEX)
        }
        ("GET", "/app.css") => write_asset(&mut stream, "text/css; charset=utf-8", APP_CSS),
        ("GET", "/app.js") => write_asset(&mut stream, "text/javascript; charset=utf-8", APP_JS),
        ("GET", "/api/v1/health") => write_json(
            &mut stream,
            200,
            json!({"status":"ok","bind":state.bind.to_string(),"csrf_token":state.csrf.as_str()}),
            None,
        ),
        ("GET", "/api/v1/catalog") => write_json(&mut stream, 200, (*state.catalog).clone(), None),
        ("GET", "/api/v1/config") => {
            let overlay = read_overlay(&state.overlay_path)?;
            write_json(&mut stream, 200, snapshot(&overlay), None)
        }
        ("POST", "/api/v1/validate") => {
            let result = (|| -> Result<Value, String> {
                let payload = parse_json_body(&request.body)?;
                let overlay = read_overlay(&state.overlay_path)?;
                let (changes, _) = validate_change_request(&payload, &overlay, &state.catalog)?;
                Ok(json!({"status":"valid","diff":changes}))
            })();
            match result {
                Ok(value) => write_json(&mut stream, 200, value, None),
                Err(error) => write_json(
                    &mut stream,
                    if error.starts_with("revision conflict") {
                        409
                    } else {
                        400
                    },
                    json!({"error":error}),
                    None,
                ),
            }
        }
        ("PUT", "/api/v1/config") => {
            let result = (|| -> Result<Value, String> {
                let payload = parse_json_body(&request.body)?;
                let mut overlay = read_overlay(&state.overlay_path)?;
                let (changes, restart_required) =
                    validate_change_request(&payload, &overlay, &state.catalog)?;
                merge_changes(&mut overlay, &changes)?;
                bump_revision(&mut overlay)?;
                write_overlay_atomic(&state.overlay_path, &overlay)?;
                Ok(
                    json!({"status":"applied","snapshot":snapshot(&overlay),"restart_required":restart_required}),
                )
            })();
            match result {
                Ok(value) => write_json(&mut stream, 200, value, None),
                Err(error) => write_json(
                    &mut stream,
                    if error.starts_with("revision conflict") {
                        409
                    } else {
                        400
                    },
                    json!({"error":error}),
                    None,
                ),
            }
        }
        ("POST", "/api/v1/models/discover") => {
            let result = (|| -> Result<Value, String> {
                let payload = parse_json_body(&request.body)?;
                let base_url = payload
                    .get("base_url")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .trim();
                // A browser request must never choose an environment
                // variable for kittd to read and forward to an arbitrary URL.
                let models = kitt_infrastructure::discover_models_from_url(base_url, None)
                    .map_err(|e| e.to_string())?;
                Ok(json!({ "status": "ok", "models": models }))
            })();
            match result {
                Ok(value) => write_json(&mut stream, 200, value, None),
                Err(error) => write_json(
                    &mut stream,
                    400,
                    json!({"error": error, "models": []}),
                    None,
                ),
            }
        }
        ("GET", "/api/v1/service/status") => {
            write_json(&mut stream, 200, get_service_status(state), None)
        }
        ("GET", "/api/v1/service/logs") => write_json(&mut stream, 200, get_service_logs(), None),
        ("GET", "/api/v1/reverse-proxy/status") => {
            write_json(&mut stream, 200, reverse_proxy_status(state), None)
        }
        ("POST", "/api/v1/reverse-proxy/start") => {
            let result = parse_json_body(&request.body)
                .and_then(|payload| start_reverse_proxy(state, &payload));
            match result {
                Ok(value) => write_json(&mut stream, 200, value, None),
                Err(error) => write_json(&mut stream, 400, json!({"error":error}), None),
            }
        }
        ("POST", "/api/v1/reverse-proxy/stop") => match stop_reverse_proxy(state) {
            Ok(value) => write_json(&mut stream, 200, value, None),
            Err(error) => write_json(&mut stream, 400, json!({"error":error}), None),
        },
        ("POST", "/api/v1/agent-gateway/install") => match agent_gateway_action(state, "install") {
            Ok(value) => write_json(&mut stream, 200, value, None),
            Err(error) => write_json(&mut stream, 400, json!({"error":error}), None),
        },
        ("POST", "/api/v1/agent-gateway/uninstall") => match agent_gateway_action(state, "uninstall") {
            Ok(value) => write_json(&mut stream, 200, value, None),
            Err(error) => write_json(&mut stream, 400, json!({"error":error}), None),
        },
        ("POST", "/api/v1/agent-gateway/verify") => match agent_gateway_action(state, "verify") {
            Ok(value) => write_json(&mut stream, 200, value, None),
            Err(error) => write_json(&mut stream, 400, json!({"error":error}), None),
        },
        ("POST", "/api/v1/service/restart") => {
            write_json(&mut stream, 200, handle_service_restart(), None)
        }
        ("POST", "/api/v1/service/ping") => {
            let ts = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            write_json(
                &mut stream,
                200,
                json!({"status": "ok", "pong": true, "timestamp_ms": ts}),
                None,
            )
        }
        _ => write_json(&mut stream, 404, json!({"error":"not found"}), None),
    }
}

fn read_request(stream: &TcpStream) -> Result<Request, String> {
    let mut reader = BufReader::new(stream.try_clone().map_err(|e| e.to_string())?);
    let mut first = String::new();
    reader.read_line(&mut first).map_err(|e| e.to_string())?;
    let mut parts = first.split_whitespace();
    let method = parts.next().ok_or("missing method")?.to_string();
    let raw_path = parts.next().ok_or("missing path")?;
    if parts.next().is_none() {
        return Err("missing HTTP version".into());
    }
    let path = raw_path.split('?').next().unwrap_or("/").to_string();

    let mut headers = HashMap::new();
    let mut read_bytes = first.len();
    loop {
        let mut line = String::new();
        let n = reader.read_line(&mut line).map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("connection closed during headers".into());
        }
        read_bytes += n;
        if read_bytes > MAX_HEADER_BYTES {
            return Err("headers too large".into());
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
        let Some((name, value)) = line.split_once(':') else {
            return Err("invalid header".into());
        };
        headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
    }
    let length = headers
        .get("content-length")
        .map(|v| v.parse::<usize>())
        .transpose()
        .map_err(|_| "invalid Content-Length")?
        .unwrap_or(0);
    if length > MAX_BODY_BYTES {
        return Err("request body too large".into());
    }
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body).map_err(|e| e.to_string())?;
    Ok(Request {
        method,
        path,
        headers,
        body,
    })
}

fn parse_json_body(bytes: &[u8]) -> Result<Value, String> {
    if bytes.is_empty() {
        return Err("JSON body required".into());
    }
    serde_json::from_slice(bytes).map_err(|e| format!("invalid JSON: {e}"))
}

fn valid_host(host: Option<&str>, port: u16) -> bool {
    let Some(host) = host else { return false };
    let host = host.trim().to_ascii_lowercase();
    host == format!("127.0.0.1:{port}")
        || host == format!("localhost:{port}")
        || host == format!("[::1]:{port}")
}

fn same_origin(origin: Option<&str>, port: u16) -> bool {
    let Some(origin) = origin else { return false };
    [
        format!("http://127.0.0.1:{port}"),
        format!("http://localhost:{port}"),
        format!("http://[::1]:{port}"),
    ]
    .iter()
    .any(|allowed| origin.eq_ignore_ascii_case(allowed))
}

fn validate_change_request(
    payload: &Value,
    overlay: &Value,
    catalog: &Value,
) -> Result<(Value, Vec<String>), String> {
    let expected = payload
        .get("expected_revision")
        .and_then(Value::as_u64)
        .ok_or("expected_revision is required")?;
    let revision = overlay.get("revision").and_then(Value::as_u64).unwrap_or(0);
    if expected != revision {
        return Err(format!(
            "revision conflict: expected {expected}, current {revision}"
        ));
    }
    let changes = payload
        .get("changes")
        .and_then(Value::as_object)
        .ok_or("changes must be an object")?;
    let sections = catalog
        .get("sections")
        .and_then(Value::as_array)
        .ok_or("catalog sections missing")?;
    let mut restart = Vec::new();
    for (section_id, section_changes) in changes {
        let section = sections
            .iter()
            .find(|s| s.get("id").and_then(Value::as_str) == Some(section_id.as_str()))
            .ok_or_else(|| format!("unknown settings section {section_id}"))?;
        let fields = section
            .get("fields")
            .and_then(Value::as_array)
            .ok_or("catalog fields missing")?;
        let object = section_changes
            .as_object()
            .ok_or_else(|| format!("{section_id} changes must be an object"))?;
        for (field_key, value) in object {
            let field = fields
                .iter()
                .find(|f| f.get("key").and_then(Value::as_str) == Some(field_key.as_str()))
                .ok_or_else(|| format!("unknown setting {section_id}.{field_key}"))?;
            validate_value(field, value).map_err(|e| format!("{section_id}.{field_key}: {e}"))?;
            if field
                .get("apply_mode")
                .and_then(Value::as_str)
                .is_some_and(|m| m != "live")
            {
                restart.push(section_id.clone());
            }
        }
    }
    restart.sort();
    restart.dedup();
    Ok((Value::Object(changes.clone()), restart))
}

fn validate_value(field: &Value, value: &Value) -> Result<(), String> {
    let kind = field
        .get("type")
        .and_then(Value::as_str)
        .ok_or("field type missing")?;
    let type_ok = match kind {
        "boolean" => value.is_boolean(),
        "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
        "number" => value.as_f64().is_some(),
        "string" | "path" | "url" | "secret_ref" => value.is_string(),
        "string_list" => value
            .as_array()
            .is_some_and(|items| items.iter().all(Value::is_string)),
        "enum" => value.as_str().is_some(),
        _ => false,
    };
    if !type_ok {
        return Err(format!("expected {kind}"));
    }
    if let Some(n) = value.as_f64() {
        if field
            .get("minimum")
            .and_then(Value::as_f64)
            .is_some_and(|min| n < min)
        {
            return Err("below minimum".into());
        }
        if field
            .get("maximum")
            .and_then(Value::as_f64)
            .is_some_and(|max| n > max)
        {
            return Err("above maximum".into());
        }
    }
    if kind == "enum" {
        let v = value.as_str().unwrap_or_default();
        if !field
            .get("options")
            .and_then(Value::as_array)
            .is_some_and(|options| options.iter().any(|o| o.as_str() == Some(v)))
        {
            return Err("value is not in enum options".into());
        }
    }
    if kind == "url" {
        let v = value.as_str().unwrap_or_default();
        if !(v.starts_with("http://") || v.starts_with("https://")) {
            return Err("URL must use http:// or https://".into());
        }
    }
    if kind == "secret_ref" {
        let v = value.as_str().unwrap_or_default();
        if !v.is_empty() && !v.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
            return Err("secret_ref must be an environment/credential identifier".into());
        }
    }
    Ok(())
}

fn merge_changes(overlay: &mut Value, changes: &Value) -> Result<(), String> {
    let root = overlay.as_object_mut().ok_or("overlay root invalid")?;
    let components = root
        .entry("components")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .ok_or("overlay components invalid")?;
    for (section, values) in changes.as_object().ok_or("changes invalid")? {
        let target = components
            .entry(section.clone())
            .or_insert_with(|| Value::Object(Map::new()))
            .as_object_mut()
            .ok_or("section override invalid")?;
        for (key, value) in values.as_object().ok_or("section changes invalid")? {
            target.insert(key.clone(), value.clone());
        }
    }
    Ok(())
}

fn snapshot(overlay: &Value) -> Value {
    json!({
        "schema_version": 1,
        "revision": overlay.get("revision").and_then(Value::as_u64).unwrap_or(0),
        "values": overlay.get("components").cloned().unwrap_or_else(|| json!({}))
    })
}

fn bump_revision(overlay: &mut Value) -> Result<(), String> {
    let object = overlay.as_object_mut().ok_or("overlay invalid")?;
    let revision = object
        .get("revision")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        .saturating_add(1);
    object.insert("revision".into(), json!(revision));
    object.insert("updated_at_ms".into(), json!(now_ms()));
    Ok(())
}

fn ensure_overlay(path: &Path) -> Result<(), String> {
    if path.exists() {
        let _ = read_overlay(path)?;
        return Ok(());
    }
    let initial = json!({"schema_version":1,"revision":0,"updated_at_ms":now_ms(),"components":{}});
    write_overlay_atomic(path, &initial)
}
fn read_overlay(path: &Path) -> Result<Value, String> {
    let value: Value =
        serde_json::from_slice(&fs::read(path).map_err(|e| format!("read overlay: {e}"))?)
            .map_err(|e| format!("parse overlay: {e}"))?;
    if value.get("schema_version").and_then(Value::as_u64) != Some(1) {
        return Err("unsupported overlay schema version".into());
    }
    if !value.get("components").is_some_and(Value::is_object) {
        return Err("overlay components must be object".into());
    }
    Ok(value)
}
fn write_overlay_atomic(path: &Path, value: &Value) -> Result<(), String> {
    let parent = path.parent().ok_or("overlay has no parent")?;
    ensure_private_dir(parent)?;
    let temp = parent.join(format!(".overrides-{}.tmp", Uuid::new_v4().simple()));
    let bytes = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    write_private(&temp, &bytes)?;
    fs::rename(&temp, path).map_err(|e| format!("replace overlay: {e}"))?;
    set_private_file(path)?;
    Ok(())
}

fn validate_catalog(catalog: &Value) -> Result<(), String> {
    if catalog.get("schema_version").and_then(Value::as_u64) != Some(1) {
        return Err("catalog schema_version must be 1".into());
    }
    let sections = catalog
        .get("sections")
        .and_then(Value::as_array)
        .ok_or("catalog sections missing")?;
    let mut seen = std::collections::HashSet::new();
    for section in sections {
        let id = section
            .get("id")
            .and_then(Value::as_str)
            .ok_or("section id missing")?;
        if !seen.insert(id) {
            return Err(format!("duplicate section id: {id}"));
        }
        if !section.get("fields").is_some_and(Value::is_array) {
            return Err(format!("{id}.fields missing"));
        }
    }
    Ok(())
}

fn write_asset(stream: &mut TcpStream, content_type: &str, body: &str) -> Result<(), String> {
    write_response(stream, 200, content_type, body.as_bytes(), None)
}
fn write_json(
    stream: &mut TcpStream,
    status: u16,
    value: Value,
    extra: Option<&[(&str, &str)]>,
) -> Result<(), String> {
    let body = serde_json::to_vec(&value).map_err(|e| e.to_string())?;
    write_response(
        stream,
        status,
        "application/json; charset=utf-8",
        &body,
        extra,
    )
}
fn write_response(
    stream: &mut TcpStream,
    status: u16,
    content_type: &str,
    body: &[u8],
    extra: Option<&[(&str, &str)]>,
) -> Result<(), String> {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        409 => "Conflict",
        _ => "Error",
    };
    let mut headers = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nX-Frame-Options: DENY\r\nReferrer-Policy: no-referrer\r\nCross-Origin-Opener-Policy: same-origin\r\nCross-Origin-Resource-Policy: same-origin\r\nPermissions-Policy: camera=(), microphone=(), geolocation=()\r\nContent-Security-Policy: default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data:; connect-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'; form-action 'self'\r\n",
        body.len()
    );
    if let Some(extra) = extra {
        for (name, value) in extra {
            headers.push_str(&format!("{name}: {value}\r\n"));
        }
    }
    headers.push_str("\r\n");
    stream
        .write_all(headers.as_bytes())
        .map_err(|e| e.to_string())?;
    stream.write_all(body).map_err(|e| e.to_string())?;
    stream.flush().map_err(|e| e.to_string())
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}

#[cfg(unix)]
fn ensure_private_dir(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    fs::create_dir_all(path).map_err(|e| e.to_string())?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())
}
#[cfg(not(unix))]
fn ensure_private_dir(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path).map_err(|e| e.to_string())
}

#[cfg(unix)]
fn write_private(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(|e| e.to_string())?;
    file.write_all(bytes).map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())
}
#[cfg(not(unix))]
fn write_private(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    file.write_all(bytes).map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())
}
#[cfg(unix)]
fn set_private_file(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(|e| e.to_string())
}
#[cfg(not(unix))]
fn set_private_file(_path: &Path) -> Result<(), String> {
    Ok(())
}


fn section_values(state: &State, section: &str) -> Map<String, Value> {
    read_overlay(&state.overlay_path)
        .ok()
        .and_then(|overlay| {
            overlay
                .get("components")
                .and_then(Value::as_object)
                .and_then(|components| components.get(section))
                .and_then(Value::as_object)
                .cloned()
        })
        .unwrap_or_default()
}

fn agent_gateway_action(state: &State, action: &str) -> Result<Value, String> {
    if !matches!(action, "install" | "uninstall" | "verify") {
        return Err("unsupported Agent Gateway action".into());
    }

    let executable = std::env::var("KITT_AGENT_GATEWAY_BIN")
        .unwrap_or_else(|_| "kitt-agent-gateway".into());
    let values = section_values(state, "agent_gateway.runtime");
    let (_, _, api_url, _) = reverse_proxy_endpoint(state);
    let openai_model = values
        .get("openai_model")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("chatgpt-web");
    let anthropic_model = values
        .get("anthropic_model")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("claude-web");

    let mut command = Command::new(&executable);
    match action {
        "install" => {
            command
                .args([
                    "jetbrains",
                    "install",
                    "--base-url",
                    &api_url,
                    "--openai-model",
                    openai_model,
                    "--anthropic-model",
                    anthropic_model,
                ]);
            if values.get("opencode_enabled").and_then(Value::as_bool) == Some(true) {
                command.arg("--with-opencode");
            }
            if let Some(path) = values
                .get("acp_path")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
            {
                command.args(["--path", path]);
            }
        }
        "uninstall" => {
            command.args(["jetbrains", "uninstall"]);
            if let Some(path) = values
                .get("acp_path")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
            {
                command.args(["--path", path]);
            }
        }
        "verify" => {
            command.args(["verify", "--base-url", &api_url]);
        }
        _ => unreachable!(),
    }

    let output = command
        .stdin(Stdio::null())
        .stderr(Stdio::piped())
        .stdout(Stdio::piped())
        .output()
        .map_err(|error| {
            format!(
                "não foi possível executar {executable}: {error}. Instale o kitt-agent-gateway ou configure KITT_AGENT_GATEWAY_BIN"
            )
        })?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() {
        return Err(format!(
            "Agent Gateway {action} falhou: {}",
            stderr.trim().is_empty().then_some(stdout.trim()).unwrap_or(stderr.trim())
        ));
    }

    let parsed = serde_json::from_str::<Value>(stdout.trim()).unwrap_or_else(|_| {
        json!({"status":"ok","output":stdout.trim()})
    });
    Ok(json!({
        "status": "ok",
        "action": action,
        "result": parsed,
        "message": match action {
            "install" => "Entradas KITT instaladas em acp.json. Reinicie/reabra a lista de agentes do IDE se necessário.",
            "uninstall" => "Entradas KITT removidas de acp.json.",
            "verify" => "KITT Agent Gateway e Reverse Proxy verificados.",
            _ => unreachable!()
        }
    }))
}

fn valid_reverse_proxy_preset(value: &str) -> bool {
    matches!(
        value,
        "chatgpt" | "claude" | "gemini" | "kimi" | "deepseek" | "custom"
    )
}

fn validate_reverse_proxy_target(value: &str) -> Result<String, String> {
    let parsed =
        url::Url::parse(value.trim()).map_err(|error| format!("URL custom inválida: {error}"))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("URL custom deve usar http:// ou https://".into());
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("URL custom não pode conter credenciais embutidas".into());
    }
    Ok(parsed.to_string())
}

fn reverse_proxy_runtime_values(state: &State) -> Map<String, Value> {
    read_overlay(&state.overlay_path)
        .ok()
        .and_then(|overlay| {
            overlay
                .get("components")
                .and_then(Value::as_object)
                .and_then(|components| components.get("reverse_proxy.runtime"))
                .and_then(Value::as_object)
                .cloned()
        })
        .unwrap_or_default()
}

fn reverse_proxy_endpoint(state: &State) -> (String, u16, String, Option<SocketAddr>) {
    let values = reverse_proxy_runtime_values(state);
    let host = values
        .get("host")
        .and_then(Value::as_str)
        .unwrap_or("127.0.0.1")
        .trim()
        .to_ascii_lowercase();
    let port = values
        .get("port")
        .and_then(Value::as_u64)
        .and_then(|value| u16::try_from(value).ok())
        .filter(|value| *value > 0)
        .unwrap_or(3000);

    let (display_host, probe) = match host.as_str() {
        "127.0.0.1" | "localhost" | "0.0.0.0" => (
            "127.0.0.1".to_string(),
            Some(SocketAddr::from(([127, 0, 0, 1], port))),
        ),
        "::1" => (
            "[::1]".to_string(),
            Some(SocketAddr::from(([0, 0, 0, 0, 0, 0, 0, 1], port))),
        ),
        "::" => (
            "127.0.0.1".to_string(),
            Some(SocketAddr::from(([127, 0, 0, 1], port))),
        ),
        _ => (host.clone(), None),
    };
    let api_url = format!("http://{display_host}:{port}");
    (host, port, api_url, probe)
}

fn reverse_proxy_status(state: &State) -> Value {
    let (_, _, api_url, probe) = reverse_proxy_endpoint(state);
    let api_online = probe.is_some_and(|address| {
        TcpStream::connect_timeout(&address, Duration::from_millis(150)).is_ok()
    });

    let (managed, pid, preset, target_url, uptime_seconds, last_exit) =
        match state.reverse_proxy.lock() {
            Ok(mut manager) => {
                let exit_status = manager
                    .child
                    .as_mut()
                    .and_then(|child| child.try_wait().ok().flatten());
                if let Some(status) = exit_status {
                    manager.child = None;
                    manager.last_exit = Some(status.to_string());
                    manager.started_at = None;
                }
                (
                    manager.child.is_some(),
                    manager.child.as_ref().map(Child::id),
                    manager.preset.clone(),
                    manager.target_url.clone(),
                    manager
                        .started_at
                        .map(|started| started.elapsed().as_secs()),
                    manager.last_exit.clone(),
                )
            }
            Err(_) => (
                false,
                None,
                None,
                None,
                None,
                Some("manager lock poisoned".into()),
            ),
        };

    let phase = if managed && api_online {
        "ready"
    } else if managed {
        "starting"
    } else if api_online {
        "external"
    } else {
        "stopped"
    };

    json!({
        "status": "ok",
        "phase": phase,
        "managed": managed,
        "api_online": api_online,
        "pid": pid,
        "preset": preset,
        "target_url": target_url,
        "uptime_seconds": uptime_seconds,
        "last_exit": last_exit,
        "api_url": api_url,
        "launcher": std::env::var("KITT_REVERSE_PROXY_BIN")
            .unwrap_or_else(|_| "kitt-reverse-proxy".into())
    })
}

fn start_reverse_proxy(state: &State, payload: &Value) -> Result<Value, String> {
    let status = reverse_proxy_status(state);
    if status.get("managed").and_then(Value::as_bool) == Some(true) {
        return Err("Reverse Proxy já está sendo gerenciado pelo Control Center".into());
    }
    if status.get("api_online").and_then(Value::as_bool) == Some(true) {
        return Err(
            "Já existe um Reverse Proxy respondendo nessa porta. Pare a instância externa antes de iniciar outra."
                .into(),
        );
    }

    let preset = payload
        .get("preset")
        .and_then(Value::as_str)
        .unwrap_or("chatgpt")
        .trim()
        .to_ascii_lowercase();
    if !valid_reverse_proxy_preset(&preset) {
        return Err(format!("preset inválido: {preset}"));
    }

    let launch_target = if preset == "custom" {
        let value = payload
            .get("target_url")
            .and_then(Value::as_str)
            .ok_or("target_url é obrigatório para preset custom")?;
        validate_reverse_proxy_target(value)?
    } else {
        preset.clone()
    };

    let runtime = reverse_proxy_runtime_values(state);
    let headed = payload
        .get("headed")
        .and_then(Value::as_bool)
        .or_else(|| runtime.get("headed").and_then(Value::as_bool))
        .unwrap_or(true);

    let launcher =
        std::env::var("KITT_REVERSE_PROXY_BIN").unwrap_or_else(|_| "kitt-reverse-proxy".into());
    let mut command = Command::new(&launcher);
    command
        .arg("start")
        .arg(&launch_target)
        .arg(if headed { "--headed" } else { "--headless" })
        .arg("--parent-stdin-lifecycle")
        .env("KITT_CONTROL_CENTER_CONFIG", &state.overlay_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    let mut child = command.spawn().map_err(|error| {
        format!(
            "não foi possível iniciar {launcher}: {error}. Instale/linke o CLI ou configure KITT_REVERSE_PROXY_BIN"
        )
    })?;
    thread::sleep(Duration::from_millis(120));
    if let Some(exit) = child
        .try_wait()
        .map_err(|error| format!("verificar Reverse Proxy iniciado: {error}"))?
    {
        return Err(format!("Reverse Proxy encerrou durante startup: {exit}"));
    }

    let pid = child.id();
    let mut manager = state
        .reverse_proxy
        .lock()
        .map_err(|_| "Reverse Proxy manager lock poisoned".to_string())?;
    manager.child = Some(child);
    manager.preset = Some(preset.clone());
    manager.target_url = (preset == "custom").then_some(launch_target);
    manager.started_at = Some(Instant::now());
    manager.last_exit = None;
    drop(manager);

    let mut response = reverse_proxy_status(state);
    response["message"] = json!(
        "Reverse Proxy iniciado. O Chromium abrirá a sessão; faça login ou resolva desafios manualmente quando necessário."
    );
    response["pid"] = json!(pid);
    Ok(response)
}

fn stop_reverse_proxy(state: &State) -> Result<Value, String> {
    let mut child = {
        let mut manager = state
            .reverse_proxy
            .lock()
            .map_err(|_| "Reverse Proxy manager lock poisoned".to_string())?;
        manager
            .child
            .take()
            .ok_or("Não há Reverse Proxy gerenciado pelo Control Center em execução")?
    };

    // Closing stdin asks the CLI to perform its graceful shutdown path.
    child.stdin.take();
    let mut exit = None;
    for _ in 0..20 {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("verificar shutdown do Reverse Proxy: {error}"))?
        {
            exit = Some(status);
            break;
        }
        thread::sleep(Duration::from_millis(100));
    }
    if exit.is_none() {
        child
            .kill()
            .map_err(|error| format!("forçar shutdown do Reverse Proxy: {error}"))?;
        exit = Some(
            child
                .wait()
                .map_err(|error| format!("aguardar Reverse Proxy: {error}"))?,
        );
    }

    if let Ok(mut manager) = state.reverse_proxy.lock() {
        manager.last_exit = exit.map(|status| status.to_string());
        manager.started_at = None;
        manager.preset = None;
        manager.target_url = None;
    }

    let mut response = reverse_proxy_status(state);
    response["message"] = json!("Reverse Proxy e sessão Chromium encerrados.");
    Ok(response)
}

fn get_service_status(state: &State) -> Value {
    let assistant_dir = if state.config_root.join("assistant").exists() {
        state.config_root.join("assistant")
    } else {
        state.config_root.clone()
    };

    let pid = std::process::id();
    let uptime_secs = state.started_at.elapsed().as_secs();

    let config_val = fs::read_to_string(assistant_dir.join("config.json"))
        .ok()
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .unwrap_or_else(|| json!({}));

    let voice_val = fs::read_to_string(assistant_dir.join("voice.json"))
        .ok()
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .unwrap_or_else(|| json!({}));

    let models_val = fs::read_to_string(assistant_dir.join("models.json"))
        .ok()
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .unwrap_or_else(|| json!({}));

    let mem_path = assistant_dir.join("memory.db");
    let mem_exists = mem_path.exists();
    let mem_size = mem_path.metadata().map(|m| m.len()).unwrap_or(0);

    let stt_probe = TcpStream::connect_timeout(
        &SocketAddr::from(([127, 0, 0, 1], 8000)),
        Duration::from_millis(150),
    )
    .is_ok();

    let wakeword_rel = voice_val
        .get("wakeword_model_path")
        .and_then(Value::as_str)
        .unwrap_or("wakewords/kitt.rpw");
    let wakeword_exists = assistant_dir.join(wakeword_rel).exists();

    json!({
        "status": "ok",
        "daemon": {
            "pid": pid,
            "uptime_seconds": uptime_secs,
            "bind": state.bind.to_string(),
            "listen": config_val.get("listen").and_then(Value::as_str).unwrap_or("127.0.0.1:41827"),
            "version": env!("CARGO_PKG_VERSION"),
            "active": true
        },
        "voice": {
            "enabled": voice_val.get("enabled").and_then(Value::as_bool).unwrap_or(true),
            "locale": voice_val.get("locale").and_then(Value::as_str).unwrap_or("pt-BR"),
            "activation_mode": voice_val.get("activation_mode").and_then(Value::as_str).unwrap_or("auto"),
            "stt_worker_model": voice_val.get("stt_worker_model").and_then(Value::as_str).unwrap_or(""),
            "wake_phrases": voice_val.get("wake_phrases").cloned().unwrap_or_else(|| json!([])),
            "wakeword_model_path": wakeword_rel,
            "wakeword_model_exists": wakeword_exists,
            "stt_worker_online": stt_probe
        },
        "models": {
            "base_url": config_val.get("base_url").and_then(Value::as_str).unwrap_or(""),
            "model": config_val.get("model").and_then(Value::as_str).unwrap_or(""),
            "fast_model": models_val.get("fast").and_then(|f| f.get("model")).and_then(Value::as_str).unwrap_or(""),
            "heavy_model": models_val.get("heavy").and_then(|h| h.get("model")).and_then(Value::as_str).unwrap_or("")
        },
        "memory": {
            "exists": mem_exists,
            "size_bytes": mem_size
        }
    })
}

fn get_service_logs() -> Value {
    let logs = match std::process::Command::new("journalctl")
        .args([
            "--user",
            "-u",
            "kitt-assistant.service",
            "-n",
            "60",
            "--no-pager",
        ])
        .output()
    {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).to_string(),
        Ok(out) => {
            let err = String::from_utf8_lossy(&out.stderr);
            if err.trim().is_empty() {
                String::from_utf8_lossy(&out.stdout).to_string()
            } else {
                format!("(journalctl: {})", err.trim())
            }
        }
        Err(e) => format!("(journalctl não disponível: {e})"),
    };
    json!({"status":"ok","source":"journalctl","logs":logs})
}

fn handle_service_restart() -> Value {
    let out = std::process::Command::new("systemctl")
        .args(["--user", "restart", "kitt-assistant.service"])
        .output();
    match out {
        Ok(output) if output.status.success() => {
            json!({"status": "ok", "message": "Comando de reinício enviado com sucesso para kitt-assistant.service."})
        }
        Ok(output) => {
            let err = String::from_utf8_lossy(&output.stderr);
            json!({"status": "error", "message": format!("Erro ao reiniciar: {}", err.trim())})
        }
        Err(e) => {
            json!({"status": "error", "message": format!("Falha ao invocar systemctl: {e}")})
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn host_is_loopback_only() {
        assert!(valid_host(Some("127.0.0.1:41828"), 41828));
        assert!(valid_host(Some("localhost:41828"), 41828));
        assert!(!valid_host(Some("192.168.1.2:41828"), 41828));
    }
    #[test]
    fn rejects_unknown_setting() {
        let catalog: Value = serde_json::from_str(CATALOG).unwrap();
        let overlay = json!({"revision":0,"components":{}});
        let payload = json!({"expected_revision":0,"changes":{"assistant.core":{"not_real":true}}});
        assert!(validate_change_request(&payload, &overlay, &catalog).is_err());
    }
    #[test]
    fn get_service_status_omits_logs() {
        let state = Arc::new(State {
            bind: SocketAddr::from(([127, 0, 0, 1], 41828)),
            config_root: PathBuf::from("/tmp"),
            overlay_path: PathBuf::from("/tmp/overrides.json"),
            csrf: Arc::new("csrf".to_string()),
            catalog: Arc::new(Value::Null),
            started_at: Instant::now(),
            reverse_proxy: Arc::new(Mutex::new(ManagedReverseProxy::default())),
        });
        let status = get_service_status(&state);
        assert!(status.get("logs").is_none());
        assert_eq!(status.get("status").and_then(Value::as_str), Some("ok"));
    }

    #[test]
    fn reverse_proxy_presets_are_allowlisted() {
        for preset in ["chatgpt", "claude", "gemini", "kimi", "deepseek", "custom"] {
            assert!(valid_reverse_proxy_preset(preset));
        }
        assert!(!valid_reverse_proxy_preset("shell"));
        assert!(!valid_reverse_proxy_preset("../../bin"));
    }

    #[test]
    fn reverse_proxy_custom_target_rejects_credentials_and_non_http() {
        assert!(validate_reverse_proxy_target("https://example.com/chat").is_ok());
        assert!(validate_reverse_proxy_target("file:///tmp/chat").is_err());
        assert!(validate_reverse_proxy_target("https://user:pass@example.com/chat").is_err());
    }
}
