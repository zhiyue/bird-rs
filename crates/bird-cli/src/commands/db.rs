//! Database maintenance commands.

use crate::cli::Cli;
use crate::output::format_json;
use bird_storage::{StorageConfig, SurrealDbStorage};
use colored::Colorize;
use serde::Serialize;

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
            sync_states: u64,
            missing_created_at_ts: u64,
        }

        let output = DbStatusOutput {
            endpoint: &db_config.endpoint,
            namespace: &db_config.namespace,
            database: &db_config.database,
            auth: auth_mode,
            tweets: stats.tweets,
            collections: stats.collections,
            sync_states: stats.sync_states,
            missing_created_at_ts: stats.missing_created_at_ts,
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
