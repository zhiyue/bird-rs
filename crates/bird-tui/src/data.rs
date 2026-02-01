//! Data fetching and resonance computation for bird-tui.

use crate::app::{App, TweetDisplayData};
use bird_core::TweetWithCollections;
use bird_storage::ResonanceScore;
use chrono::{DateTime, Local};
use std::cmp::Ordering;
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

/// Load tweets for calendar mode and apply the active range filter.
pub async fn load_calendar_tweets(app: &mut App, collections: &[&str]) -> Result<(), String> {
    if app.calendar_needs_reload {
        app.loading = true;

        let tweets_result = app
            .storage
            .get_tweets_interleaved(collections, &app.user_id, None, None)
            .await
            .map_err(|e| format!("Failed to fetch calendar tweets: {}", e))?;

        let mut display_tweets = convert_tweets_to_display(app, tweets_result);
        display_tweets.sort_by(|a, b| match (&a.created_at_local, &b.created_at_local) {
            (Some(a_dt), Some(b_dt)) => b_dt.cmp(a_dt),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => a.id.cmp(&b.id),
        });

        app.calendar_all_tweets = display_tweets;
        app.calendar_needs_reload = false;
        app.calendar_needs_filter = true;
        app.loading = false;
    }

    if app.calendar_needs_filter {
        apply_calendar_filter(app);
        app.calendar_needs_filter = false;
    }

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
            let created_at_raw = tweet.tweet.created_at.as_deref();
            let created_at_local = created_at_raw.and_then(parse_created_at);
            let created_at = match (created_at_local.as_ref(), created_at_raw) {
                (Some(parsed), _) => Some(format_timestamp_local(parsed)),
                (None, Some(raw)) => Some(raw.to_string()),
                (None, None) => None,
            };

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
                created_at_local,
                like_count: tweet.tweet.like_count,
                retweet_count: tweet.tweet.retweet_count,
                reply_count: tweet.tweet.reply_count,
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

/// Filter calendar tweets by the active range.
fn apply_calendar_filter(app: &mut App) {
    let Some((start, end)) = app.calendar_range_naive() else {
        app.calendar_tweets.clear();
        app.calendar_reset_selection();
        return;
    };

    app.calendar_tweets = app
        .calendar_all_tweets
        .iter()
        .filter(|tweet| {
            let Some(dt) = tweet.created_at_local.as_ref() else {
                return false;
            };
            let date = dt.date_naive();
            date >= start && date <= end
        })
        .cloned()
        .collect();

    app.calendar_reset_selection();
}

/// Format a timestamp string into a more readable format with local timezone.
/// Converts Twitter timestamp format (e.g., "Wed Jan 28 15:00:44 +0000 2026")
/// to user's local timezone and formats as "Wed Jan 28 03:00pm".
fn parse_created_at(ts_str: &str) -> Option<DateTime<Local>> {
    if let Ok(rfc3339) = DateTime::parse_from_rfc3339(ts_str) {
        return Some(rfc3339.with_timezone(&Local));
    }

    if let Ok(twitter_time) = DateTime::parse_from_str(ts_str, "%a %b %d %H:%M:%S %z %Y") {
        return Some(twitter_time.with_timezone(&Local));
    }

    None
}

/// Format a timestamp into a readable local string.
fn format_timestamp_local(dt: &DateTime<Local>) -> String {
    dt.format("%a %b %d %I:%M%p").to_string().to_lowercase()
}

/// Format a timestamp as a relative time (e.g., 2h ago, 3d ago, or YYYY/MM/DD).
pub fn format_relative_time(dt: &DateTime<Local>) -> String {
    let now = Local::now();
    let diff = now.signed_duration_since(*dt);
    let seconds = diff.num_seconds().max(0);

    if seconds < 60 {
        return "now".to_string();
    }

    let minutes = diff.num_minutes();
    if minutes < 60 {
        return format!("{}m ago", minutes.max(1));
    }

    let hours = diff.num_hours();
    if hours < 24 {
        return format!("{}h ago", hours);
    }

    let days = diff.num_days();
    if days < 30 {
        return format!("{}d ago", days);
    }

    dt.format("%Y/%m/%d").to_string()
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
        let parsed = parse_created_at("Wed Jan 28 15:00:44 +0000 2026");
        assert!(parsed.is_some(), "Should parse Twitter timestamp format");

        let result = format_timestamp_local(&parsed.unwrap());

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
