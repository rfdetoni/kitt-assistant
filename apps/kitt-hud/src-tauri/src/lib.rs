use kitt_protocol::{AuthenticatedFrame, Envelope, MAX_FRAME_BYTES, kinds};
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::TcpStream,
    thread,
    time::Duration,
};
use tauri::{Emitter, Manager};

#[tauri::command]
fn exit_hud(app: tauri::AppHandle) {
    app.exit(0)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![exit_hud])
        .setup(|app| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_always_on_top(true);
                let _ = window.set_ignore_cursor_events(true);
                #[cfg(target_os = "linux")]
                {
                    let _ = window.set_visible_on_all_workspaces(true);
                }
                if let Ok(Some(monitor)) = window.current_monitor() {
                    let screen_size = monitor.size();
                    let window_size = window
                        .outer_size()
                        .unwrap_or(tauri::PhysicalSize { width: 640, height: 180 });
                    let x = (screen_size.width.saturating_sub(window_size.width)) / 2;
                    let y = 20;
                    let _ = window.set_position(tauri::Position::Physical(
                        tauri::PhysicalPosition {
                            x: x as i32,
                            y: y as i32,
                        },
                    ));
                }
            }
            let handle = app.handle().clone();
            thread::spawn(move || {
                let config_dir = std::env::var("XDG_CONFIG_HOME")
                    .map(std::path::PathBuf::from)
                    .ok()
                    .or_else(|| {
                        std::env::var("HOME")
                            .or_else(|_| std::env::var("USERPROFILE"))
                            .map(|h| std::path::PathBuf::from(h).join(".config"))
                            .ok()
                    })
                    .unwrap_or_else(|| std::path::PathBuf::from("."));
                let dir = config_dir.join("kitt").join("assistant");

                let token = std::env::var("KITT_DAEMON_TOKEN")
                    .ok()
                    .filter(|s| !s.trim().is_empty())
                    .or_else(|| std::fs::read_to_string(dir.join("auth.token")).ok())
                    .unwrap_or_default();

                let addr = std::env::var("KITT_DAEMON_ADDR")
                    .ok()
                    .filter(|s| !s.trim().is_empty())
                    .or_else(|| {
                        let cfg: serde_json::Value =
                            serde_json::from_str(&std::fs::read_to_string(dir.join("config.json")).ok()?)
                                .ok()?;
                        cfg.get("listen").and_then(serde_json::Value::as_str).map(String::from)
                    })
                    .unwrap_or_else(|| "127.0.0.1:41827".into());

                if token.trim().is_empty() {
                    handle.exit(2);
                    return;
                }

                let Ok(mut stream) = TcpStream::connect(&addr) else {
                    handle.exit(2);
                    return;
                };
                let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
                let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));

                let request =
                    match Envelope::new(kinds::HUD_SUBSCRIBE_REQUEST, serde_json::json!({})) {
                        Ok(request) => request,
                        Err(_) => {
                            handle.exit(2);
                            return;
                        }
                    };
                let request_id = request.id.clone();
                let frame = match AuthenticatedFrame::new(token, request) {
                    Ok(frame) => frame,
                    Err(_) => {
                        handle.exit(2);
                        return;
                    }
                };
                let line = match serde_json::to_string(&frame) {
                    Ok(line) if line.len() <= MAX_FRAME_BYTES => line,
                    _ => {
                        handle.exit(2);
                        return;
                    }
                };
                if writeln!(stream, "{line}").is_err() {
                    handle.exit(2);
                    return;
                }

                let mut reader = BufReader::new(stream);
                let mut subscribed = false;
                loop {
                    let mut limited = reader.by_ref().take(MAX_FRAME_BYTES as u64 + 1);
                    let mut bytes = Vec::new();
                    match limited.read_until(b'\n', &mut bytes) {
                        Ok(0) | Err(_) => break,
                        Ok(_) if bytes.len() > MAX_FRAME_BYTES => break,
                        Ok(_) => {}
                    }
                    if !bytes.ends_with(b"\n") {
                        break;
                    }
                    while bytes.last().is_some_and(|b| *b == b'\n' || *b == b'\r') {
                        bytes.pop();
                    }
                    let Ok(envelope) = Envelope::decode(&bytes) else {
                        break;
                    };

                    if envelope.kind == kinds::SYSTEM_ERROR
                        && envelope.correlation_id.as_deref() == Some(request_id.as_str())
                    {
                        break;
                    }
                    if envelope.kind == kinds::HUD_SUBSCRIBE_RESPONSE
                        && envelope.correlation_id.as_deref() == Some(request_id.as_str())
                    {
                        subscribed = true;
                        continue;
                    }
                    if envelope.kind == kinds::HUD_EVENT && subscribed {
                        let _ = handle.emit("hud-event", envelope.payload);
                    }
                }
                handle.exit(if subscribed { 0 } else { 2 });
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("run KITT HUD");
}
