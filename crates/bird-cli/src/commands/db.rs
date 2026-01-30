//! Database maintenance commands.

use crate::cli::Cli;
use crate::insights::headlines::generate_headlines;
use crate::insights::llm::claude_code::ClaudeCodeProvider;
use crate::insights::llm::LlmProvider;
use crate::output::format_json;
use crate::resonance::{get_interaction_stats, refresh_resonance_scores};
use bird_storage::{StorageConfig, SurrealDbStorage, TweetStore};
use chrono::{TimeZone, Utc};
use colored::Colorize;
use serde::Serialize;
use std::path::{Path, PathBuf};

/// Backfill created_at_ts values for existing tweets.
pub async fn run_backfill_created_at(
    cli: &Cli,
    batch_size: Option<u32>,
    show_emoji: bool,
) -> anyhow::Result<()> {
    let StorageConfig::SurrealDb(db_config) = cli.storage_config()? else {
        anyhow::bail!("Backfill requires SurrealDB storage");
    };

    let storage = SurrealDbStorage::new_with_config(&db_config).await?;
    let batch = batch_size.unwrap_or(200);
    let result = storage.backfill_created_at_ts(batch).await?;

    let icon = if show_emoji { "🛠️  " } else { "" };
    println!(
        "{}Backfill complete: {} updated, {} skipped",
        icon,
        result.updated.to_string().green().bold(),
        result.skipped.to_string().yellow()
    );

    Ok(())
}

