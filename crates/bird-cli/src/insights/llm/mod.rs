//! LLM provider abstraction for insights generation.

pub mod anthropic;
pub mod claude_code;

use async_trait::async_trait;
use thiserror::Error;

/// Errors that can occur during LLM operations.
#[derive(Debug, Error)]
pub enum LlmError {
    #[error("API key not configured: {0}")]
    ApiKeyMissing(String),

    #[error("HTTP request failed: {0}")]
    HttpError(#[from] reqwest::Error),

    #[error("Failed to parse response: {0}")]
    ParseError(String),

    #[error("API error: {0}")]
    ApiError(String),

    #[error("Rate limited: {0}")]
    RateLimited(String),
}

/// Result type for LLM operations.
pub type LlmResult<T> = Result<T, LlmError>;

/// Request to send to an LLM provider.
#[derive(Debug, Clone)]
pub struct LlmRequest {
    /// System prompt to set context.
    pub system: String,
    /// User message/prompt.
    pub user: String,
    /// Maximum tokens to generate.
    pub max_tokens: u32,
    /// Temperature for sampling (0.0-1.0).
    pub temperature: f32,
}

impl Default for LlmRequest {
    fn default() -> Self {
        Self {
            system: String::new(),
            user: String::new(),
            max_tokens: 4096,
            temperature: 0.7,
        }
    }
}

/// Response from an LLM provider.
#[derive(Debug, Clone)]
pub struct LlmResponse {
    /// Generated text content.
    pub content: String,
    /// Model used for generation.
    pub model: String,
    /// Input tokens used.
    pub input_tokens: Option<u32>,
    /// Output tokens generated.
    pub output_tokens: Option<u32>,
}

/// Trait for LLM providers.
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Send a completion request to the LLM.
    async fn complete(&self, request: LlmRequest) -> LlmResult<LlmResponse>;

    /// Get the provider name.
    fn name(&self) -> &'static str;

    /// Get the model being used.
    fn model(&self) -> &str;
}
