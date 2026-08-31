use std::{
    io::{BufRead, BufReader, Write},
    net::TcpStream,
    thread,
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
                let _ = window.set_ignore_cursor_events(true);
            }
            let handle = app.handle().clone();
            thread::spawn(move || {
                let addr =
                    std::env::var("KITT_DAEMON_ADDR").unwrap_or_else(|_| "127.0.0.1:41827".into());
                let token = std::env::var("KITT_DAEMON_TOKEN").unwrap_or_default();
                let Ok(mut stream) = TcpStream::connect(addr) else {
                    handle.exit(2);
                    return;
                };
                let req = serde_json::json!({"token":token,"command":"subscribe_hud"});
                if writeln!(stream, "{req}").is_err() {
                    handle.exit(2);
                    return;
                }
                let mut reader = BufReader::new(stream);
                let mut first = String::new();
                if reader.read_line(&mut first).is_err() {
                    handle.exit(2);
                    return;
                }
                for line in reader.lines().map_while(Result::ok) {
                    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) {
                        let _ = handle.emit("hud-event", value);
                    }
                }
                handle.exit(0);
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("run KITT HUD");
}
