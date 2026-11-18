//! Rune-native HUD. Subscribes to existing authenticated kittd IPC.
//! The daemon, not the UI, owns all assistant state and execution.
use rune::tok::{self, type_};
use rune::{App, AppSpec, Cx, El, col, row, text};
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Write},
    net::{IpAddr, SocketAddr, TcpStream},
    sync::{Arc, atomic::{AtomicBool, Ordering}},
    time::Duration,
};

const LIMIT: usize = 1024 * 1024;

#[derive(Default)]
struct Hud {
    connected: bool,
    phase: String,
    message: String,
}
enum HudUpdate {
    Status(String), Text(String), ImageAlt(String), Hide,
}
fn update_from_frame(line: &[u8]) -> Option<HudUpdate> {
    if line.len() > LIMIT { return None; }
    let data: Value = serde_json::from_slice(line).ok()?;
    if data.get("kind")?.as_str()? != "hud.event" { return None; }
    let p = data.get("payload")?;
    match p.get("type")?.as_str()? {
        "status" => Some(HudUpdate::Status(
            p.get("state").and_then(Value::as_str).unwrap_or("idle")
                .chars().take(48).collect())),
        "text" => Some(HudUpdate::Text(
            p.get("content").and_then(Value::as_str).unwrap_or("")
                .chars().take(2048).collect())),
        "image" => Some(HudUpdate::ImageAlt(
            p.get("alt").and_then(Value::as_str).unwrap_or("Imagem recebida")
                .chars().take(128).collect())),
        "hide" => Some(HudUpdate::Hide),
        _ => None,
    }
}

fn view(s: &Hud, _: &mut Cx<Hud>) -> El {
    let colors = tok::theme(true);
    let phase = if s.phase.is_empty() { "IDLE" } else { &s.phase };
    let message = if s.message.is_empty() {
        "Aguardando atividade do K.I.T.T."
    } else { &s.message };
    col().w_full().h_full().padding(22.0).gap(12.0).fill(colors.bg)
        .child(row().w_full().center_y()
            .child(text("K.I.T.T.").font(type_::TITLE).color(colors.text))
            .child(row().grow(1.0))
            .child(text(if s.connected { "CONECTADO" } else { "DESCONECTADO" })
                .font(type_::MONO_CAPTION).color(colors.text_secondary)))
        .child(text(phase.to_uppercase()).font(type_::MONO_CAPTION).color(tok::accent(0, true)))
        .child(text(message.to_string()).font(type_::BODY).color(colors.text).w_full())
}

fn stream_updates(handle: rune::AppHandle<Hud>, alive: Arc<AtomicBool>) {
    let endpoint = std::env::var("KITT_DAEMON_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:41827".to_owned());
    let Ok(address) = endpoint.parse::<SocketAddr>() else { return };
    if !matches!(address.ip(), IpAddr::V4(_) | IpAddr::V6(_)) ||
        !address.ip().is_loopback() { return; }
    let Ok(token) = std::env::var("KITT_DAEMON_TOKEN") else { return };
    if token.len() < 48 || !token.bytes().all(|b| b.is_ascii_hexdigit()) { return; }
    while alive.load(Ordering::Relaxed) {
        let session = TcpStream::connect_timeout(&address, Duration::from_secs(3));
        if let Ok(mut stream) = session {
            let _ = stream.set_read_timeout(Some(Duration::from_secs(3)));
            let _ = stream.set_write_timeout(Some(Duration::from_secs(3)));
            let hello = json!({
                "token": token,
                "envelope": {"version":1, "id":"hud-rune", "kind":"hud.subscribe.request", "payload":{}}
            }).to_string() + "\n";
            if stream.write_all(hello.as_bytes()).is_ok() {
                handle.update(|s,tx| { s.connected=true; tx.instant(); });
                let mut reader = BufReader::new(stream);
                loop {
                    if !alive.load(Ordering::Relaxed) { break; }
                    let mut line = Vec::new();
                    match reader.read_until(b'\n', &mut line) {
                        Ok(0) => break,
                        Ok(n) if n <= LIMIT && line.ends_with(b"\n") => {
                            if let Some(item) = update_from_frame(&line) {
                                handle.update(move |s,tx| {
                                    match item {
                                        HudUpdate::Status(phase) => s.phase=phase,
                                        HudUpdate::Text(content) => s.message=content,
                                        HudUpdate::ImageAlt(alt) => s.message=alt,
                                        HudUpdate::Hide => {s.phase.clear(); s.message.clear();},
                                    }
                                    tx.instant();
                                });
                            }
                        }
                        Err(err) if err.kind() == std::io::ErrorKind::WouldBlock
                             || err.kind() == std::io::ErrorKind::TimedOut => continue,
                        _ => break,
                    }
                }
            }
        }
        handle.update(|s,tx| { s.connected=false; tx.instant(); });
        for _ in 0..20 {
            if !alive.load(Ordering::Relaxed) { break; }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}
pub fn run() {
    let app = App::new(AppSpec::new("KITT Rune HUD", (560.0, 175.0), Hud::default, view));
    let active = Arc::new(AtomicBool::new(true));
    let worker = active.clone();
    let handle = app.handle();
    std::thread::spawn(move || stream_updates(handle, worker));
    app.run();
    active.store(false, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_bounded_hud_events_are_rendered() {
        let line = br#"{"version":1,"kind":"hud.event","payload":{"type":"text","content":"ok","ttl_ms":5000}}"#;
        assert!(matches!(update_from_frame(line), Some(HudUpdate::Text(s)) if s=="ok"));
        assert!(update_from_frame(br#"{"kind":"system.error","payload":{"type":"text"}}"#).is_none());
        assert!(update_from_frame(&vec![b'0'; LIMIT + 1]).is_none());
    }
}
