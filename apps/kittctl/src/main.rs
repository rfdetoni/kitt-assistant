use kitt_protocol::{
    AskRequest, AssistantRememberRequest, AuthenticatedFrame, Envelope, HudImageRequest,
    MAX_FRAME_BYTES, ModelRoute, RoutedAskRequest, StatusResponse, TranscribeRequest, kinds,
};
use serde_json::Value;
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    net::TcpStream,
    time::Duration,
};

mod service;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        usage();
    }

    if args[1] == "service" {
        service::handle_service_command(&args[2..]);
        return;
    }

    let (kind, payload) = match args[1].as_str() {
        "ask" if args.len() >= 3 => (
            kinds::ASSISTANT_ASK_REQUEST,
            serde_json::to_value(AskRequest {
                text: args[2..].join(" "),
                locale: None,
                show_hud: true,
            })
            .unwrap_or_else(|e| fatal(e.to_string())),
        ),
        "ask-fast" if args.len() >= 3 => routed_ask(&args[2..], ModelRoute::Fast),
        "ask-heavy" if args.len() >= 3 => routed_ask(&args[2..], ModelRoute::Heavy),
        "transcribe" if args.len() >= 3 => (
            kinds::ASSISTANT_TRANSCRIBE_REQUEST,
            serde_json::to_value(TranscribeRequest {
                path: args[2].clone(),
                locale: args.get(3).cloned(),
                show_hud: true,
            })
            .unwrap_or_else(|e| fatal(e.to_string())),
        ),
        "remember" if args.len() >= 3 => (
            kinds::ASSISTANT_REMEMBER_REQUEST,
            serde_json::to_value(AssistantRememberRequest {
                text: args[2..].join(" "),
            })
            .unwrap_or_else(|e| fatal(e.to_string())),
        ),
        "image" if args.len() >= 3 => (
            kinds::HUD_IMAGE_REQUEST,
            serde_json::to_value(HudImageRequest {
                src: args[2].clone(),
                alt: args.get(3).cloned(),
            })
            .unwrap_or_else(|e| fatal(e.to_string())),
        ),
        "ping" => (kinds::SYSTEM_PING_REQUEST, serde_json::json!({})),
        _ => usage(),
    };

    let dir = dirs::config_dir()
        .unwrap_or_default()
        .join("kitt")
        .join("assistant");
    let token = fs::read_to_string(dir.join("auth.token"))
        .unwrap_or_else(|e| fatal(format!("read token: {e}")));
    let cfg: Value = serde_json::from_str(
        &fs::read_to_string(dir.join("config.json"))
            .unwrap_or_else(|e| fatal(format!("read config: {e}"))),
    )
    .unwrap_or_else(|e| fatal(e.to_string()));
    let addr = cfg
        .get("listen")
        .and_then(Value::as_str)
        .unwrap_or("127.0.0.1:41827");

    let request = Envelope::new(kind, payload).unwrap_or_else(|e| fatal(e.to_string()));
    let response = call(addr, token.trim(), request.clone());
    if response.correlation_id.as_deref() != Some(request.id.as_str()) {
        fatal("response correlation_id does not match request");
    }
    if response.kind == kinds::SYSTEM_ERROR {
        fatal(response.payload.to_string());
    }

    match response.kind.as_str() {
        kinds::ASSISTANT_ASK_RESPONSE
        | kinds::ASSISTANT_ASK_ROUTED_RESPONSE
        | kinds::ASSISTANT_TRANSCRIBE_RESPONSE => {
            if let Some(text) = response.payload.get("text").and_then(Value::as_str) {
                println!("{text}");
                if response.kind == kinds::ASSISTANT_ASK_ROUTED_RESPONSE {
                    if let Some(tier) = response.payload.get("tier").and_then(Value::as_str) {
                        eprintln!("[route:{tier}]");
                    }
                }
            } else {
                fatal("assistant response missing text");
            }
        }
        kinds::SYSTEM_PING_RESPONSE => {
            let status: StatusResponse =
                serde_json::from_value(response.payload).unwrap_or_else(|e| fatal(e.to_string()));
            println!("{}", status.status);
        }
        _ => println!("{}", response.payload),
    }
}

fn routed_ask(text: &[String], route: ModelRoute) -> (&'static str, Value) {
    (
        kinds::ASSISTANT_ASK_ROUTED_REQUEST,
        serde_json::to_value(RoutedAskRequest {
            text: text.join(" "),
            locale: None,
            route,
            show_hud: true,
        })
        .unwrap_or_else(|e| fatal(e.to_string())),
    )
}

fn call(addr: &str, token: &str, request: Envelope) -> Envelope {
    let mut stream =
        TcpStream::connect(addr).unwrap_or_else(|e| fatal(format!("connect {addr}: {e}")));
    stream
        .set_read_timeout(Some(Duration::from_secs(600)))
        .unwrap_or_else(|e| fatal(e.to_string()));
    stream
        .set_write_timeout(Some(Duration::from_secs(600)))
        .unwrap_or_else(|e| fatal(e.to_string()));
    let frame = AuthenticatedFrame::new(token, request).unwrap_or_else(|e| fatal(e));
    let line = serde_json::to_string(&frame).unwrap_or_else(|e| fatal(e.to_string()));
    if line.len() > MAX_FRAME_BYTES {
        fatal("request exceeds protocol frame limit");
    }
    writeln!(stream, "{line}").unwrap_or_else(|e| fatal(e.to_string()));
    let mut reader = BufReader::new(stream).take(MAX_FRAME_BYTES as u64 + 1);
    let mut bytes = Vec::new();
    reader
        .read_until(b'\n', &mut bytes)
        .unwrap_or_else(|e| fatal(e.to_string()));
    if bytes.len() > MAX_FRAME_BYTES {
        fatal("response too large");
    }
    if !bytes.ends_with(b"\n") {
        fatal("response frame is not newline terminated");
    }
    while bytes.last().is_some_and(|b| *b == b'\n' || *b == b'\r') {
        bytes.pop();
    }
    Envelope::decode(&bytes).unwrap_or_else(|e| fatal(e))
}

fn usage() -> ! {
    eprintln!(
        "usage: kittctl ask <text> | ask-fast <text> | ask-heavy <text> | transcribe <audio-path> [locale] | remember <text> | image <path-or-url> [alt] | ping | service <install|uninstall|start|stop|restart|status>"
    );
    std::process::exit(2)
}

fn fatal<T: std::fmt::Display>(error: T) -> ! {
    eprintln!("error: {error}");
    std::process::exit(1)
}
