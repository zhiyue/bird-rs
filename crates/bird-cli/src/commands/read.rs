//! Read command implementation with cache-first behavior.

use crate::cli::{extract_tweet_id, Cli};
use crate::output::{format_json, format_tweet};
use bird_storage::TweetStore;

/// Run the read command.
pub async fn run(
    cli: &Cli,
    tweet_id: &str,
    _json_full: bool,
    show_emoji: bool,
) -> anyhow::Result<()> {
    let id = extract_tweet_id(tweet_id)?;

    // Try cache first (unless --no-cache is specified)
    if !cli.no_cache() {
        if let Ok(storage) = cli.create_storage().await {
            if let Ok(Some(tweet)) = storage.get_tweet(&id).await {
                if cli.json() {
                    println!("{}", format_json(&tweet));
                } else {
                    print!("{}", format_tweet(&tweet, show_emoji));
                }
                return Ok(());
            }
        }
    }

    // Fetch from API
    let client = cli.create_client()?;
    let tweet = client.get_tweet(&id).await?;

    // Cache the result
    if let Ok(storage) = cli.create_storage().await {
        let _ = storage.upsert_tweet(&tweet).await;
    }

    if cli.json() {
        println!("{}", format_json(&tweet));
    } else {
        print!("{}", format_tweet(&tweet, show_emoji));
    }

    Ok(())
}
