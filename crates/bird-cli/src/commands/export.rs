//! Export command implementation for exporting tweets to files.

use crate::cli::Cli;
use bird_client::TweetData;
use chrono::{DateTime, Local, NaiveDate};
use std::collections::{BTreeMap, HashSet};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

/// Supported export formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Jsonl,
    Json,
    Md,
}

impl std::str::FromStr for ExportFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "jsonl" => Ok(ExportFormat::Jsonl),
            "json" => Ok(ExportFormat::Json),
            "md" | "markdown" => Ok(ExportFormat::Md),
            _ => Err(format!(
                "Unknown format '{}'. Use: jsonl, json, or md",
                s
            )),
        }
    }
}

impl ExportFormat {
    fn extension(&self) -> &'static str {
        match self {
            ExportFormat::Jsonl => "jsonl",
            ExportFormat::Json => "json",
            ExportFormat::Md => "md",
        }
    }
}

/// Supported grouping modes for export.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupBy {
    Day,
    Month,
}

impl std::str::FromStr for GroupBy {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "day" | "daily" => Ok(GroupBy::Day),
            "month" | "monthly" => Ok(GroupBy::Month),
            _ => Err(format!(
                "Unknown group-by '{}'. Use: day or month",
                s
            )),
        }
    }
}

/// Run the export command.
pub async fn run(
    cli: &Cli,
    collection: &str,
    format: &str,
    output: Option<&str>,
    group_by: Option<&str>,
) -> anyhow::Result<()> {
    // Validate collection name
    let valid_collections = ["bookmarks", "likes", "user_tweets"];
    if !valid_collections.contains(&collection) {
        anyhow::bail!(
            "Unknown collection '{}'. Use: bookmarks, likes, or user_tweets",
            collection
        );
    }

    // Parse format
    let fmt: ExportFormat = format.parse().map_err(|e: String| anyhow::anyhow!(e))?;

    // Parse group-by
    let grouping: Option<GroupBy> = group_by
        .map(|g| g.parse().map_err(|e: String| anyhow::anyhow!(e)))
        .transpose()?;

    // Cannot use --output with --group-by (grouped mode generates multiple files)
    if output.is_some() && grouping.is_some() {
        anyhow::bail!("Cannot use --output with --group-by (grouped mode generates multiple files)");
    }

    // Open storage and get user_id from sync state (works offline)
    let storage = cli.create_storage().await?;
    let user_id = storage
        .get_any_synced_user_id()
        .await?
        .ok_or_else(|| anyhow::anyhow!("No synced data found. Run 'bird sync' first."))?;

    // Fetch all tweets from the collection (no limit)
    let tweets = storage
        .get_tweets_by_collection(collection, &user_id, None, None)
        .await?;

    if tweets.is_empty() {
        println!(
            "No tweets found in '{}'. Run 'bird sync {}' first.",
            collection, collection
        );
        return Ok(());
    }

    match grouping {
        Some(group) => export_grouped(&tweets, collection, &fmt, group),
        None => export_single(&tweets, collection, &fmt, output),
    }
}

/// Export all tweets to a single file (original behavior).
fn export_single(
    tweets: &[TweetData],
    collection: &str,
    fmt: &ExportFormat,
    output: Option<&str>,
) -> anyhow::Result<()> {
    let output_path = match output {
        Some(p) => PathBuf::from(p),
        None => default_export_path(collection, fmt),
    };

    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let (total, new_count, skipped) = match fmt {
        ExportFormat::Jsonl => export_jsonl(tweets, &output_path)?,
        ExportFormat::Json => export_json(tweets, &output_path)?,
        ExportFormat::Md => export_md(tweets, &output_path)?,
    };

    let path_display = output_path.display();
    match fmt {
        ExportFormat::Jsonl => {
            println!(
                "Exported {} tweets to {} ({} new, {} skipped)",
                total, path_display, new_count, skipped
            );
        }
        _ => {
            println!("Exported {} tweets to {}", total, path_display);
        }
    }

    Ok(())
}

