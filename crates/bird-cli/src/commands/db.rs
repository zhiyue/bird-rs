//! Database maintenance commands.

use crate::cli::Cli;
use crate::output::format_json;
use bird_storage::{StorageConfig, SurrealDbStorage};
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
pub async fn run_status(cli: &Cli, show_emoji: bool) -> anyhow::Result<()> {
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
    println!("{}Database status", icon);
    println!("  Endpoint: {}", db_config.endpoint.cyan());
    println!("  Namespace: {}", db_config.namespace.cyan());
    println!("  Database: {}", db_config.database.cyan());
    println!("  Auth: {}", auth_mode.cyan());
    println!();
    println!("  Tweets: {}", stats.tweets.to_string().green().bold());
    println!(
        "  Collections: {}",
        stats.collections.to_string().green().bold()
    );
    println!("  Likes: {}", stats.likes.to_string().green().bold());
    println!(
        "  Bookmarks: {}",
        stats.bookmarks.to_string().green().bold()
    );
    println!(
        "  Sync states: {}",
        stats.sync_states.to_string().green().bold()
    );
    let missing = stats.missing_created_at_ts;
    if missing > 0 {
        println!(
            "  Missing created_at_ts: {}",
            missing.to_string().yellow().bold()
        );
    } else {
        println!("  Missing created_at_ts: {}", "0".green().bold());
    }
    match &oldest_tweet_at {
        Some(date) => println!("  Oldest tweet: {}", date.cyan()),
        None => println!(
            "  Oldest tweet: {}",
            "unknown (missing created_at_ts)".yellow()
        ),
    }
    match &newest_tweet_at {
        Some(date) => println!("  Newest tweet: {}", date.cyan()),
        None => println!(
            "  Newest tweet: {}",
            "unknown (missing created_at_ts)".yellow()
        ),
    }
    println!();
    match storage_path {
        Some(path) => {
            println!("  Storage path: {}", path.display().to_string().cyan());
            match storage_size_bytes {
                Some(size) => {
                    let human = format_bytes(size);
                    println!(
                        "  Storage size: {} ({})",
                        human.green().bold(),
                        size.to_string().green()
                    );
                }
                None => {
                    let message = storage_error
                        .as_deref()
                        .unwrap_or("unavailable for this endpoint");
                    println!("  Storage size: {}", message.yellow());
                }
            }
        }
        None => {
            println!(
                "  Storage size: {}",
                "unavailable for remote endpoint".yellow()
            );
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
