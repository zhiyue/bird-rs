//! List command implementation for browsing tweets from the local database.

use crate::cli::Cli;
use crate::output::format_json;
use bird_client::{CurrentUserResult, TweetData};
use bird_storage::ResonanceScore;
use chrono::{DateTime, Local};
use colored::Colorize;
use serde::Serialize;
use std::collections::HashMap;

const DEFAULT_PAGE_SIZE: u32 = 20;

/// Available columns for the list command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Column {
    Id,
    Text,
    Time,
    Author,
    Liked,
    Bookmarked,
    Score,
    Headline,
    Collections,
}

impl Column {
    fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "id" => Some(Column::Id),
            "text" => Some(Column::Text),
            "time" => Some(Column::Time),
            "author" => Some(Column::Author),
            "liked" => Some(Column::Liked),
            "bookmarked" => Some(Column::Bookmarked),
            "score" => Some(Column::Score),
            "headline" => Some(Column::Headline),
            "collections" => Some(Column::Collections),
            _ => None,
        }
    }

    fn header(&self) -> &'static str {
        match self {
            Column::Id => "ID",
            Column::Text => "Text",
            Column::Time => "Time",
            Column::Author => "Author",
            Column::Liked => "Liked",
            Column::Bookmarked => "Bookmarked",
            Column::Score => "Score",
            Column::Headline => "Headline",
            Column::Collections => "Collections",
        }
    }

    fn width(&self) -> usize {
        match self {
            Column::Id => 20,
            Column::Text => 40,
            Column::Time => 18,
            Column::Author => 15,
            Column::Liked => 5,
            Column::Bookmarked => 10,
            Column::Score => 6,
            Column::Headline => 30,
            Column::Collections => 12,
        }
    }

    /// Returns true if this column requires resonance score data.
    fn needs_resonance(&self) -> bool {
        matches!(self, Column::Liked | Column::Bookmarked | Column::Score)
    }
}

/// Parse columns from a list of strings.
fn parse_columns(columns: Option<Vec<String>>) -> Result<Vec<Column>, String> {
    match columns {
        Some(cols) => {
            let mut result = Vec::new();
            for col in cols {
                match Column::from_str(&col) {
                    Some(c) => result.push(c),
                    None => {
                        return Err(format!(
                            "Unknown column '{}'. Available: id, text, time, author, liked, bookmarked, score, headline",
                            col
                        ))
                    }
                }
            }
            if result.is_empty() {
                Ok(default_columns())
            } else {
                Ok(result)
            }
        }
        None => Ok(default_columns()),
    }
}

fn default_columns() -> Vec<Column> {
    vec![Column::Id, Column::Text, Column::Time]
}

/// JSON output with optional resonance data.
#[derive(Serialize)]
struct TweetWithResonance {
    #[serde(flatten)]
    tweet: TweetData,
    #[serde(skip_serializing_if = "Option::is_none")]
    resonance: Option<ResonanceJson>,
    #[serde(skip_serializing_if = "Option::is_none")]
    collections: Option<Vec<String>>,
}

#[derive(Serialize)]
struct ResonanceJson {
    liked: bool,
    bookmarked: bool,
    score: f64,
}

