//! Sync command implementations.

use crate::cli::Cli;
use crate::output::format_json;
use crate::storage_monitor::{format_bytes, parse_size, StorageMonitor};
use crate::sync_engine::{SyncEngine, SyncOptions, SyncProgress};
use bird_client::{Collection, CurrentUserResult, RateLimitConfig};
use bird_storage::StorageConfig;
use chrono::Utc;
use colored::Colorize;

/// Get endpoint string from StorageConfig for monitoring.
fn get_storage_endpoint(config: &StorageConfig) -> Option<String> {
    match config {
        StorageConfig::SurrealDb(cfg) => Some(cfg.endpoint.clone()),
        StorageConfig::Memory => None,
    }
}

/// Run the sync likes command.
pub async fn run_sync_likes(
    cli: &Cli,
    full: bool,
    max_pages: Option<u32>,
    delay_ms: Option<u64>,
    no_backfill: bool,
    max_storage: Option<String>,
    show_emoji: bool,
) -> anyhow::Result<()> {
    run_sync(
        cli,
        Collection::Likes,
        full,
        max_pages,
        delay_ms,
        no_backfill,
        max_storage,
        show_emoji,
    )
    .await
}

/// Run the sync bookmarks command.
pub async fn run_sync_bookmarks(
    cli: &Cli,
    full: bool,
    max_pages: Option<u32>,
    delay_ms: Option<u64>,
    no_backfill: bool,
    max_storage: Option<String>,
    show_emoji: bool,
) -> anyhow::Result<()> {
    run_sync(
        cli,
        Collection::Bookmarks,
        full,
        max_pages,
        delay_ms,
        no_backfill,
        max_storage,
        show_emoji,
    )
    .await
}

/// Run the sync posts command.
pub async fn run_sync_posts(
    cli: &Cli,
    full: bool,
    max_pages: Option<u32>,
    delay_ms: Option<u64>,
    no_backfill: bool,
    max_storage: Option<String>,
    show_emoji: bool,
) -> anyhow::Result<()> {
    run_sync(
        cli,
        Collection::UserTweets,
        full,
        max_pages,
        delay_ms,
        no_backfill,
        max_storage,
        show_emoji,
    )
    .await
}

/// Run the sync backfill command.
pub async fn run_backfill(
    cli: &Cli,
    collection: Collection,
    max_pages: Option<u32>,
    delay_ms: Option<u64>,
    max_storage: Option<String>,
    show_emoji: bool,
) -> anyhow::Result<()> {
    let mut client = cli.create_client()?;
    let storage = cli.create_storage().await?;
    let db_config = cli.storage_config()?;

    // Get current user ID
    let user_id = match client.get_current_user().await {
        CurrentUserResult::Success(user) => user.id,
        CurrentUserResult::Error(e) => {
            anyhow::bail!("Failed to get current user: {}", e);
        }
    };

    // Parse max storage limit
    let max_storage_bytes = parse_max_storage(&max_storage)?;

    // Create storage monitor
    let storage_endpoint = get_storage_endpoint(&db_config);
    let storage_monitor = storage_endpoint
        .as_ref()
        .map(|e| StorageMonitor::from_endpoint(e, max_storage_bytes))
        .unwrap_or_else(|| StorageMonitor::new(None, max_storage_bytes));

    // Show initial storage info
    if storage_monitor.is_available() {
        let progress = storage_monitor.progress_info();
        let icon = if show_emoji { "💾 " } else { "" };
        eprintln!("{}Storage: {}", icon, progress.format().cyan());
    }

    let icon = if show_emoji { "⏪ " } else { "" };
    println!(
        "{}Backfilling {} for user {}...",
        icon,
        collection.as_str().bold(),
        user_id.dimmed()
    );

    // Build rate limit config (use default 45s human-like delay unless overridden)
    let rate_limit = match delay_ms {
        Some(ms) => RateLimitConfig::with_delay(ms),
        None => RateLimitConfig::default(),
    };

    // Build sync options with storage monitor and progress callback
    let options = SyncOptions {
        full: false,
        max_pages: max_pages.or(Some(10)), // Conservative default
        no_backfill: false,
        rate_limit,
        storage_monitor: Some(storage_monitor),
        on_progress: Some(create_progress_callback(show_emoji)),
    };

    // Create sync engine
    let engine = SyncEngine::new(client, storage);

    // Run backfill
    let result = engine
        .backfill_collection(collection, &user_id, &options)
        .await?;

    // Output results
    output_sync_result(cli, &collection, &result, show_emoji);

    Ok(())
}

