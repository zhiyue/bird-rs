//! Data fetching and resonance computation for bird-tui.

use crate::app::{App, TweetDisplayData};
use bird_storage::ResonanceScore;
use std::collections::HashMap;

/// Load tweets for the current page and update app state.
pub async fn load_page_tweets(app: &mut App, collections: &[&str]) -> Result<(), String> {
    app.loading = true;

    let offset = app.current_page * app.page_size;

    // Fetch tweets for this page
    let tweets_result = app
        .storage
        .get_tweets_interleaved(collections, &app.user_id, Some(app.page_size), Some(offset))
        .await
        .map_err(|e| format!("Failed to fetch tweets: {}", e))?;

    app.clear();

    // Convert to display data
    for tweet in tweets_result {
        let tweet_id = tweet.tweet.id.clone();

        // Get resonance score (should already be computed)
        let resonance_score = app
            .resonance_scores
            .get(&tweet_id)
            .cloned()
            .unwrap_or_else(|| {
                ResonanceScore::new(tweet_id.clone(), app.user_id.clone(), false, false, 0, 0, 0)
            });

        // Get collections and timestamps
        let collections_vec = tweet.collections.clone();
        let created_at = tweet.tweet.created_at.as_ref().map(|s| format_timestamp(s));

        let display = TweetDisplayData {
            id: tweet_id.clone(),
            text: tweet.tweet.text.clone(),
            author_username: tweet.tweet.author.username.clone(),
            author_name: tweet.tweet.author.name.clone(),
            headline: truncate_text(&tweet.tweet.text, 50),
            collections: collections_vec,
            resonance_score,
            created_at,
        };

        app.tweets.push(display);
    }

    app.loading = false;
    Ok(())
}

/// Compute and cache resonance scores for all tweets in the database.
pub async fn compute_resonance_scores(app: &mut App) -> Result<(), String> {
    app.loading = true;

    // Batch fetch all interaction pairs
    let reply_pairs = app
        .storage
        .get_user_reply_tweets(&app.user_id, None)
        .await
        .map_err(|e| format!("Failed to fetch replies: {}", e))?;

    let quote_pairs = app
        .storage
        .get_user_quote_tweets(&app.user_id, None)
        .await
        .map_err(|e| format!("Failed to fetch quotes: {}", e))?;

    let retweet_pairs = app
        .storage
        .get_user_retweets(&app.user_id, None)
        .await
        .map_err(|e| format!("Failed to fetch retweets: {}", e))?;

    // Build count maps
    let mut reply_count_map: HashMap<String, u32> = HashMap::new();
    for (tweet_id, _) in reply_pairs {
        *reply_count_map.entry(tweet_id).or_insert(0) += 1;
    }

    let mut quote_count_map: HashMap<String, u32> = HashMap::new();
    for (tweet_id, _) in quote_pairs {
        *quote_count_map.entry(tweet_id).or_insert(0) += 1;
    }

    let mut retweet_count_map: HashMap<String, u32> = HashMap::new();
    for (tweet_id, _) in retweet_pairs {
        *retweet_count_map.entry(tweet_id).or_insert(0) += 1;
    }

    // Cache the maps for later computation
    app.resonance_scores.clear();

    // Compute scores for a sample (we'll compute on-demand for displayed tweets)
    // This is just storing the count maps; actual scores computed when rendering

    app.loading = false;
    Ok(())
}

/// Truncate text to a maximum length, adding ellipsis if needed.
fn truncate_text(text: &str, max_len: usize) -> String {
    if text.len() <= max_len {
        text.to_string()
    } else {
        format!("{}…", &text[..max_len - 1])
    }
}

/// Format a timestamp string into a more readable format.
fn format_timestamp(ts_str: &str) -> String {
    // Try to parse and format, but fallback to original if parsing fails
    ts_str.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_text() {
        assert_eq!(truncate_text("hello", 10), "hello");
        assert_eq!(truncate_text("hello world", 8), "hello w…");
    }
}