/// Show database status and counts.
pub async fn run_status(cli: &Cli, show_emoji: bool, debug: bool) -> anyhow::Result<()> {
    let StorageConfig::SurrealDb(db_config) = cli.storage_config()? else {
        anyhow::bail!("Status requires SurrealDB storage");
    };

    let storage = SurrealDbStorage::new_with_config(&db_config).await?;
    let stats = storage.stats().await?;
    let oldest_tweet_at = format_timestamp(stats.oldest_tweet_ts);
    let newest_tweet_at = format_timestamp(stats.newest_tweet_ts);
    let storage_path = rocksdb_path_from_endpoint(&db_config.endpoint);
    let (storage_size_bytes, storage_error) = match storage_path.as_deref() {
        Some(path) => match directory_size_bytes(path) {
            Ok(size) => (Some(size), None),
            Err(err) => (None, Some(err.to_string())),
        },
        None => (None, None),
    };

    let auth_mode = match db_config.auth {
        Some(bird_storage::SurrealDbAuth::Root { .. }) => "root",
        Some(bird_storage::SurrealDbAuth::Namespace { .. }) => "namespace",
        Some(bird_storage::SurrealDbAuth::Database { .. }) => "database",
        None => "none",
    };

    if cli.json() {
        #[derive(Serialize)]
        struct DbStatusOutput<'a> {
            endpoint: &'a str,
            namespace: &'a str,
            database: &'a str,
            auth: &'a str,
            tweets: u64,
            collections: u64,
            bookmarks: u64,
            likes: u64,
            sync_states: u64,
            missing_created_at_ts: u64,
            #[serde(skip_serializing_if = "Option::is_none")]
            oldest_tweet_ts: Option<i64>,
            #[serde(skip_serializing_if = "Option::is_none")]
            newest_tweet_ts: Option<i64>,
            #[serde(skip_serializing_if = "Option::is_none")]
            oldest_tweet_at: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            newest_tweet_at: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            storage_path: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            storage_size_bytes: Option<u64>,
            #[serde(skip_serializing_if = "Option::is_none")]
            storage_error: Option<String>,
        }

        let output = DbStatusOutput {
            endpoint: &db_config.endpoint,
            namespace: &db_config.namespace,
            database: &db_config.database,
            auth: auth_mode,
            tweets: stats.tweets,
            collections: stats.collections,
            bookmarks: stats.bookmarks,
            likes: stats.likes,
            sync_states: stats.sync_states,
            missing_created_at_ts: stats.missing_created_at_ts,
            oldest_tweet_ts: stats.oldest_tweet_ts,
            newest_tweet_ts: stats.newest_tweet_ts,
            oldest_tweet_at,
            newest_tweet_at,
            storage_path: storage_path.map(|path| path.display().to_string()),
            storage_size_bytes,
            storage_error,
        };
        println!("{}", format_json(&output));
        return Ok(());
    }

    let icon = if show_emoji { "🗄️  " } else { "" };
    println!("{}Database Status", icon);
    println!("{}", "─".repeat(50));

    // Connection info section
    println!();
    println!("  {}", "Connection".bold());
    println!("  {:<22} {}", "Endpoint:", db_config.endpoint.cyan());
    println!("  {:<22} {}", "Namespace:", db_config.namespace.cyan());
    println!("  {:<22} {}", "Database:", db_config.database.cyan());
    println!("  {:<22} {}", "Auth:", auth_mode.cyan());

    // Counts section
    println!();
    println!("  {}", "Counts".bold());
    println!(
        "  {:<22} {:>8}",
        "Tweets:",
        stats.tweets.to_string().green().bold()
    );
    println!(
        "  {:<22} {:>8}",
        "Likes:",
        stats.likes.to_string().green().bold()
    );
    println!(
        "  {:<22} {:>8}",
        "Bookmarks:",
        stats.bookmarks.to_string().green().bold()
    );
    println!(
        "  {:<22} {:>8}",
        "Collections:",
        stats.collections.to_string().green().bold()
    );
    println!(
        "  {:<22} {:>8}",
        "Sync states:",
        stats.sync_states.to_string().green().bold()
    );
    let missing = stats.missing_created_at_ts;
    if missing > 0 {
        println!(
            "  {:<22} {:>8}",
            "Missing created_at_ts:",
            missing.to_string().yellow().bold()
        );
    }

    // Timeline section
    println!();
    println!("  {}", "Timeline".bold());
    match &oldest_tweet_at {
        Some(date) => println!("  {:<22} {}", "Oldest tweet:", date.cyan()),
        None => println!(
            "  {:<22} {}",
            "Oldest tweet:",
            "unknown (missing created_at_ts)".yellow()
        ),
    }
    match &newest_tweet_at {
        Some(date) => println!("  {:<22} {}", "Newest tweet:", date.cyan()),
        None => println!(
            "  {:<22} {}",
            "Newest tweet:",
            "unknown (missing created_at_ts)".yellow()
        ),
    }

    // Storage section
    println!();
    println!("  {}", "Storage".bold());
    match storage_path {
        Some(path) => {
            println!("  {:<22} {}", "Path:", path.display().to_string().cyan());
            match storage_size_bytes {
                Some(size) => {
                    let human = format_bytes(size);
                    println!(
                        "  {:<22} {} ({})",
                        "Size:",
                        human.green().bold(),
                        format!("{} bytes", size).dimmed()
                    );
                }
                None => {
                    let message = storage_error
                        .as_deref()
                        .unwrap_or("unavailable for this endpoint");
                    println!("  {:<22} {}", "Size:", message.yellow());
                }
            }
        }
        None => {
            println!(
                "  {:<22} {}",
                "Size:",
                "unavailable for remote endpoint".yellow()
            );
        }
    }

    // Debug: show timestamp distribution
    if debug {
        println!();
        println!("  {}", "Debug: Timestamp Analysis".bold());
        let info = storage.debug_timestamp_distribution().await?;

        println!(
            "  {:<26} {:>6}",
            "Tweets with NULL ts:",
            info.none_count.to_string().yellow()
        );
        println!(
            "  {:<26} {:>6}",
            "Tweets with ts=0:",
            info.zero_count.to_string().yellow()
        );
        println!(
            "  {:<26} {:>6}",
            "Tweets with valid ts:",
            info.valid_count.to_string().green()
        );
        println!(
            "  {:<26} {:>6}",
            "Distinct timestamps:",
            info.distinct_count.to_string().cyan()
        );
        println!();

        // Show actual MIN/MAX from math functions
        let format_ts = |ts: Option<i64>| -> String {
            ts.and_then(|t| Utc.timestamp_opt(t, 0).single())
                .map(|d| d.format("%Y-%m-%d %H:%M:%S UTC").to_string())
                .unwrap_or_else(|| "N/A".to_string())
        };
        println!(
            "  {:<26} {} {}",
            "Oldest tweet:",
            format_ts(info.min_ts).green(),
            info.min_ts
                .map(|t| format!("(ts={})", t))
                .unwrap_or_default()
                .dimmed()
        );
        if let Some(ref tweet_id) = info.oldest_tweet_id {
            println!(
                "  {:<26} {}",
                "",
                format!("https://x.com/i/status/{}", tweet_id).cyan()
            );
        }
        println!(
            "  {:<26} {} {}",
            "Newest tweet:",
            format_ts(info.max_ts).green(),
            info.max_ts
                .map(|t| format!("(ts={})", t))
                .unwrap_or_default()
                .dimmed()
        );
        if let Some(ref tweet_id) = info.newest_tweet_id {
            println!(
                "  {:<26} {}",
                "",
                format!("https://x.com/i/status/{}", tweet_id).cyan()
            );
        }
        println!();

        if info.distribution.is_empty() {
            println!(
                "  {:<26} {}",
                "No valid timestamps:",
                "run `bird db backfill-created-at`".yellow()
            );
        } else {
            println!("  {}", "Top timestamps by frequency:".dimmed());
            for (ts, count) in info.distribution {
                let dt = Utc.timestamp_opt(ts, 0).single();
                let formatted = dt
                    .map(|d| d.format("%Y-%m-%d %H:%M:%S UTC").to_string())
                    .unwrap_or_else(|| "invalid".to_string());
                println!(
                    "  {:<26} {:>6} tweets  {}",
                    formatted,
                    count.to_string().cyan(),
                    format!("(ts={})", ts).dimmed()
                );
            }
        }
    }

    Ok(())
}

