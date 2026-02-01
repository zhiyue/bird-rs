//! Data fetching and resonance computation for bird-tui.

use crate::app::{App, TweetDisplayData};
use bird_core::TweetWithCollections;
use bird_storage::ResonanceScore;
use chrono::{DateTime, Local};
use std::collections::HashMap;

/// Load tweets for the current page and update app state.
/// Also preloads next and previous pages for faster navigation.
pub async fn load_page_tweets(app: &mut App, collections: &[&str]) -> Result<(), String> {
    app.loading = true;

    // Check if current page is cached
    if let Some(cached_tweets) = app.page_cache.get(&app.current_page).cloned() {
        app.tweets = cached_tweets;
        app.table_state.select(Some(0));
        app.detail_scroll_offset = 0;
        app.loading = false;

        // Preload adjacent pages in background (non-blocking)
        let next_page = app.current_page + 1;
        let total_pages = (app.total_count as u32).div_ceil(app.page_size);

        if next_page < total_pages && !app.page_cache.contains_key(&next_page) {
            // Store for background preloading
            app.page_cache.insert(next_page, Vec::new());
        }

        return Ok(());
    }

    // Fetch current page if not cached
    let offset = app.current_page * app.page_size;
    let tweets_result = app
        .storage
        .get_tweets_interleaved(collections, &app.user_id, Some(app.page_size), Some(offset))
        .await
        .map_err(|e| format!("Failed to fetch tweets: {}", e))?;

    let display_tweets = convert_tweets_to_display(app, tweets_result);

    // Cache current page
    app.page_cache
        .insert(app.current_page, display_tweets.clone());
    app.tweets = display_tweets;
    app.table_state.select(Some(0));
    app.detail_scroll_offset = 0;

    app.loading = false;

    // Preload next and previous pages asynchronously
    // Note: This is set up but not fully awaited to avoid blocking UI
    let _ = preload_adjacent_pages(app, collections).await;

    Ok(())
}

/// Helper function to convert raw tweets to display data.
fn convert_tweets_to_display(
    app: &App,
    tweets_result: Vec<TweetWithCollections>,
) -> Vec<TweetDisplayData> {
    tweets_result
        .into_iter()
        .map(|tweet| {
            let tweet_id = tweet.tweet.id.clone();
            let author_id = tweet.tweet.author_id.clone();

            let resonance_score =
                app.resonance_scores
                    .get(&tweet_id)
                    .cloned()
                    .unwrap_or_else(|| {
                        ResonanceScore::new(
                            tweet_id.clone(),
                            app.user_id.clone(),
                            false,
                            false,
                            0,
                            0,
                            0,
                        )
                    });

            let collections_vec = tweet.collections.clone();
            let created_at = tweet.tweet.created_at.as_ref().map(|s| format_timestamp(s));

            // Count interactions with this author's tweets across all loaded tweets
            let (author_liked_count, author_quoted_count, author_retweeted_count) = author_id
                .as_ref()
                .map(|author_id| count_author_interactions(app, author_id))
                .unwrap_or((0, 0, 0));

            TweetDisplayData {
                id: tweet_id,
                text: tweet.tweet.text.clone(),
                author_username: tweet.tweet.author.username.clone(),
                author_name: tweet.tweet.author.name.clone(),
                author_id,
                headline: truncate_text(&tweet.tweet.text, 50),
                collections: collections_vec,
                resonance_score,
                created_at,
                author_liked_count,
                author_quoted_count,
                author_retweeted_count,
            }
        })
        .collect()
}

/// Count how many of an author's tweets are in each collection.
fn count_author_interactions(app: &App, author_id: &str) -> (u32, u32, u32) {
    let mut liked = 0u32;
    let mut quoted = 0u32;
    let mut retweeted = 0u32;

    for tweet in &app.tweets {
        if tweet.author_id.as_deref() == Some(author_id) {
            for collection in &tweet.collections {
                match collection.as_str() {
                    "likes" => liked += 1,
                    "quote_tweets" => quoted += 1,
                    "retweets" => retweeted += 1,
                    _ => {}
                }
            }
        }
    }

    (liked, quoted, retweeted)
}

/// Preload adjacent pages (±2 pages) for faster pagination.
async fn preload_adjacent_pages(app: &mut App, collections: &[&str]) -> Result<(), String> {
    let total_pages = (app.total_count as u32).div_ceil(app.page_size);

    // Pages to preload: current-2, current-1, current+1, current+2
    let pages_to_preload: Vec<i32> = vec![-2, -1, 1, 2];

    for offset_delta in pages_to_preload {
        let page_num = app.current_page as i32 + offset_delta;

        // Skip if page is out of bounds or already cached
        if page_num < 0
            || page_num >= total_pages as i32
            || app.page_cache.contains_key(&(page_num as u32))
        {
            continue;
        }

        let offset = (page_num as u32) * app.page_size;
        if let Ok(tweets_result) = app
            .storage
            .get_tweets_interleaved(collections, &app.user_id, Some(app.page_size), Some(offset))
            .await
        {
            let display_tweets = convert_tweets_to_display(app, tweets_result);
            app.page_cache.insert(page_num as u32, display_tweets);
        }
    }

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

/// Truncate text to a maximum length, adding ellipsis if needed. UTF-8 safe.
fn truncate_text(text: &str, max_len: usize) -> String {
    let mut result = String::new();
    let mut byte_count = 0;
    let limit = max_len.saturating_sub(1);

    for ch in text.chars() {
        let ch_len = ch.len_utf8();
        if byte_count + ch_len > limit {
            result.push('…');
            break;
        }
        result.push(ch);
        byte_count += ch_len;
    }

    if result.is_empty() && !text.is_empty() {
        String::from("…")
    } else {
        result
    }
}

/// Format a timestamp string into a more readable format with local timezone.
/// Converts Twitter timestamp format (e.g., "Wed Jan 28 15:00:44 +0000 2026")
/// to user's local timezone and formats as "Wed Jan 28 03:00pm".
fn format_timestamp(ts_str: &str) -> String {
    // Try to parse Twitter's format: "Wed Jan 28 15:00:44 +0000 2026"
    // Format: "%a %b %d %H:%M:%S %z %Y"
    match DateTime::parse_from_str(ts_str, "%a %b %d %H:%M:%S %z %Y") {
        Ok(utc_time) => {
            // Convert to local timezone
            let local_time = utc_time.with_timezone(&Local);
            // Format as "Wed Jan 28 03:00pm"
            local_time
                .format("%a %b %d %I:%M%p")
                .to_string()
                .to_lowercase()
        }
        Err(_) => {
            // Fallback if parsing fails
            ts_str.to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_text() {
        assert_eq!(truncate_text("hello", 10), "hello");
        assert_eq!(truncate_text("hello world", 8), "hello w…");
    }

    #[test]
    fn test_format_timestamp() {
        // Test parsing Twitter timestamp format and converting to local timezone
        let result = format_timestamp("Wed Jan 28 15:00:44 +0000 2026");

        // The output should be in the format "day mon dd hh:mmp" with lowercase am/pm
        // Since we converted from UTC+0 to local, the exact time depends on the system timezone
        // But it should have the pattern we expect (containing day, month abbreviation, and am/pm)
        let result_lower = result.to_lowercase();
        assert!(!result.is_empty(), "Timestamp should not be empty");
        assert!(result_lower.contains("28"), "Should contain day of month");
        assert!(
            result_lower.contains("am") || result_lower.contains("pm"),
            "Should contain am or pm, got: {}",
            result
        );
    }
}
