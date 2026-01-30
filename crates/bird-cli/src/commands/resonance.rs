//! Resonance command implementation.
//!
//! All resonance commands run fully offline using only local database data.

use crate::cli::Cli;
use crate::output::format_json;
use crate::resonance::{get_interaction_stats, refresh_resonance_scores};
use colored::Colorize;
use serde::Serialize;
use std::sync::Arc;

const DEFAULT_MAX_PER_COLLECTION: u32 = 5000;

/// Get the user_id from existing sync states in the database.
/// This allows resonance commands to run fully offline.
async fn get_user_id_offline(storage: &Arc<dyn bird_storage::Storage>) -> anyhow::Result<String> {
    storage.get_any_synced_user_id().await?.ok_or_else(|| {
        anyhow::anyhow!(
            "No synced data found. Run 'bird sync likes' or 'bird sync bookmarks' first."
        )
    })
}

/// Run the resonance refresh command (fully offline).
pub async fn run_refresh(
    cli: &Cli,
    max_per_collection: Option<u32>,
    show_emoji: bool,
) -> anyhow::Result<()> {
    let storage = cli.create_storage().await?;

    // Get user ID from local sync state (no API call needed)
    let user_id = get_user_id_offline(&storage).await?;

    let max_per_collection = max_per_collection.unwrap_or(DEFAULT_MAX_PER_COLLECTION);

    if !cli.json() {
        let icon = if show_emoji { "🔄 " } else { "" };
        println!("{}Computing resonance scores...", icon);
    }

    // Get stats first for reporting
    let stats = get_interaction_stats(&storage, &user_id).await?;

    // Refresh scores
    let result = refresh_resonance_scores(&storage, &user_id, Some(max_per_collection)).await?;

    if cli.json() {
        #[derive(Serialize)]
        struct RefreshResultJson {
            computed: usize,
            unique_tweets: usize,
            previous_count: u64,
            interactions: InteractionsJson,
        }

        #[derive(Serialize)]
        struct InteractionsJson {
            likes: usize,
            bookmarks: usize,
            replies: usize,
            quotes: usize,
            retweets: usize,
        }

        let json = RefreshResultJson {
            computed: result.computed,
            unique_tweets: result.unique_tweets,
            previous_count: result.previous_count,
            interactions: InteractionsJson {
                likes: stats.likes,
                bookmarks: stats.bookmarks,
                replies: stats.replies,
                quotes: stats.quotes,
                retweets: stats.retweets,
            },
        };
        println!("{}", format_json(&json));
        return Ok(());
    }

    let icon = if show_emoji { "✅ " } else { "" };
    println!("{}Resonance scores refreshed!", icon);
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

    if result.previous_count > 0 {
        println!(
            "  Previous cache cleared: {} scores",
            result.previous_count.to_string().dimmed()
        );
    }

    println!();
    println!("Use {} to see scores.", "bird list --columns score".cyan());

    Ok(())
}
