use std::io::{BufRead, BufReader, Write};
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
            std::env::temp_dir().join(format!("kittd-test-{}-{}", std::process::id(), nanos));
        let config_dir = work_dir.join("kitt").join("assistant");
        std::fs::create_dir_all(&config_dir).unwrap();

        let token = format!("test-token-{}", nanos);
        let token_path = config_dir.join("auth.token");
        std::fs::write(&token_path, &token).unwrap();

        let config = serde_json::json!({
            "listen": format!("127.0.0.1:{}", port),
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

        let bin = env!("CARGO_BIN_EXE_kittd");

        let child = Command::new(bin)
            .env("XDG_CONFIG_HOME", &work_dir)
            .env("HOME", &work_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn kittd");

        let addr = format!("127.0.0.1:{}", port);

        // Wait for port to become reachable
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

    fn send_raw(&self, line: &str) -> serde_json::Value {
        let mut stream = TcpStream::connect(&self.addr).expect("connect to kittd");
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        writeln!(stream, "{}", line).unwrap();

        let mut reader = BufReader::new(stream);
        let mut resp_line = String::new();
        reader.read_line(&mut resp_line).expect("read response");
        serde_json::from_str(&resp_line).expect("parse json response")
    }

    fn send(
        &self,
        cmd: &str,
        mut payload: serde_json::Map<String, serde_json::Value>,
    ) -> serde_json::Value {
        payload.insert("token".into(), self.token.clone().into());
        payload.insert("command".into(), cmd.into());
        let line = serde_json::to_string(&payload).unwrap();
        self.send_raw(&line)
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
fn test_unauthorized_request_rejected() {
    let daemon = TestDaemon::start(41930);

    let resp = daemon.send_raw(r#"{"token":"wrong","command":"ping"}"#);
    assert_eq!(resp["ok"], false);
    assert_eq!(resp["error"], "unauthorized");

    let resp_no_tok = daemon.send_raw(r#"{"command":"ping"}"#);
    assert_eq!(resp_no_tok["ok"], false);
    assert_eq!(resp_no_tok["error"], "unauthorized");
}

#[test]
fn test_oversized_request_rejected_by_bounded_reader() {
    let daemon = TestDaemon::start(41931);

    // Request > 1 MiB (1024 * 1024 + 100 bytes)
    let huge_content = "A".repeat(1024 * 1024 + 100);
    let raw = format!(
        r#"{{"token":"{}","command":"memory_remember","content":"{}"}}"#,
        daemon.token, huge_content
    );

    let resp = daemon.send_raw(&raw);
    assert_eq!(resp["ok"], false);
    assert_eq!(resp["error"], "request_too_large");
}

#[test]
fn test_authenticated_ping_and_memory_roundtrip() {
    let daemon = TestDaemon::start(41932);

    let ping_resp = daemon.send("ping", serde_json::Map::new());
    assert_eq!(ping_resp["ok"], true);

    // Remember
    let mut mem_payload = serde_json::Map::new();
    mem_payload.insert("workspace_id".into(), "test-ws".into());
    mem_payload.insert("content".into(), "Configuração de rede local".into());
    mem_payload.insert("kind".into(), "PROJECT_RULE".into());
    mem_payload.insert("pinned".into(), true.into());
    let rem_resp = daemon.send("memory_remember", mem_payload);
    assert_eq!(rem_resp["ok"], true);
    assert!(rem_resp["result"]["id"].as_str().is_some());

    // Recall
    let mut recall_payload = serde_json::Map::new();
    recall_payload.insert("workspace_id".into(), "test-ws".into());
    recall_payload.insert("query".into(), "rede local".into());
    let rec_resp = daemon.send("memory_recall", recall_payload);
    assert_eq!(rec_resp["ok"], true);
    let records = rec_resp["result"].as_array().unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["content"], "Configuração de rede local");
    assert_eq!(records[0]["pinned"], true);
}

#[test]
fn test_hud_subscriber_without_token_rejected() {
    let daemon = TestDaemon::start(41933);

    let resp = daemon.send_raw(r#"{"command":"subscribe_hud","token":"invalid"}"#);
    assert_eq!(resp["ok"], false);
    assert_eq!(resp["error"], "unauthorized");
}
