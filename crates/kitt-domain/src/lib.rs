pub use kitt_memory_core::MemoryRecord;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AssistantError {
    #[error("model: {0}")]
    Model(String),
    #[error("memory: {0}")]
    Memory(String),
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

pub trait ModelPort: Send + Sync {
    fn complete(&self, request: &ModelRequest) -> Result<ModelAnswer>;
    fn is_local(&self) -> bool;
}
pub trait MemoryPort: Send + Sync {
    fn recall_for_model(&self, query: &str, is_local_provider: bool) -> Result<Vec<MemoryRecord>>;
    fn remember_episode(&self, text: &str) -> Result<()>;
    fn remember_explicit(&self, text: &str) -> Result<String>;
}