/// Run sync for a specific collection.
#[allow(clippy::too_many_arguments)]
async fn run_sync(
    cli: &Cli,
    collection: Collection,
    full: bool,
    max_pages: Option<u32>,
    delay_ms: Option<u64>,
    no_backfill: bool,
    max_storage: Option<String>,
    show_emoji: bool,
) -> anyhow::Result<()> {
    let mut client = cli.create_client()?;
    let storage = cli.create_storage().await?;
    let db_config = cli.storage_config()?;

    // Get current user ID
    let user_id = match client.get_current_user().await {
        CurrentUserResult::Success(user) => user.id,
        CurrentUserResult::Error(e) => {
            anyhow::bail!("Failed to get current user: {}", e);
        }
    };

    // Parse max storage limit
    let max_storage_bytes = parse_max_storage(&max_storage)?;

    // Create storage monitor
    let storage_endpoint = get_storage_endpoint(&db_config);
    let storage_monitor = storage_endpoint
        .as_ref()
        .map(|e| StorageMonitor::from_endpoint(e, max_storage_bytes))
        .unwrap_or_else(|| StorageMonitor::new(None, max_storage_bytes));

    // Show initial storage info
    if storage_monitor.is_available() {
        let progress = storage_monitor.progress_info();
        let icon = if show_emoji { "💾 " } else { "" };
        eprintln!("{}Storage: {}", icon, progress.format().cyan());
    }

    let icon = if show_emoji { "🔄 " } else { "" };
    println!(
        "{}Syncing {} for user {}...",
        icon,
        collection.as_str().bold(),
        user_id.dimmed()
    );

    // Build rate limit config (use default 45s human-like delay unless overridden)
    let rate_limit = match delay_ms {
        Some(ms) => RateLimitConfig::with_delay(ms),
        None => RateLimitConfig::default(),
    };

    // Build sync options with storage monitor and progress callback
    let options = SyncOptions {
        full,
        max_pages: max_pages.or(Some(10)), // Conservative default
        no_backfill,
        rate_limit,
        storage_monitor: Some(storage_monitor),
        on_progress: Some(create_progress_callback(show_emoji)),
    };

    // Create sync engine
    let engine = SyncEngine::new(client, storage);

    // Run sync
    let result = engine
        .sync_collection(collection, &user_id, &options)
        .await?;

    // Output results
    output_sync_result(cli, &collection, &result, show_emoji);

    Ok(())
}

/// Parse max storage string to bytes.
fn parse_max_storage(max_storage: &Option<String>) -> anyhow::Result<Option<u64>> {
    match max_storage {
        Some(s) => {
            let bytes =
                parse_size(s).map_err(|e| anyhow::anyhow!("Invalid --max-storage: {}", e))?;
            Ok(Some(bytes))
        }
        None => Ok(None),
    }
}

/// Create a progress callback for sync operations.
fn create_progress_callback(show_emoji: bool) -> Box<dyn Fn(&SyncProgress) + Send + Sync> {
    Box::new(move |progress: &SyncProgress| {
        let icon = if show_emoji { "📊 " } else { "" };
        let storage_info = match (&progress.storage_formatted, &progress.max_storage_bytes) {
            (Some(current), Some(max)) => {
                format!(" | Storage: {} / {}", current.cyan(), format_bytes(*max))
            }
            (Some(current), None) => format!(" | Storage: {}", current.cyan()),
            _ => String::new(),
        };
        eprintln!(
            "{}Progress: {} fetched, {} new{}",
            icon,
            progress.tweets_fetched.to_string().green(),
            progress.new_tweets.to_string().green().bold(),
            storage_info
        );
    })
}

