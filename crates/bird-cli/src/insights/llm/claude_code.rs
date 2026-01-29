//! Claude Code LLM provider - uses your existing MAX subscription via the `claude` CLI.

use super::{LlmError, LlmProvider, LlmRequest, LlmResponse, LlmResult};
use async_trait::async_trait;
use std::process::Stdio;
use tokio::process::Command;

/// Default model for Claude Code.
pub const DEFAULT_MODEL: &str = "sonnet";

/// Claude Code LLM provider - invokes the `claude` CLI.
/// This uses your existing Anthropic MAX subscription.
pub struct ClaudeCodeProvider {
    model: String,
}

impl ClaudeCodeProvider {
    /// Create a new Claude Code provider.
    pub fn new(model: Option<String>) -> Self {
        Self {
            model: model.unwrap_or_else(|| DEFAULT_MODEL.to_string()),
        }
    }

    /// Check if the `claude` CLI is available.
    pub async fn is_available() -> bool {
        Command::new("claude")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await
            .map(|s| s.success())
            .unwrap_or(false)
    }
}

#[async_trait]
impl LlmProvider for ClaudeCodeProvider {
    async fn complete(&self, request: LlmRequest) -> LlmResult<LlmResponse> {
        // Build the prompt - combine system and user prompts
        let prompt = if request.system.is_empty() {
            request.user
        } else {
            format!("{}\n\n---\n\n{}", request.system, request.user)
        };

        // Invoke claude CLI in print mode from /tmp to avoid loading project context
        let output = Command::new("claude")
            .arg("-p") // Print mode (non-interactive)
            .arg("--model")
            .arg(&self.model)
            .arg(&prompt)
            .current_dir(std::env::temp_dir()) // Run from temp dir to avoid project context
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    LlmError::ApiKeyMissing(
                        "Claude Code CLI not found. Install it from https://docs.anthropic.com/en/docs/claude-code".to_string()
                    )
                } else {
                    LlmError::ApiError(format!("Failed to invoke claude CLI: {}", e))
                }
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(LlmError::ApiError(format!(
                "Claude CLI failed: {}",
                stderr.trim()
            )));
        }

        let content = String::from_utf8_lossy(&output.stdout).trim().to_string();

        Ok(LlmResponse {
            content,
            model: self.model.clone(),
            input_tokens: None, // Claude CLI doesn't report tokens
            output_tokens: None,
        })
    }

    fn name(&self) -> &'static str {
        "Claude Code"
    }

    fn model(&self) -> &str {
        &self.model
    }
}
