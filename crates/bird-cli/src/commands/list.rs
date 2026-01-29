//! List command implementation for browsing tweets from the local database.

use crate::cli::Cli;
use crate::output::format_json;
use bird_client::{CurrentUserResult, TweetData};
use chrono::{DateTime, Local};
use colored::Colorize;

const DEFAULT_PAGE_SIZE: u32 = 20;

/// Run the list command.
pub async fn run(
    cli: &Cli,
    collection: &str,
    page: u32,
    page_size: Option<u32>,
    show_headline: bool,
    show_emoji: bool,
) -> anyhow::Result<()> {
    let storage = cli.create_storage().await?;
    let mut client = cli.create_client()?;

    // Get current user ID
    let user_id = match client.get_current_user().await {
        CurrentUserResult::Success(user) => user.id,
        CurrentUserResult::Error(e) => {
            anyhow::bail!("Failed to get current user: {}", e);
        }
    };

    let size = page_size.unwrap_or(DEFAULT_PAGE_SIZE);
    let offset = (page.saturating_sub(1)) * size;

    // Get total count
    let total = storage.collection_count(collection, &user_id).await?;

    // Get tweets for this page
    let tweets = storage
        .get_tweets_by_collection(collection, &user_id, Some(size), Some(offset))
        .await?;

    if cli.json() {
        println!("{}", format_json(&tweets));
        return Ok(());
    }

    if tweets.is_empty() {
        if page > 1 {
            println!("No tweets on page {}.", page);
        } else {
            println!("No {} found in database.", collection);
        }
        return Ok(());
    }

    // Print table header
    print_table_header(show_headline, show_emoji);

    // Print each tweet as a row
    for tweet in &tweets {
        print_tweet_row(tweet, show_headline, show_emoji);
    }

    // Print pagination info
    let total_pages = (total as f64 / size as f64).ceil() as u64;
    println!();
    println!(
        "{}",
        format!("Page {}/{} ({} total tweets)", page, total_pages, total).dimmed()
    );

    if page > 1 || (page as u64) < total_pages {
        let nav_hint = if show_emoji { "📖 " } else { "" };
        let mut nav_parts = Vec::new();
        if page > 1 {
            nav_parts.push(format!("--page {}", page - 1));
        }
        if (page as u64) < total_pages {
            nav_parts.push(format!("--page {}", page + 1));
        }
        println!("{}Navigate: {}", nav_hint, nav_parts.join(" | ").dimmed());
    }

    Ok(())
}

/// Print the table header.
fn print_table_header(show_headline: bool, show_emoji: bool) {
    let id_header = "ID";
    let text_header = "Text";
    let time_header = "Time";

    let icon = if show_emoji { "📋 " } else { "" };
    if show_headline {
        println!(
            "{}{:<20} {:<40} {:<30} {}",
            icon,
            id_header.bold(),
            text_header.bold(),
            "Headline".bold(),
            time_header.bold()
        );
        println!("{}", "─".repeat(110).dimmed());
    } else {
        println!(
            "{}{:<20} {:<50} {}",
            icon,
            id_header.bold(),
            text_header.bold(),
            time_header.bold()
        );
        println!("{}", "─".repeat(90).dimmed());
    }
}

/// Print a single tweet as a table row.
fn print_tweet_row(tweet: &TweetData, show_headline: bool, _show_emoji: bool) {
    let id = &tweet.id;

    // Format timestamp
    let time_str = format_timestamp(&tweet.created_at);

    if show_headline {
        // Truncate text to 37 chars + "..."
        let text = tweet.text.replace('\n', " ");
        let truncated_text = if text.len() > 37 {
            format!("{}...", &text[..37])
        } else {
            text
        };

        // Format headline (truncate to 27 chars + "...")
        let headline = tweet
            .headline
            .as_ref()
            .map(|h| {
                let h = h.replace('\n', " ");
                if h.len() > 27 {
                    format!("{}...", &h[..27])
                } else {
                    h
                }
            })
            .unwrap_or_else(|| "-".to_string());

        println!(
            "{:<20} {:<40} {:<30} {}",
            id.cyan(),
            truncated_text,
            headline.yellow(),
            time_str.dimmed()
        );
    } else {
        // Truncate text to 47 chars + "..."
        let text = tweet.text.replace('\n', " ");
        let truncated_text = if text.len() > 47 {
            format!("{}...", &text[..47])
        } else {
            text
        };

        println!(
            "{:<20} {:<50} {}",
            id.cyan(),
            truncated_text,
            time_str.dimmed()
        );
    }
}

/// Format a Twitter timestamp to human-readable format (e.g., "2026/01/28 7:44am").
fn format_timestamp(created_at: &Option<String>) -> String {
    match created_at {
        Some(ts) => {
            // Twitter format: "Wed Jan 28 02:28:48 +0000 2026"
            match DateTime::parse_from_str(ts, "%a %b %d %H:%M:%S %z %Y") {
                Ok(dt) => {
                    // Convert to local time
                    let local: DateTime<Local> = dt.with_timezone(&Local);
                    local.format("%Y/%m/%d %-I:%M%p").to_string().to_lowercase()
                }
                Err(_) => ts.clone(),
            }
        }
        None => "unknown".to_string(),
    }
}