/// Output sync result.
fn output_sync_result(
    cli: &Cli,
    collection: &Collection,
    result: &crate::sync_engine::SyncResult,
    show_emoji: bool,
) {
    if cli.json() {
        #[derive(serde::Serialize)]
        struct SyncResultJson {
            collection: String,
            direction: String,
            new_tweets: usize,
            total_fetched: usize,
            stopped_at_known: bool,
            has_more_history: bool,
            stopped_at_storage_limit: bool,
            #[serde(skip_serializing_if = "Option::is_none")]
            final_storage_bytes: Option<u64>,
        }

        println!(
            "{}",
            format_json(&SyncResultJson {
                collection: collection.as_str().to_string(),
                direction: result.direction.to_string(),
                new_tweets: result.new_tweets,
                total_fetched: result.total_fetched,
                stopped_at_known: result.stopped_at_known,
                has_more_history: result.has_more_history,
                stopped_at_storage_limit: result.stopped_at_storage_limit,
                final_storage_bytes: result.final_storage_bytes,
            })
        );
    } else {
        let check = if show_emoji { "✓ " } else { "" };
        println!(
            "{}Sync complete ({} sync): {} new tweets stored ({} total fetched)",
            check.green(),
            result.direction.to_string().cyan(),
            result.new_tweets.to_string().green().bold(),
            result.total_fetched
        );

        if result.stopped_at_known {
            let info = if show_emoji { "ℹ️  " } else { "" };
            println!(
                "{}Stopped at previously synced tweet (incremental sync)",
                info.dimmed()
            );
        }

        if result.stopped_at_storage_limit {
            let warn = if show_emoji { "⚠️  " } else { "" };
            let storage_info = result
                .final_storage_bytes
                .map(|b| format!(" (current: {})", format_bytes(b)))
                .unwrap_or_default();
            println!(
                "{}Storage limit reached - sync halted{}",
                warn.yellow(),
                storage_info
            );
        }

        if result.has_more_history && !result.stopped_at_storage_limit {
            let info = if show_emoji { "📚 " } else { "" };
            println!(
                "{}More history available. Run `bird sync backfill {}` to continue.",
                info.yellow(),
                collection.as_str()
            );
        }

        // Show final storage size
        if let Some(bytes) = result.final_storage_bytes {
            let icon = if show_emoji { "💾 " } else { "" };
            println!("{}Final storage size: {}", icon, format_bytes(bytes).cyan());
        }
    }
}

