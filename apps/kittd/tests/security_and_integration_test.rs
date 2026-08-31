use kitt_protocol::{
    AuthenticatedFrame, Envelope, MAX_FRAME_BYTES, MemoryKind, MemoryRecallRequest,
    MemoryRememberRequest, MemoryScope, Sensitivity, kinds,
};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

struct TestDaemon {
    child: Child,
    addr: String,
    token: String,
    work_dir: PathBuf,
}

impl TestDaemon {
    fn start(port: u16) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let work_dir =
            std::env::temp_dir().join(format!("kittd-test-{}-{nanos}", std::process::id()));
        let config_dir = work_dir.join("kitt").join("assistant");
        std::fs::create_dir_all(&config_dir).unwrap();

        let token = format!("{nanos:064x}");
        std::fs::write(config_dir.join("auth.token"), &token).unwrap();
        let config = serde_json::json!({
            "listen": format!("127.0.0.1:{port}"),
            "base_url": "http://127.0.0.1:11434/v1",
            "model": "qwen3:4b",
            "api_key_env": null,
            "local_provider": true,
            "allow_personal_remote": false,
            "hud_ttl_ms": 1000
        });
        std::fs::write(
            config_dir.join("config.json"),
            serde_json::to_string(&config).unwrap(),
        )
        .unwrap();

        let child = Command::new(env!("CARGO_BIN_EXE_kittd"))
            .env("XDG_CONFIG_HOME", &work_dir)
            .env("HOME", &work_dir)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn kittd");
        let addr = format!("127.0.0.1:{port}");
        for _ in 0..50 {
            if TcpStream::connect(&addr).is_ok() {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }

        Self {
            child,
            addr,
            token,
            work_dir,
        }
    }

    fn call(&self, envelope: Envelope) -> Envelope {
        let mut stream = TcpStream::connect(&self.addr).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let request_id = envelope.id.clone();
        let frame = AuthenticatedFrame::new(self.token.clone(), envelope).unwrap();
        writeln!(stream, "{}", serde_json::to_string(&frame).unwrap()).unwrap();

        let mut reader = BufReader::new(stream).take(MAX_FRAME_BYTES as u64 + 1);
        let mut bytes = Vec::new();
        reader.read_until(b'\n', &mut bytes).unwrap();
        assert!(bytes.len() <= MAX_FRAME_BYTES);
        while bytes.last().is_some_and(|b| *b == b'\n' || *b == b'\r') {
            bytes.pop();
        }
        let response = Envelope::decode(&bytes).unwrap();
        assert_eq!(
            response.correlation_id.as_deref(),
            Some(request_id.as_str())
        );
        response
    }

    fn raw(&self, line: &[u8]) -> Envelope {
        let mut stream = TcpStream::connect(&self.addr).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        stream.write_all(line).unwrap();
        stream.write_all(b"\n").unwrap();
        let mut bytes = Vec::new();
        BufReader::new(stream)
            .read_until(b'\n', &mut bytes)
            .unwrap();
        while bytes.last().is_some_and(|b| *b == b'\n' || *b == b'\r') {
            bytes.pop();
        }
        Envelope::decode(&bytes).unwrap()
    }
}

impl Drop for TestDaemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.work_dir);
    }
}

#[test]
fn legacy_flat_command_is_rejected() {
    let daemon = TestDaemon::start(41930);
    let response = daemon.raw(br#"{"token":"x","command":"ping"}"#);
    assert_eq!(response.kind, kinds::SYSTEM_ERROR);
    assert_eq!(response.payload["code"], "invalid_frame");
}

#[test]
fn unauthorized_protocol_v1_request_is_rejected() {
    let daemon = TestDaemon::start(41931);
    let request = Envelope::new(kinds::SYSTEM_PING_REQUEST, serde_json::json!({})).unwrap();
    let frame = AuthenticatedFrame::new("b".repeat(64), request.clone()).unwrap();
    let response = daemon.raw(serde_json::to_string(&frame).unwrap().as_bytes());
    assert_eq!(response.kind, kinds::SYSTEM_ERROR);
    assert_eq!(response.payload["code"], "unauthorized");
    assert_eq!(
        response.correlation_id.as_deref(),
        Some(request.id.as_str())
    );
}

#[test]
fn authenticated_ping_and_memory_roundtrip() {
    let daemon = TestDaemon::start(41932);

    let ping =
        daemon.call(Envelope::new(kinds::SYSTEM_PING_REQUEST, serde_json::json!({})).unwrap());
    assert_eq!(ping.kind, kinds::SYSTEM_PING_RESPONSE);

    let remember = daemon.call(
        Envelope::new(
            kinds::MEMORY_REMEMBER_REQUEST,
            MemoryRememberRequest {
                namespace: "agent-cli".into(),
                workspace_id: "test-ws".into(),
                content: "Configuração de rede local".into(),
                kind: MemoryKind::ProjectRule,
                sensitivity: Sensitivity::Private,
                scope: MemoryScope::Workspace,
                importance: 0.8,
                confidence: 1.0,
                pinned: true,
                ttl_seconds: None,
            },
        )
        .unwrap(),
    );
    assert_eq!(remember.kind, kinds::MEMORY_REMEMBER_RESPONSE);

    let recall = daemon.call(
        Envelope::new(
            kinds::MEMORY_RECALL_REQUEST,
            MemoryRecallRequest {
                namespace: "agent-cli".into(),
                workspace_id: "test-ws".into(),
                query: "rede local".into(),
                limit: 8,
                allow_private: true,
                allow_secret: false,
            },
        )
        .unwrap(),
    );
    assert_eq!(recall.kind, kinds::MEMORY_RECALL_RESPONSE);
    assert_eq!(
        recall.payload["records"][0]["content"],
        "Configuração de rede local"
    );
}

#[test]
fn oversized_request_is_bounded() {
    let daemon = TestDaemon::start(41933);
    let huge = vec![b'x'; MAX_FRAME_BYTES + 10];
    let response = daemon.raw(&huge);
    assert_eq!(response.kind, kinds::SYSTEM_ERROR);
    assert_eq!(response.payload["code"], "request_too_large");
}

#[test]
fn routed_ask_and_transcribe_correlation() {
    let daemon = TestDaemon::start(41934);

    let empty_ask = daemon.call(
        Envelope::new(
            kinds::ASSISTANT_ASK_ROUTED_REQUEST,
            kitt_protocol::RoutedAskRequest {
                text: "   ".into(),
                locale: None,
                route: kitt_protocol::ModelRoute::Auto,
                show_hud: false,
            },
        )
        .unwrap(),
    );
    assert_eq!(empty_ask.kind, kinds::SYSTEM_ERROR);
    assert_eq!(empty_ask.payload["code"], "empty_text");

    let empty_transcribe = daemon.call(
        Envelope::new(
            kinds::ASSISTANT_TRANSCRIBE_REQUEST,
            kitt_protocol::TranscribeRequest {
                path: "".into(),
                locale: None,
                show_hud: false,
            },
        )
        .unwrap(),
    );
    assert_eq!(empty_transcribe.kind, kinds::SYSTEM_ERROR);
    assert_eq!(empty_transcribe.payload["code"], "empty_path");
}