/// Run the list command.
pub async fn run(
    cli: &Cli,
    collection: &str,
    page: u32,
    page_size: Option<u32>,
    show_headline: bool,
    columns: Option<Vec<String>>,
    show_emoji: bool,
) -> anyhow::Result<()> {
    // Parse columns
    let mut cols = parse_columns(columns).map_err(|e| anyhow::anyhow!(e))?;

    // If show_headline is set and headline not in columns, add it
    if show_headline && !cols.contains(&Column::Headline) {
        cols.push(Column::Headline);
    }

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

    // Check if we need resonance data
    let needs_resonance = cols.iter().any(|c| c.needs_resonance());
    let resonance_map: HashMap<String, ResonanceScore> = if needs_resonance {
        let tweet_ids: Vec<&str> = tweets.iter().map(|t| t.id.as_str()).collect();
        let mut map = HashMap::new();
        for id in tweet_ids {
            if let Ok(Some(score)) = storage.get_resonance_score(id, &user_id).await {
                map.insert(id.to_string(), score);
            }
        }
        map
    } else {
        HashMap::new()
    };

    // Check if we need collections data
    let needs_collections = cols.iter().any(|c| matches!(c, Column::Collections));
    let collections_map: HashMap<String, Vec<String>> = if needs_collections {
        let mut map = HashMap::new();
        for tweet in &tweets {
            let mut tweet_collections = Vec::new();
            // Check all known collections for this tweet
            for coll in &["likes", "bookmarks", "user_tweets"] {
                if storage.is_in_collection(&tweet.id, coll, &user_id).await? {
                    tweet_collections.push(coll.to_string());
                }
            }
            map.insert(tweet.id.clone(), tweet_collections);
        }
        map
    } else {
        HashMap::new()
    };

    if cli.json() {
        let results: Vec<TweetWithResonance> = tweets
            .iter()
            .map(|t| {
                let resonance = resonance_map.get(&t.id).map(|s| ResonanceJson {
                    liked: s.liked,
                    bookmarked: s.bookmarked,
                    score: s.total,
                });
                let collections = collections_map.get(&t.id).cloned();
                TweetWithResonance {
                    tweet: t.clone(),
                    resonance,
                    collections,
                }
            })
            .collect();
        println!("{}", format_json(&results));
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
    print_table_header(&cols, show_emoji);

    // Print each tweet as a row
    for tweet in &tweets {
        let resonance = resonance_map.get(&tweet.id);
        let tweet_collections = collections_map.get(&tweet.id);
        print_tweet_row(tweet, resonance, tweet_collections, &cols, show_emoji);
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
fn print_table_header(cols: &[Column], show_emoji: bool) {
    let icon = if show_emoji { "📋 " } else { "" };

    let headers: Vec<String> = cols
        .iter()
        .map(|c| format!("{:<width$}", c.header().bold(), width = c.width()))
        .collect();

    println!("{}{}", icon, headers.join(" "));

    let total_width: usize = cols.iter().map(|c| c.width() + 1).sum();
    println!("{}", "─".repeat(total_width).dimmed());
}

/// Print a single tweet as a table row.
fn print_tweet_row(
    tweet: &TweetData,
    resonance: Option<&ResonanceScore>,
    collections: Option<&Vec<String>>,
    cols: &[Column],
    show_emoji: bool,
) {
    let values: Vec<String> = cols
        .iter()
        .map(|c| format_column(tweet, resonance, collections, c, show_emoji))
        .collect();

    println!("{}", values.join(" "));
}

/// Truncate a string to a maximum number of characters (not bytes).
fn truncate_str(s: &str, max_chars: usize) -> String {
    let char_count = s.chars().count();
    if char_count > max_chars {
        let truncated: String = s.chars().take(max_chars).collect();
        format!("{}...", truncated)
    } else {
        s.to_string()
    }
}

/// Format a single column value.
fn format_column(
    tweet: &TweetData,
    resonance: Option<&ResonanceScore>,
    collections: Option<&Vec<String>>,
    col: &Column,
    show_emoji: bool,
) -> String {
    let width = col.width();

    match col {
        Column::Id => format!("{:<width$}", tweet.id.cyan(), width = width),

        Column::Text => {
            let text = tweet.text.replace('\n', " ");
            let max_len = width.saturating_sub(3);
            let truncated = truncate_str(&text, max_len);
            format!("{:<width$}", truncated, width = width)
        }

        Column::Time => {
            let time_str = format_timestamp(&tweet.created_at);
            format!("{:<width$}", time_str.dimmed(), width = width)
        }

        Column::Author => {
            let author = format!("@{}", tweet.author.username);
            let max_len = width.saturating_sub(3);
            let truncated = truncate_str(&author, max_len);
            format!("{:<width$}", truncated.cyan(), width = width)
        }

        Column::Liked => {
            let liked = resonance.map(|r| r.liked).unwrap_or(false);
            let display = if liked {
                if show_emoji {
                    "❤️".to_string()
                } else {
                    "Y".to_string()
                }
            } else {
                "-".to_string()
            };
            format!("{:<width$}", display, width = width)
        }

        Column::Bookmarked => {
            let bookmarked = resonance.map(|r| r.bookmarked).unwrap_or(false);
            let display = if bookmarked {
                if show_emoji {
                    "🔖".to_string()
                } else {
                    "Y".to_string()
                }
            } else {
                "-".to_string()
            };
            format!("{:<width$}", display, width = width)
        }

        Column::Score => {
            let score = resonance.map(|r| r.total).unwrap_or(0.0);
            let display = if score > 0.0 {
                format!("{:.2}", score).green().to_string()
            } else {
                "-".to_string()
            };
            format!("{:<width$}", display, width = width)
        }

        Column::Headline => {
            let headline = tweet
                .headline
                .as_ref()
                .map(|h| {
                    let h = h.replace('\n', " ");
                    let max_len = width.saturating_sub(3);
                    truncate_str(&h, max_len)
                })
                .unwrap_or_else(|| "-".to_string());
            format!("{:<width$}", headline.yellow(), width = width)
        }

        Column::Collections => {
            let collections_str = collections
                .map(|colls| {
                    if show_emoji {
                        // Use emoji badges for each collection
                        let badges: Vec<&str> = colls
                            .iter()
                            .map(|c| match c.as_str() {
                                "likes" => "❤️",
                                "bookmarks" => "🔖",
                                "user_tweets" => "📝",
                                _ => "•",
                            })
                            .collect();
                        badges.join("")
                    } else {
                        // Use single-letter abbreviations
                        let abbrevs: Vec<&str> = colls
                            .iter()
                            .map(|c| match c.as_str() {
                                "likes" => "L",
                                "bookmarks" => "B",
                                "user_tweets" => "U",
                                _ => "?",
                            })
                            .collect();
                        abbrevs.join(",")
                    }
                })
                .unwrap_or_else(|| "-".to_string());
            format!("{:<width$}", collections_str, width = width)
        }
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