/// Run the sync status command.
pub async fn run_status(cli: &Cli, show_emoji: bool) -> anyhow::Result<()> {
    let mut client = cli.create_client()?;
    let storage = cli.create_storage().await?;

    // Get current user ID
    let user_id = match client.get_current_user().await {
        CurrentUserResult::Success(user) => user.id,
        CurrentUserResult::Error(e) => {
            anyhow::bail!("Failed to get current user: {}", e);
        }
    };

    let states = storage.get_all_sync_states(&user_id).await?;

    if cli.json() {
        println!("{}", format_json(&states));
        return Ok(());
    }

    if states.is_empty() {
        let icon = if show_emoji { "📭 " } else { "" };
        println!("{}No sync history found for user {}", icon, user_id);
        return Ok(());
    }

    let icon = if show_emoji { "📊 " } else { "" };
    println!("{}Sync Status for user {}", icon, user_id.bold());
    println!("{}", "─".repeat(70));

    // Summary table header
    println!();
    println!(
        "  {:<14} {:>10} {:>12} {:>10} {:>12}",
        "Collection".bold(),
        "Synced".bold(),
        "Backfill".bold(),
        "Rate Lim".bold(),
        "Last Sync".bold()
    );
    println!(
        "  {:<14} {:>10} {:>12} {:>10} {:>12}",
        "──────────────", "──────────", "────────────", "──────────", "────────────"
    );

    // Summary rows
    for state in &states {
        let collection_icon = if show_emoji {
            match state.collection.as_str() {
                "likes" => "❤️  ",
                "bookmarks" => "🔖 ",
                "timeline" => "🏠 ",
                "user_tweets" => "📝 ",
                _ => "📁 ",
            }
        } else {
            ""
        };

        let backfill_status = if state.has_more_history {
            "pending".yellow().to_string()
        } else {
            "complete".green().to_string()
        };

        let rate_limit_status = if state.last_rate_limited_at.is_some() {
            "yes".yellow().to_string()
        } else {
            "no".dimmed().to_string()
        };

        let last_sync = format_relative_time(state.last_sync_at);

        println!(
            "  {}{:<12} {:>10} {:>12} {:>10} {:>12}",
            collection_icon,
            state.collection,
            state.total_synced.to_string().green(),
            backfill_status,
            rate_limit_status,
            last_sync.cyan()
        );
    }

    // Detailed info per collection
    println!();
    println!("{}", "─".repeat(70));
    println!();
    println!("  {}", "Details".bold());

    for state in &states {
        let collection_icon = if show_emoji {
            match state.collection.as_str() {
                "likes" => "❤️  ",
                "bookmarks" => "🔖 ",
                "timeline" => "🏠 ",
                "user_tweets" => "📝 ",
                _ => "📁 ",
            }
        } else {
            ""
        };

        println!();
        println!("  {}{}", collection_icon, state.collection.bold());

        // IDs
        if let Some(ref newest_id) = state.newest_item_id {
            println!("    {:<18} {}", "Newest ID:", newest_id.dimmed());
        }
        if let Some(ref oldest_id) = state.oldest_item_id {
            println!("    {:<18} {}", "Oldest ID:", oldest_id.dimmed());
        }

        // Last sync with full timestamp
        println!(
            "    {:<18} {}",
            "Last sync:",
            format_sync_time(state.last_sync_at)
        );

        // Rate limit details
        if let Some(last_rate_limited_at) = state.last_rate_limited_at {
            let mut info = format_sync_time(last_rate_limited_at);
            if let Some(backoff_ms) = state.last_rate_limit_backoff_ms {
                info.push_str(&format!(" · backoff {}ms", backoff_ms));
            }
            if let Some(retries) = state.last_rate_limit_retries {
                info.push_str(&format!(" · {} retries", retries));
            }
            println!("    {:<18} {}", "Rate limited:", info.yellow());
        }

        // Backfill details
        if state.has_more_history {
            if let Some(ref cursor) = state.backfill_cursor {
                let truncated = if cursor.len() > 30 {
                    format!("{}...", &cursor[..30])
                } else {
                    cursor.clone()
                };
                println!("    {:<18} {}", "Backfill cursor:", truncated.dimmed());
            }
        }
    }

    println!();

    Ok(())
}

/// Run the sync reset command.
pub async fn run_reset(cli: &Cli, collection: &str) -> anyhow::Result<()> {
    let mut client = cli.create_client()?;
    let storage = cli.create_storage().await?;

    // Get current user ID
    let user_id = match client.get_current_user().await {
        CurrentUserResult::Success(user) => user.id,
        CurrentUserResult::Error(e) => {
            anyhow::bail!("Failed to get current user: {}", e);
        }
    };

    // Validate collection name
    let _: Collection = collection.parse().map_err(|e: String| anyhow::anyhow!(e))?;

    storage.clear_sync_state(collection, &user_id).await?;

    println!("Reset sync state for {} (user {})", collection, user_id);

    Ok(())
}

/// Format a relative time string.
fn format_relative_time(time: chrono::DateTime<Utc>) -> String {
    let now = Utc::now();
    let duration = now.signed_duration_since(time);

    if duration.num_seconds() < 60 {
        "just now".to_string()
    } else if duration.num_minutes() < 60 {
        format!("{} minutes ago", duration.num_minutes())
    } else if duration.num_hours() < 24 {
        format!("{} hours ago", duration.num_hours())
    } else if duration.num_days() < 7 {
        format!("{} days ago", duration.num_days())
    } else {
        time.format("%Y-%m-%d %H:%M").to_string()
    }
}

fn format_sync_time(time: chrono::DateTime<Utc>) -> String {
    let absolute = time.format("%Y-%m-%d %H:%M UTC").to_string();
    let relative = format_relative_time(time);
    format!("{} ({})", absolute, relative)
}
