//! CLI interface for bird.

use crate::cookies::{check_available_sources, resolve_credentials};
use crate::output::{format_current_user, format_json, format_tweet};
use crate::types::CurrentUserResult;
use crate::{TwitterClient, TwitterClientOptions};
use clap::{Parser, Subcommand};

/// A fast X/Twitter CLI for reading tweets.
#[derive(Parser)]
#[command(name = "bird")]
#[command(author, version, about, long_about = None)]
#[command(after_help = "Examples:
  bird whoami              Show the logged-in account
  bird read 1234567890     Read a tweet by ID
  bird 1234567890          Shorthand for read
  bird check               Show available credential sources")]
pub struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Twitter auth_token cookie (overrides browser extraction).
    #[arg(long, global = true, env = "AUTH_TOKEN")]
    auth_token: Option<String>,

    /// Twitter ct0 cookie (overrides browser extraction).
    #[arg(long, global = true, env = "CT0")]
    ct0: Option<String>,

    /// Request timeout in milliseconds.
    #[arg(long, global = true, default_value = "30000")]
    timeout: u64,

    /// Max quoted tweet depth (0 disables).
    #[arg(long, global = true, default_value = "1")]
    quote_depth: u32,

    /// Output as JSON.
    #[arg(long, global = true)]
    json: bool,

    /// Plain output (no emoji, no color).
    #[arg(long, global = true)]
    plain: bool,

    /// Disable emoji output.
    #[arg(long, global = true)]
    no_emoji: bool,

    /// Tweet ID or URL (shorthand for `read`).
    #[arg(value_name = "TWEET_ID_OR_URL")]
    tweet_id: Option<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// Show the logged-in account.
    Whoami,

    /// Check available credential sources.
    Check,

    /// Read a tweet by ID or URL.
    Read {
        /// Tweet ID or URL.
        tweet_id: String,

        /// Include raw API response in output.
        #[arg(long)]
        json_full: bool,
    },
}

impl Cli {
    /// Run the CLI.
    pub async fn run(self) -> anyhow::Result<()> {
        let show_emoji = !self.plain && !self.no_emoji;

        // Handle commands
        match &self.command {
            Some(Commands::Check) => self.run_check(),
            Some(Commands::Whoami) => self.run_whoami(show_emoji).await,
            Some(Commands::Read {
                tweet_id,
                json_full,
            }) => self.run_read(tweet_id, *json_full, show_emoji).await,
            None => {
                // Check for shorthand tweet ID
                if let Some(tweet_id) = &self.tweet_id {
                    return self.run_read(tweet_id, false, show_emoji).await;
                }

                // No command provided, show help
                use clap::CommandFactory;
                let mut cmd = Cli::command();
                cmd.print_help()?;
                Ok(())
            }
        }
    }

    /// Run the check command.
    fn run_check(&self) -> anyhow::Result<()> {
        let sources = check_available_sources();

        if sources.is_empty() {
            println!("No credential sources available.");
            println!();
            println!("To authenticate, either:");
            println!("  1. Log in to x.com in Safari");
            println!("  2. Set AUTH_TOKEN and CT0 environment variables");
            println!("  3. Pass --auth-token and --ct0 flags");
        } else {
            println!("Available credential sources:");
            for source in sources {
                println!("  - {}", source);
            }
        }

        Ok(())
    }

    /// Run the whoami command.
    async fn run_whoami(&self, show_emoji: bool) -> anyhow::Result<()> {
        let cookies = resolve_credentials(self.auth_token.as_deref(), self.ct0.as_deref(), &[])?;

        let mut client = TwitterClient::new(TwitterClientOptions {
            cookies,
            timeout_ms: Some(self.timeout),
            quote_depth: Some(self.quote_depth),
        });

        match client.get_current_user().await {
            CurrentUserResult::Success(user) => {
                if self.json {
                    println!("{}", format_json(&user));
                } else {
                    println!("{}", format_current_user(&user, show_emoji));
                }
            }
            CurrentUserResult::Error(e) => {
                anyhow::bail!("Failed to get current user: {}", e);
            }
        }

        Ok(())
    }

    /// Run the read command.
    async fn run_read(
        &self,
        tweet_id: &str,
        _json_full: bool,
        show_emoji: bool,
    ) -> anyhow::Result<()> {
        let cookies = resolve_credentials(self.auth_token.as_deref(), self.ct0.as_deref(), &[])?;

        let client = TwitterClient::new(TwitterClientOptions {
            cookies,
            timeout_ms: Some(self.timeout),
            quote_depth: Some(self.quote_depth),
        });

        // Extract tweet ID from URL if needed
        let id = extract_tweet_id(tweet_id)?;

        let tweet = client.get_tweet(&id).await?;

        if self.json {
            println!("{}", format_json(&tweet));
        } else {
            print!("{}", format_tweet(&tweet, show_emoji));
        }

        Ok(())
    }
}

/// Extract a tweet ID from a URL or return the ID directly.
fn extract_tweet_id(input: &str) -> anyhow::Result<String> {
    // If it looks like a URL, try to parse it
    if input.contains('/') || input.contains("twitter.com") || input.contains("x.com") {
        // Try to extract ID from various URL formats:
        // https://twitter.com/user/status/1234567890
        // https://x.com/user/status/1234567890
        // twitter.com/user/status/1234567890/...

        let url = if input.starts_with("http") {
            input.to_string()
        } else {
            format!("https://{}", input)
        };

        if let Ok(parsed) = url::Url::parse(&url) {
            let segments: Vec<&str> = parsed
                .path_segments()
                .map(|s| s.collect())
                .unwrap_or_default();

            // Look for "status" followed by an ID
            for (i, segment) in segments.iter().enumerate() {
                if *segment == "status" && i + 1 < segments.len() {
                    let id = segments[i + 1];
                    // Validate it looks like an ID
                    if id.chars().all(|c| c.is_ascii_digit()) {
                        return Ok(id.to_string());
                    }
                }
            }
        }

        anyhow::bail!("Could not extract tweet ID from URL: {}", input);
    }

    // Validate the ID is numeric
    if !input.chars().all(|c| c.is_ascii_digit()) {
        anyhow::bail!("Invalid tweet ID: {}", input);
    }

    Ok(input.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_tweet_id_bare() {
        assert_eq!(
            extract_tweet_id("1234567890123456789").unwrap(),
            "1234567890123456789"
        );
    }

    #[test]
    fn test_extract_tweet_id_x_url() {
        assert_eq!(
            extract_tweet_id("https://x.com/user/status/1234567890123456789").unwrap(),
            "1234567890123456789"
        );
    }

    #[test]
    fn test_extract_tweet_id_twitter_url() {
        assert_eq!(
            extract_tweet_id("https://twitter.com/user/status/1234567890123456789").unwrap(),
            "1234567890123456789"
        );
    }

    #[test]
    fn test_extract_tweet_id_without_https() {
        assert_eq!(
            extract_tweet_id("x.com/user/status/1234567890123456789").unwrap(),
            "1234567890123456789"
        );
    }

    #[test]
    fn test_extract_tweet_id_invalid() {
        assert!(extract_tweet_id("not-a-valid-id").is_err());
    }
}