/// Export tweets grouped by day or month into separate files.
fn export_grouped(
    tweets: &[TweetData],
    collection: &str,
    fmt: &ExportFormat,
    group: GroupBy,
) -> anyhow::Result<()> {
    let base_dir = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".bird")
        .join("exports")
        .join(collection);

    // Group tweets by time key
    let mut groups: BTreeMap<String, Vec<&TweetData>> = BTreeMap::new();
    let mut no_date_tweets: Vec<&TweetData> = Vec::new();

    for tweet in tweets {
        match extract_group_key(&tweet.created_at, group) {
            Some(key) => groups.entry(key).or_default().push(tweet),
            None => no_date_tweets.push(tweet),
        }
    }

    let mut total_exported = 0usize;
    let mut files_written = 0usize;

    // Export each group
    for (key, group_tweets) in &groups {
        let file_path = base_dir.join(format!("{}.{}", key, fmt.extension()));
        if let Some(parent) = file_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let count = group_tweets.len();
        let tweet_data: Vec<TweetData> = group_tweets.iter().map(|t| (*t).clone()).collect();

        match fmt {
            ExportFormat::Jsonl => { export_jsonl(&tweet_data, &file_path)?; }
            ExportFormat::Json => { export_json(&tweet_data, &file_path)?; }
            ExportFormat::Md => { export_md(&tweet_data, &file_path)?; }
        }

        total_exported += count;
        files_written += 1;
    }

    // Export tweets without date to "unknown" file
    if !no_date_tweets.is_empty() {
        let file_path = base_dir.join(format!("unknown.{}", fmt.extension()));
        if let Some(parent) = file_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let count = no_date_tweets.len();
        let tweet_data: Vec<TweetData> = no_date_tweets.iter().map(|t| (*t).clone()).collect();

        match fmt {
            ExportFormat::Jsonl => { export_jsonl(&tweet_data, &file_path)?; }
            ExportFormat::Json => { export_json(&tweet_data, &file_path)?; }
            ExportFormat::Md => { export_md(&tweet_data, &file_path)?; }
        }

        total_exported += count;
        files_written += 1;
    }

    let group_label = match group {
        GroupBy::Day => "day",
        GroupBy::Month => "month",
    };

    println!(
        "Exported {} tweets to {} files (grouped by {}) in {}",
        total_exported,
        files_written,
        group_label,
        base_dir.display()
    );

    Ok(())
}

/// Extract a grouping key from a tweet's created_at timestamp.
fn extract_group_key(created_at: &Option<String>, group: GroupBy) -> Option<String> {
    let ts = created_at.as_ref()?;

    // Twitter format: "Wed Oct 10 20:19:24 +0000 2018"
    let dt = DateTime::parse_from_str(ts, "%a %b %d %H:%M:%S %z %Y").ok()?;
    let date: NaiveDate = dt.date_naive();

    Some(match group {
        GroupBy::Day => date.format("%Y-%m-%d").to_string(),
        GroupBy::Month => date.format("%Y-%m").to_string(),
    })
}

/// Export tweets in JSONL format with incremental support.
fn export_jsonl(
    tweets: &[TweetData],
    path: &PathBuf,
) -> anyhow::Result<(usize, usize, usize)> {
    // Read existing IDs from the file (incremental)
    let existing_ids = read_existing_jsonl_ids(path)?;

    let mut new_count = 0usize;
    let mut skipped = 0usize;

    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;

    for tweet in tweets {
        if existing_ids.contains(&tweet.id) {
            skipped += 1;
        } else {
            let line = serde_json::to_string(tweet)?;
            writeln!(file, "{}", line)?;
            new_count += 1;
        }
    }

    let total = new_count + skipped;
    Ok((total, new_count, skipped))
}

/// Read tweet IDs from an existing JSONL file.
fn read_existing_jsonl_ids(path: &PathBuf) -> anyhow::Result<HashSet<String>> {
    let mut ids = HashSet::new();

    if !path.exists() {
        return Ok(ids);
    }

    let file = std::fs::File::open(path)?;
    let reader = BufReader::new(file);

    for line in reader.lines() {
        let line = line?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // Parse just enough to get the id field
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(line) {
            if let Some(id) = value.get("id").and_then(|v| v.as_str()) {
                ids.insert(id.to_string());
            }
        }
    }

    Ok(ids)
}

/// Export tweets as a pretty-printed JSON array (always overwrites).
fn export_json(
    tweets: &[TweetData],
    path: &PathBuf,
) -> anyhow::Result<(usize, usize, usize)> {
    let json = serde_json::to_string_pretty(tweets)?;
    std::fs::write(path, json)?;
    let total = tweets.len();
    Ok((total, total, 0))
}

/// Export tweets as Markdown (always overwrites).
fn export_md(
    tweets: &[TweetData],
    path: &PathBuf,
) -> anyhow::Result<(usize, usize, usize)> {
    let mut file = std::fs::File::create(path)?;

    writeln!(file, "# Exported Tweets")?;
    writeln!(file)?;

    for tweet in tweets {
        let author = &tweet.author;
        let date = format_timestamp(&tweet.created_at);
        let link = format!(
            "https://x.com/{}/status/{}",
            author.username, tweet.id
        );

        writeln!(file, "## @{} - {}", author.username, date)?;
        writeln!(file)?;
        writeln!(file, "{}", tweet.text)?;
        writeln!(file)?;
        writeln!(file, "[Link]({})", link)?;
        writeln!(file)?;
        writeln!(file, "---")?;
        writeln!(file)?;
    }

    let total = tweets.len();
    Ok((total, total, 0))
}

/// Format a Twitter timestamp for display.
fn format_timestamp(created_at: &Option<String>) -> String {
    match created_at {
        Some(ts) => {
            match DateTime::parse_from_str(ts, "%a %b %d %H:%M:%S %z %Y") {
                Ok(dt) => {
                    let local: DateTime<Local> = dt.with_timezone(&Local);
                    local.format("%Y-%m-%d %H:%M").to_string()
                }
                Err(_) => ts.clone(),
            }
        }
        None => "unknown date".to_string(),
    }
}

/// Get the default export path: ~/.bird/exports/<collection>.<ext>
fn default_export_path(collection: &str, format: &ExportFormat) -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".bird")
        .join("exports")
        .join(format!("{}.{}", collection, format.extension()))
}
