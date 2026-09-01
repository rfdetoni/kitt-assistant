use serde::{Deserialize, Serialize};
use std::{fs, net::IpAddr, path::Path};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderProfile {
    pub base_url: String,
    pub model: String,
    #[serde(default)]
    pub api_key_env: Option<String>,
    #[serde(default = "default_true")]
    pub local_provider: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpeechProfile {
    pub base_url: String,
    pub model: String,
    #[serde(default)]
    pub api_key_env: Option<String>,
    #[serde(default = "default_true")]
    pub local_provider: bool,
    #[serde(default)]
    pub allow_remote: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelProfiles {
    pub fast: ProviderProfile,
    pub heavy: ProviderProfile,
    pub speech_to_text: SpeechProfile,
    #[serde(default = "default_fast_chars")]
    pub fast_max_chars: usize,
    #[serde(default = "default_fast_lines")]
    pub fast_max_lines: usize,
}

fn default_true() -> bool {
    true
}
fn default_fast_chars() -> usize {
    360
}
fn default_fast_lines() -> usize {
    4
}

impl ModelProfiles {
    pub fn load_or_create(
        dir: &Path,
        legacy_base_url: &str,
        legacy_model: &str,
        legacy_api_key_env: Option<String>,
        legacy_local: bool,
    ) -> Result<Self, String> {
        let path = dir.join("models.json");
        if path.exists() {
            let profiles: Self = serde_json::from_str(
                &fs::read_to_string(&path).map_err(|e| format!("read models.json: {e}"))?,
            )
            .map_err(|e| format!("parse models.json: {e}"))?;
            profiles.validate()?;
            return Ok(profiles);
        }

        let fast_model = std::env::var("KITT_FAST_MODEL")
            .or_else(|_| std::env::var("KITT_MODEL"))
            .unwrap_or_else(|_| legacy_model.to_string());
        let heavy_model = std::env::var("KITT_HEAVY_MODEL").unwrap_or_else(|_| fast_model.clone());
        let stt_model = std::env::var("KITT_STT_MODEL").unwrap_or_else(|_| "whisper-1".into());

        let fast_base_url =
            std::env::var("KITT_FAST_BASE_URL").unwrap_or_else(|_| legacy_base_url.to_string());
        let heavy_base_url =
            std::env::var("KITT_HEAVY_BASE_URL").unwrap_or_else(|_| legacy_base_url.to_string());
        let stt_base_url =
            std::env::var("KITT_STT_BASE_URL").unwrap_or_else(|_| legacy_base_url.to_string());

        let profiles = Self {
            fast: ProviderProfile {
                local_provider: env_bool("KITT_FAST_LOCAL_PROVIDER")
                    .unwrap_or_else(|| infer_local(&fast_base_url).unwrap_or(legacy_local)),
                base_url: fast_base_url,
                model: fast_model,
                api_key_env: std::env::var("KITT_FAST_API_KEY_ENV")
                    .ok()
                    .or_else(|| legacy_api_key_env.clone()),
            },
            heavy: ProviderProfile {
                local_provider: env_bool("KITT_HEAVY_LOCAL_PROVIDER")
                    .unwrap_or_else(|| infer_local(&heavy_base_url).unwrap_or(legacy_local)),
                base_url: heavy_base_url,
                model: heavy_model,
                api_key_env: std::env::var("KITT_HEAVY_API_KEY_ENV")
                    .ok()
                    .or_else(|| legacy_api_key_env.clone()),
            },
            speech_to_text: SpeechProfile {
                local_provider: env_bool("KITT_STT_LOCAL_PROVIDER")
                    .unwrap_or_else(|| infer_local(&stt_base_url).unwrap_or(legacy_local)),
                allow_remote: env_bool("KITT_STT_ALLOW_REMOTE").unwrap_or(false),
                base_url: stt_base_url,
                model: stt_model,
                api_key_env: std::env::var("KITT_STT_API_KEY_ENV")
                    .ok()
                    .or(legacy_api_key_env),
            },
            fast_max_chars: default_fast_chars(),
            fast_max_lines: default_fast_lines(),
        };
        profiles.validate()?;
        fs::write(
            &path,
            serde_json::to_string_pretty(&profiles).map_err(|e| e.to_string())?,
        )
        .map_err(|e| format!("write models.json: {e}"))?;
        Ok(profiles)
    }

    pub fn validate(&self) -> Result<(), String> {
        validate_profile(
            "fast",
            &self.fast.base_url,
            &self.fast.model,
            self.fast.local_provider,
        )?;
        validate_profile(
            "heavy",
            &self.heavy.base_url,
            &self.heavy.model,
            self.heavy.local_provider,
        )?;
        validate_profile(
            "speech_to_text",
            &self.speech_to_text.base_url,
            &self.speech_to_text.model,
            self.speech_to_text.local_provider,
        )?;
        if self.fast_max_chars == 0 || self.fast_max_lines == 0 {
            return Err("routing thresholds must be greater than zero".into());
        }
        Ok(())
    }
}

pub fn api_key(env_name: Option<&String>) -> Option<String> {
    env_name
        .and_then(|name| std::env::var(name).ok())
        .filter(|value| !value.trim().is_empty())
}

fn env_bool(name: &str) -> Option<bool> {
    let value = std::env::var(name).ok()?;
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn infer_local(base_url: &str) -> Option<bool> {
    let parsed = url::Url::parse(base_url).ok()?;
    Some(is_loopback_host(parsed.host_str()))
}

fn validate_profile(name: &str, base_url: &str, model: &str, local: bool) -> Result<(), String> {
    if model.trim().is_empty() {
        return Err(format!("{name} model cannot be empty"));
    }
    let parsed = url::Url::parse(base_url).map_err(|e| format!("invalid {name} base_url: {e}"))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(format!("{name} provider must use http or https"));
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(format!("{name} base_url must not embed credentials"));
    }
    if parsed.query().is_some() || parsed.fragment().is_some() {
        return Err(format!("{name} base_url cannot contain query or fragment"));
    }
    if local && !is_loopback_host(parsed.host_str()) {
        return Err(format!(
            "{name}.local_provider=true requires localhost/loopback base_url"
        ));
    }
    Ok(())
}

fn is_loopback_host(host: Option<&str>) -> bool {
    let Some(host) = host else {
        return false;
    };
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<IpAddr>()
            .map(|ip| ip.is_loopback())
            .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_local_provider_rejects_remote_host() {
        let result = validate_profile("fast", "http://192.168.1.100:11434/v1", "qwen", true);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("requires localhost/loopback"));
    }

    #[test]
    fn test_local_provider_accepts_loopback() {
        assert!(validate_profile("fast", "http://127.0.0.1:11434/v1", "qwen", true).is_ok());
        assert!(validate_profile("fast", "http://localhost:11434/v1", "qwen", true).is_ok());
    }

    #[test]
    fn test_remote_provider_accepts_remote_host() {
        assert!(validate_profile("fast", "http://192.168.1.100:11434/v1", "qwen", false).is_ok());
    }

    #[test]
    fn test_provider_base_url_rejects_embedded_credentials() {
        let result = validate_profile("fast", "http://user:pass@127.0.0.1:11434/v1", "qwen", true);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("must not embed credentials"));
    }

    #[test]
    fn test_provider_base_url_rejects_query_and_fragment() {
        assert!(
            validate_profile(
                "fast",
                "http://127.0.0.1:11434/v1?token=secret",
                "qwen",
                true,
            )
            .is_err()
        );
        assert!(
            validate_profile("fast", "http://127.0.0.1:11434/v1#unexpected", "qwen", true,)
                .is_err()
        );
    }

    #[test]
    fn test_infer_local_detects_loopback() {
        assert_eq!(infer_local("http://127.0.0.1:8000/v1"), Some(true));
        assert_eq!(infer_local("https://api.example.com/v1"), Some(false));
    }

    #[test]
    fn test_invalid_models_json_fails_load() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let temp_dir = std::env::temp_dir().join(format!("test-models-{nanos}"));
        std::fs::create_dir_all(&temp_dir).unwrap();
        std::fs::write(temp_dir.join("models.json"), b"invalid json content").unwrap();

        let result =
            ModelProfiles::load_or_create(&temp_dir, "http://127.0.0.1:11434", "qwen", None, true);
        assert!(result.is_err());
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
