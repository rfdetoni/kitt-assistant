use serde_json::{Value, json};
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    net::TcpStream,
};
fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        usage()
    };
    let (command, payload) = match args[1].as_str() {
        "ask" if args.len() >= 3 => ("ask", json!({"text":args[2..].join(" ")})),
        "remember" if args.len() >= 3 => ("remember", json!({"text":args[2..].join(" ")})),
        "image" if args.len() >= 3 => ("image", json!({"src":args[2],"alt":args.get(3)})),
        "ping" => ("ping", json!({})),
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
    let mut stream =
        TcpStream::connect(addr).unwrap_or_else(|e| fatal(format!("connect {addr}: {e}")));
    let mut req = payload.as_object().cloned().unwrap_or_default();
    req.insert("token".into(), json!(token.trim()));
    req.insert("command".into(), json!(command));
    writeln!(stream, "{}", Value::Object(req)).unwrap();
    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line).unwrap();
    let resp: Value = serde_json::from_str(line.trim()).unwrap_or_else(|e| fatal(e.to_string()));
    if !resp.get("ok").and_then(Value::as_bool).unwrap_or(false) {
        fatal(resp.get("error").unwrap_or(&resp).to_string())
    }
    if let Some(text) = resp.pointer("/result/text").and_then(Value::as_str) {
        println!("{text}")
    } else {
        println!("{}", resp.get("result").unwrap_or(&Value::Null))
    }
}
fn usage() -> ! {
    eprintln!("usage: kittctl ask <text> | remember <text> | image <path-or-url> [alt] | ping");
    std::process::exit(2)
}
fn fatal<T: std::fmt::Display, R>(e: T) -> R {
    eprintln!("error: {e}");
    std::process::exit(1)
}
