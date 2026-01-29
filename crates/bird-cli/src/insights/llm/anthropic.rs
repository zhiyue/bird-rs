//! Anthropic Claude LLM provider implementation.

use super::{LlmError, LlmProvider, LlmRequest, LlmResponse, LlmResult};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};

/// Default model to use for Anthropic API.
pub const DEFAULT_MODEL: &str = "claude-sonnet-4-20250514";

/// Anthropic API base URL.
const API_BASE_URL: &str = "https://api.anthropic.com/v1/messages";

/// Anthropic Claude LLM provider.
pub struct AnthropicProvider {
    client: Client,
    api_key: String,
    model: String,
}

impl AnthropicProvider {
    /// Create a new Anthropic provider.
    pub fn new(api_key: String, model: Option<String>) -> Self {
        Self {
            client: Client::new(),
            api_key,
            model: model.unwrap_or_else(|| DEFAULT_MODEL.to_string()),
        }
    }

    /// Create a provider from environment variables.
    pub fn from_env(model: Option<String>) -> LlmResult<Self> {
        let api_key = std::env::var("ANTHROPIC_API_KEY").map_err(|_| {
            LlmError::ApiKeyMissing(
                "ANTHROPIC_API_KEY environment variable not set. \
                Get your API key at https://console.anthropic.com/"
                    .to_string(),
            )
        })?;

        Ok(Self::new(api_key, model))
    }
}

/// Anthropic API request format.
#[derive(Debug, Serialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    messages: Vec<AnthropicMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
}

#[derive(Debug, Serialize)]
struct AnthropicMessage {
    role: String,
    content: String,
}

/// Anthropic API response format.
#[derive(Debug, Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicContent>,
    model: String,
    usage: Option<AnthropicUsage>,
}

#[derive(Debug, Deserialize)]
struct AnthropicContent {
    #[serde(rename = "type")]
    content_type: String,
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AnthropicUsage {
    input_tokens: u32,
    output_tokens: u32,
}

/// Anthropic API error response.
#[derive(Debug, Deserialize)]
struct AnthropicErrorResponse {
    error: AnthropicError,
}

#[derive(Debug, Deserialize)]
struct AnthropicError {
    #[serde(rename = "type")]
    error_type: String,
    message: String,
}

#[async_trait]
impl LlmProvider for AnthropicProvider {
    async fn complete(&self, request: LlmRequest) -> LlmResult<LlmResponse> {
        let api_request = AnthropicRequest {
            model: self.model.clone(),
            max_tokens: request.max_tokens,
            system: if request.system.is_empty() {
                None
            } else {
                Some(request.system)
            },
            messages: vec![AnthropicMessage {
                role: "user".to_string(),
                content: request.user,
            }],
            temperature: Some(request.temperature),
        };

        let response = self
            .client
            .post(API_BASE_URL)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&api_request)
            .send()
            .await?;

        let status = response.status();

        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(LlmError::RateLimited(
                "Rate limited by Anthropic API. Please try again later.".to_string(),
            ));
        }

        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            if let Ok(error_response) = serde_json::from_str::<AnthropicErrorResponse>(&error_text)
            {
                return Err(LlmError::ApiError(format!(
                    "{}: {}",
                    error_response.error.error_type, error_response.error.message
                )));
            }
            return Err(LlmError::ApiError(format!(
                "HTTP {}: {}",
                status, error_text
            )));
        }

        let api_response: AnthropicResponse = response
            .json()
            .await
            .map_err(|e| LlmError::ParseError(e.to_string()))?;

        // Extract text from response
        let content = api_response
            .content
            .into_iter()
            .filter_map(|c| {
                if c.content_type == "text" {
                    c.text
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("");

        Ok(LlmResponse {
            content,
            model: api_response.model,
            input_tokens: api_response.usage.as_ref().map(|u| u.input_tokens),
            output_tokens: api_response.usage.as_ref().map(|u| u.output_tokens),
        })
    }

    fn name(&self) -> &'static str {
        "Anthropic"
    }

    fn model(&self) -> &str {
        &self.model
    }
}
