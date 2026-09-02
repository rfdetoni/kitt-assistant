pub use kitt_memory_core::MemoryRecord;
use serde::{Deserialize, Serialize};
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AssistantError {
    #[error("model: {0}")]
    Model(String),
    #[error("memory: {0}")]
    Memory(String),
    #[error("transcription: {0}")]
    Transcription(String),
    #[error("speech output: {0}")]
    SpeechOutput(String),
    #[error("configuration: {0}")]
    Configuration(String),
    #[error("I/O: {0}")]
    Io(String),
}

pub type Result<T> = std::result::Result<T, AssistantError>;

#[derive(Debug, Clone)]
pub struct ModelRequest {
    pub system: String,
    pub user: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelAnswer {
    pub text: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ModelTier {
    Fast,
    Heavy,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum RouteHint {
    #[default]
    Auto,
    Fast,
    Heavy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutedAnswer {
    pub text: String,
    pub tier: ModelTier,
    pub fallback_used: bool,
}

#[derive(Debug, Clone)]
pub struct RoutingPolicy {
    pub fast_max_chars: usize,
    pub fast_max_lines: usize,
}

impl Default for RoutingPolicy {
    fn default() -> Self {
        Self {
            fast_max_chars: 360,
            fast_max_lines: 4,
        }
    }
}

impl RoutingPolicy {
    pub fn choose(&self, text: &str, hint: RouteHint) -> ModelTier {
        match hint {
            RouteHint::Fast => return ModelTier::Fast,
            RouteHint::Heavy => return ModelTier::Heavy,
            RouteHint::Auto => {}
        }

        let normalized = text.to_ascii_lowercase();
        let heavy_markers = [
            "```",
            "analis",
            "analy",
            "arquitet",
            "architect",
            "implementar",
            "implement",
            "refator",
            "refactor",
            "debug",
            "investig",
            "pesquis",
            "research",
            "compar",
            "planej",
            "design",
            "código",
            "codigo",
            "codebase",
            "stack trace",
        ];
        if heavy_markers
            .iter()
            .any(|marker| normalized.contains(marker))
        {
            return ModelTier::Heavy;
        }

        let lines = text.lines().count();
        if text.chars().count() <= self.fast_max_chars && lines <= self.fast_max_lines {
            ModelTier::Fast
        } else {
            ModelTier::Heavy
        }
    }
}

pub trait ModelPort: Send + Sync {
    fn complete(&self, request: &ModelRequest) -> Result<ModelAnswer>;
    fn is_local(&self) -> bool;
}

pub trait TranscriptionPort: Send + Sync {
    fn transcribe(&self, path: &Path, locale: Option<&str>, prompt: Option<&str>)
    -> Result<String>;
    fn is_local(&self) -> bool;
}

pub trait SpeechOutputPort: Send + Sync {
    fn speak(&self, text: &str, locale: Option<&str>) -> Result<()>;
}

pub trait MemoryPort: Send + Sync {
    fn recall_for_model(&self, query: &str, is_local_provider: bool) -> Result<Vec<MemoryRecord>>;
    fn remember_episode(&self, text: &str) -> Result<()>;
    fn remember_explicit(&self, text: &str) -> Result<String>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_short_conversation_to_fast() {
        assert_eq!(
            RoutingPolicy::default().choose("Que horas são?", RouteHint::Auto),
            ModelTier::Fast
        );
    }

    #[test]
    fn routes_complex_work_to_heavy() {
        assert_eq!(
            RoutingPolicy::default().choose(
                "Analise esta arquitetura e refatore o código",
                RouteHint::Auto
            ),
            ModelTier::Heavy
        );
    }

    #[test]
    fn explicit_hint_wins() {
        assert_eq!(
            RoutingPolicy::default().choose("implementar tudo", RouteHint::Fast),
            ModelTier::Fast
        );
    }
}
