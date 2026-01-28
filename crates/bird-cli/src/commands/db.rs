//! Database maintenance commands.

use crate::cli::Cli;
use bird_storage::SurrealDbStorage;
use colored::Colorize;

/// Backfill created_at_ts values for existing tweets.
pub async fn run_backfill_created_at(
    cli: &Cli,
    batch_size: Option<u32>,
    show_emoji: bool,
) -> anyhow::Result<()> {
    if !cli.uses_surrealdb() {
        anyhow::bail!("Backfill requires SurrealDB storage");
    };

    let storage = SurrealDbStorage::new_with_config(&cli.surrealdb_config()?).await?;
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