/// Ensure database schema and indexes exist.
pub async fn run_optimize(cli: &Cli, show_emoji: bool) -> anyhow::Result<()> {
    let StorageConfig::SurrealDb(db_config) = cli.storage_config()? else {
        anyhow::bail!("Optimize requires SurrealDB storage");
    };

    let storage = SurrealDbStorage::new_with_config(&db_config).await?;
    storage.ensure_schema().await?;

    if cli.json() {
        #[derive(Serialize)]
        struct OptimizeOutput<'a> {
            endpoint: &'a str,
            namespace: &'a str,
            database: &'a str,
            status: &'a str,
        }

        let output = OptimizeOutput {
            endpoint: &db_config.endpoint,
            namespace: &db_config.namespace,
            database: &db_config.database,
            status: "ok",
        };
        println!("{}", format_json(&output));
        return Ok(());
    }

    let icon = if show_emoji { "🧰 " } else { "" };
    println!(
        "{}Schema ensured for {} / {}",
        icon,
        db_config.namespace.cyan(),
        db_config.database.cyan()
    );

    Ok(())
}

fn rocksdb_path_from_endpoint(endpoint: &str) -> Option<PathBuf> {
    const PREFIX: &str = "rocksdb://";
    let path = endpoint.strip_prefix(PREFIX)?;
    if path.is_empty() {
        None
    } else {
        Some(PathBuf::from(path))
    }
}

fn directory_size_bytes(path: &Path) -> anyhow::Result<u64> {
    let metadata = std::fs::symlink_metadata(path).map_err(|err| {
        anyhow::anyhow!("Failed to read storage path {}: {}", path.display(), err)
    })?;

    if metadata.is_file() {
        return Ok(metadata.len());
    }

    if !metadata.is_dir() {
        return Ok(0);
    }

    let mut total = 0u64;
    let mut stack = vec![path.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let entries = std::fs::read_dir(&dir).map_err(|err| {
            anyhow::anyhow!("Failed to read directory {}: {}", dir.display(), err)
        })?;

        for entry in entries {
            let entry = entry.map_err(|err| anyhow::anyhow!("Failed to read entry: {}", err))?;
            let entry_path = entry.path();
            let entry_meta = std::fs::symlink_metadata(&entry_path).map_err(|err| {
                anyhow::anyhow!(
                    "Failed to read metadata for {}: {}",
                    entry_path.display(),
                    err
                )
            })?;

            if entry_meta.file_type().is_symlink() {
                continue;
            }

            if entry_meta.is_dir() {
                stack.push(entry_path);
            } else {
                total = total.saturating_add(entry_meta.len());
            }
        }
    }

    Ok(total)
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut size = bytes as f64;
    let mut unit = 0usize;

    while size >= 1024.0 && unit + 1 < UNITS.len() {
        size /= 1024.0;
        unit += 1;
    }

    if unit == 0 {
        format!("{} B", bytes)
    } else {
        format!("{:.1} {}", size, UNITS[unit])
    }
}

