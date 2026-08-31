use kitt_domain::{AssistantError, MemoryPort, ModelAnswer, ModelPort, ModelRequest, Result};
use kitt_memory_core::{
    EgressPolicy, MemoryKind, MemoryScope, MemoryStore, NewMemory, RecallQuery, Sensitivity,
};
use kitt_memory_sqlite::SqliteMemoryStore;
use reqwest::blocking::Client;
use serde_json::{Value, json};
use std::{io::Read, sync::Arc, time::Duration};

const MAX_PROVIDER_RESPONSE_BYTES: u64 = 8 * 1024 * 1024;

pub struct OpenAiCompatibleModel {
    base_url: String,
    model: String,
    api_key: Option<String>,
    local: bool,
}

impl OpenAiCompatibleModel {
    pub fn new(
        base_url: String,
        model: String,
        api_key: Option<String>,
        local: bool,
    ) -> Result<Self> {
        if base_url.trim().is_empty() || model.trim().is_empty() {
            return Err(AssistantError::Configuration(
                "base_url and model are required".into(),
            ));
        }
        Ok(Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            model,
            api_key,
            local,
        })
    }
}

impl ModelPort for OpenAiCompatibleModel {
    fn complete(&self, r: &ModelRequest) -> Result<ModelAnswer> {
        let url = format!("{}/chat/completions", self.base_url);
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(120))
            .build()
            .map_err(|e| AssistantError::Model(e.to_string()))?;
        let mut req = client.post(url).json(&json!({
            "model": &self.model,
            "messages": [
                {"role": "system", "content": &r.system},
                {"role": "user", "content": &r.user}
            ],
            "stream": false
        }));
        if let Some(key) = &self.api_key {
            req = req.bearer_auth(key);
        }

        let resp = req
            .send()
            .map_err(|e| AssistantError::Model(e.to_string()))?;
        let status = resp.status();
        if resp
            .content_length()
            .is_some_and(|n| n > MAX_PROVIDER_RESPONSE_BYTES)
        {
            return Err(AssistantError::Model("provider response too large".into()));
        }

        let mut body_bytes = Vec::new();
        resp.take(MAX_PROVIDER_RESPONSE_BYTES + 1)
            .read_to_end(&mut body_bytes)
            .map_err(|e| AssistantError::Model(format!("provider response read failed: {e}")))?;
        if body_bytes.len() as u64 > MAX_PROVIDER_RESPONSE_BYTES {
            return Err(AssistantError::Model("provider response too large".into()));
        }

        let body: Value = serde_json::from_slice(&body_bytes)
            .map_err(|e| AssistantError::Model(format!("invalid JSON: {e}")))?;
        if !status.is_success() {
            return Err(AssistantError::Model(format!(
                "HTTP {status}: {}",
                body.get("error").unwrap_or(&body)
            )));
        }
        let text = body
            .pointer("/choices/0/message/content")
            .and_then(Value::as_str)
            .ok_or_else(|| AssistantError::Model("missing choices[0].message.content".into()))?;
        Ok(ModelAnswer {
            text: text.to_string(),
        })
    }

    fn is_local(&self) -> bool {
        self.local
    }
}

pub struct AssistantMemory {
    store: Arc<SqliteMemoryStore>,
    workspace_id: String,
    allow_personal_remote: bool,
}

impl AssistantMemory {
    pub fn new(
        store: Arc<SqliteMemoryStore>,
        workspace_id: String,
        allow_personal_remote: bool,
    ) -> Self {
        Self {
            store,
            workspace_id,
            allow_personal_remote,
        }
    }
}

impl MemoryPort for AssistantMemory {
    fn recall_for_model(
        &self,
        query: &str,
        is_local_provider: bool,
    ) -> Result<Vec<kitt_memory_core::MemoryRecord>> {
        let policy = EgressPolicy {
            is_local_provider,
            allow_personal_remote: self.allow_personal_remote,
        };
        let mut rows = self
            .store
            .recall(&RecallQuery {
                namespace: "assistant".into(),
                workspace_id: self.workspace_id.clone(),
                text: query.into(),
                limit: 6,
                allow_private: is_local_provider,
                allow_secret: is_local_provider,
            })
            .map_err(|e| AssistantError::Memory(e.to_string()))?;
        rows.retain(|m| policy.allows(m.sensitivity));
        Ok(rows)
    }

    fn remember_episode(&self, text: &str) -> Result<()> {
        self.store
            .remember(NewMemory {
                namespace: "assistant".into(),
                workspace_id: self.workspace_id.clone(),
                kind: MemoryKind::Episodic,
                content: text.into(),
                sensitivity: Sensitivity::Private,
                scope: MemoryScope::Global,
                importance: 0.25,
                confidence: 1.0,
                pinned: false,
                ttl_seconds: Some(30 * 24 * 3600),
                metadata_json: "{\"source\":\"assistant_session\"}".into(),
            })
            .map_err(|e| AssistantError::Memory(e.to_string()))?;
        Ok(())
    }

    fn remember_explicit(&self, text: &str) -> Result<String> {
        let m = self
            .store
            .remember(NewMemory {
                namespace: "assistant".into(),
                workspace_id: self.workspace_id.clone(),
                kind: MemoryKind::PersonalFact,
                content: text.into(),
                sensitivity: Sensitivity::Private,
                scope: MemoryScope::Global,
                importance: 0.8,
                confidence: 1.0,
                pinned: true,
                ttl_seconds: None,
                metadata_json: "{\"source\":\"explicit_remember\"}".into(),
            })
            .map_err(|e| AssistantError::Memory(e.to_string()))?;
        Ok(m.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_assistant_memory_egress_filter() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("kitt-infra-test-{}-{nanos}", std::process::id()));
        let store = Arc::new(SqliteMemoryStore::open(&dir).unwrap());
        let mem = AssistantMemory::new(store.clone(), "global".into(), false);

        store
            .remember(NewMemory {
                namespace: "assistant".into(),
                workspace_id: "global".into(),
                kind: MemoryKind::PersonalFact,
                content: "Secret credential info".into(),
                sensitivity: Sensitivity::Secret,
                scope: MemoryScope::Global,
                importance: 0.9,
                confidence: 1.0,
                pinned: true,
                ttl_seconds: None,
                metadata_json: "{}".into(),
            })
            .unwrap();

        store
            .remember(NewMemory {
                namespace: "assistant".into(),
                workspace_id: "global".into(),
                kind: MemoryKind::UserPreference,
                content: "Public preference".into(),
                sensitivity: Sensitivity::Public,
                scope: MemoryScope::Global,
                importance: 0.8,
                confidence: 1.0,
                pinned: false,
                ttl_seconds: None,
                metadata_json: "{}".into(),
            })
            .unwrap();

        let remote = mem
            .recall_for_model("preference credential", false)
            .unwrap();
        assert_eq!(remote.len(), 1);
        assert_eq!(remote[0].content, "Public preference");

        let local = mem.recall_for_model("preference credential", true).unwrap();
        assert_eq!(local.len(), 2);
        let _ = std::fs::remove_file(&dir);
    }
}
