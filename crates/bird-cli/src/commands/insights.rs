//! Insights command implementation.

use crate::cli::Cli;
use crate::insights::llm::anthropic::AnthropicProvider;
use crate::insights::llm::claude_code::ClaudeCodeProvider;
use crate::insights::llm::LlmProvider;
use crate::insights::output::{format_insights, format_insights_json};
use crate::insights::{CollectionFilter, InsightsEngine, InsightsOptions, TimePeriod};
use bird_client::CurrentUserResult;
use colored::Colorize;

/// LLM provider options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlmProviderChoice {
    /// Claude Code CLI (uses MAX subscription).
    ClaudeCode,
    /// Anthropic API (requires ANTHROPIC_API_KEY).
    AnthropicApi,
}

impl std::str::FromStr for LlmProviderChoice {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "claude-code" | "claude" | "cc" => Ok(LlmProviderChoice::ClaudeCode),
            "anthropic-api" | "anthropic" | "api" => Ok(LlmProviderChoice::AnthropicApi),
            _ => Err(format!(
                "Unknown provider: {}. Use 'claude-code' or 'anthropic-api'.",
                s
            )),
        }
    }
}

/// Run the insights generate command.
#[allow(clippy::too_many_arguments)]
pub async fn run_generate(
    cli: &Cli,
    period: Option<String>,
    collection: Option<String>,
    max_tweets: Option<u32>,
    provider: String,
    model: Option<String>,
    verbose: bool,
    show_emoji: bool,
) -> anyhow::Result<()> {
    // Parse time period
    let period: TimePeriod = period
        .as_deref()
        .unwrap_or("week")
        .parse()
        .map_err(|e: String| anyhow::anyhow!(e))?;

    // Parse collection filter
    let collection_filter: CollectionFilter = collection
        .as_deref()
        .unwrap_or("all")
        .parse()
        .map_err(|e: String| anyhow::anyhow!(e))?;

    // Parse provider choice
    let provider_choice: LlmProviderChoice = provider
        .parse()
        .map_err(|e: String| anyhow::anyhow!(e))?;

    // Get model override from env if not specified
    let model = model.or_else(|| std::env::var("BIRD_LLM_MODEL").ok());

    // Create the appropriate LLM provider
    let llm: Box<dyn LlmProvider> = match provider_choice {
        LlmProviderChoice::ClaudeCode => {
            // Check if Claude Code is available
            if !ClaudeCodeProvider::is_available().await {
                anyhow::bail!(
                    "Claude Code CLI not found. Either:\n\
                     1. Install Claude Code: https://docs.anthropic.com/en/docs/claude-code\n\
                     2. Use --provider anthropic-api with ANTHROPIC_API_KEY"
                );
            }
            Box::new(ClaudeCodeProvider::new(model))
        }
        LlmProviderChoice::AnthropicApi => {
            Box::new(AnthropicProvider::from_env(model)?)
        }
    };

    // Get storage and user ID
    let storage = cli.create_storage().await?;
    let mut client = cli.create_client()?;

    let user_id = match client.get_current_user().await {
        CurrentUserResult::Success(user) => user.id,
        CurrentUserResult::Error(e) => {
            anyhow::bail!("Failed to get current user: {}", e);
        }
    };

    // Show progress
    if !cli.json() {
        let icon = if show_emoji { "  " } else { "" };
        let provider_info = match provider_choice {
            LlmProviderChoice::ClaudeCode => format!("{} (MAX subscription)", llm.name()),
            LlmProviderChoice::AnthropicApi => format!("{} (API)", llm.name()),
        };
        eprintln!(
            "{}Analyzing tweets from {} using {} {}...",
            icon,
            period.description().cyan(),
            provider_info.green(),
            llm.model().dimmed()
        );
        if verbose {
            eprintln!("  User ID: {}", user_id);
        }
    }

    // Build options
    let options = InsightsOptions {
        period,
        collection: collection_filter,
        max_tweets,
        verbose,
    };

    // Create engine and generate insights
    let engine = InsightsEngine::new(storage, llm);
    let result = engine.generate(&user_id, &options).await?;

    // Output results
    if cli.json() {
        println!("{}", format_insights_json(&result));
    } else {
        println!("{}", format_insights(&result, period, show_emoji));
    }

    Ok(())
}