fn format_timestamp(timestamp: Option<i64>) -> Option<String> {
    let timestamp = timestamp?;
    let dt = Utc.timestamp_opt(timestamp, 0).single()?;
    Some(dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
}

/// Backfill headlines for long tweets using LLM.
pub async fn run_backfill_headlines(
    cli: &Cli,
    min_length: usize,
    batch_size: u32,
    max_tweets: Option<u32>,
    provider: String,
    model: Option<String>,
    show_emoji: bool,
) -> anyhow::Result<()> {
    let StorageConfig::SurrealDb(db_config) = cli.storage_config()? else {
        anyhow::bail!("Backfill headlines requires SurrealDB storage");
    };

    let storage = SurrealDbStorage::new_with_config(&db_config).await?;

    // Create LLM provider
    let llm: Box<dyn LlmProvider> = match provider.as_str() {
        "claude-code" => Box::new(ClaudeCodeProvider::new(model)),
        _ => anyhow::bail!("Unsupported provider: {}. Use 'claude-code'.", provider),
    };

    let icon = if show_emoji { "📝 " } else { "" };
    println!(
        "{}Backfilling headlines for tweets with text > {} chars",
        icon, min_length
    );
    println!("  Using {} ({})", llm.name(), llm.model());
    println!();

    let mut total_processed = 0u32;
    let mut total_generated = 0usize;

    loop {
        // Check if we've hit the max
        if let Some(max) = max_tweets {
            if total_processed >= max {
                break;
            }
        }

        // Calculate how many to fetch this batch
        let remaining = max_tweets.map(|m| m - total_processed);
        let fetch_limit = match remaining {
            Some(r) => std::cmp::min(batch_size, r),
            None => batch_size,
        };

        // Get tweets missing headlines
        let tweets = storage
            .get_tweets_missing_headlines(min_length, Some(fetch_limit))
            .await?;

        if tweets.is_empty() {
            break;
        }

        println!(
            "  Processing batch of {} tweets (total: {})...",
            tweets.len(),
            total_processed
        );

        // Generate headlines
        let tweet_refs: Vec<_> = tweets.iter().collect();
        match generate_headlines(&tweet_refs, llm.as_ref()).await {
            Ok(headlines) => {
                if !headlines.is_empty() {
                    // Store headlines
                    let headline_pairs: Vec<(String, String)> = headlines.into_iter().collect();
                    let updated = storage.update_tweet_headlines(&headline_pairs).await?;
                    total_generated += updated;
                    println!(
                        "    {} Generated {} headlines",
                        "✓".green(),
                        updated.to_string().bold()
                    );
                }
            }
            Err(e) => {
                eprintln!("    {} Failed to generate headlines: {}", "✗".red(), e);
            }
        }

        total_processed += tweets.len() as u32;
    }

    println!();
    println!(
        "{}Backfill complete: {} headlines generated for {} tweets",
        icon,
        total_generated.to_string().green().bold(),
        total_processed.to_string().cyan()
    );

    Ok(())
}

/// Repair missing data: backfill headlines and recalculate resonance scores.
pub async fn run_repair(
    cli: &Cli,
    min_length: usize,
    batch_size: u32,
    provider: String,
    model: Option<String>,
    show_emoji: bool,
) -> anyhow::Result<()> {
    let icon = if show_emoji { "🔧 " } else { "" };

    println!("{}Repairing database...", icon);
    println!();

    // Step 1: Backfill headlines
    println!("{}Step 1: Backfilling missing headlines...", icon);
    run_backfill_headlines(
        cli,
        min_length,
        batch_size,
        None,
        provider,
        model,
        show_emoji,
    )
    .await?;

    println!();

    // Step 2: Recalculate resonance scores
    println!("{}Step 2: Recalculating resonance scores...", icon);
    let storage = cli.create_storage().await?;

    // Get user ID from local sync state
    let user_id = storage
        .get_any_synced_user_id()
        .await?
        .ok_or_else(|| anyhow::anyhow!(
            "No synced data found. Run 'bird sync likes' or 'bird sync bookmarks' first."
        ))?;

    // Get stats and refresh scores
    let stats = get_interaction_stats(&storage, &user_id).await?;
    let result = refresh_resonance_scores(&storage, &user_id, Some(5000)).await?;

    let check_icon = if show_emoji { "✅ " } else { "" };
    println!("{}Resonance scores refreshed!", check_icon);
    println!();
    println!("  Interactions analyzed:");
    let like_icon = if show_emoji { "❤️  " } else { "  " };
    let bookmark_icon = if show_emoji { "🔖 " } else { "  " };
    let reply_icon = if show_emoji { "💬 " } else { "  " };
    let quote_icon = if show_emoji { "💬 " } else { "  " };
    let retweet_icon = if show_emoji { "🔁 " } else { "  " };
    println!("    {}Likes: {}", like_icon, stats.likes);
    println!("    {}Bookmarks: {}", bookmark_icon, stats.bookmarks);
    println!("    {}Replies: {}", reply_icon, stats.replies);
    println!("    {}Quotes: {}", quote_icon, stats.quotes);
    println!("    {}Retweets: {}", retweet_icon, stats.retweets);
    println!();
    println!(
        "  Unique tweets with interactions: {}",
        result.unique_tweets.to_string().green()
    );
    println!("  Scores computed: {}", result.computed.to_string().green());

    println!();
    println!("{}Repair complete! ✨", icon.bold().green());
    println!("Use {} to see results.", "bird list --columns id,headline,collections,score".cyan());

    Ok(())
}
