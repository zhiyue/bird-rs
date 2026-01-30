//! Resonance score calculation and management.
//!
//! Resonance scores track which tweets resonated most with you based on
//! cumulative interaction weight (ordered by effort/intent):
//! - Bookmark: 1.0 (highest - saved for later)
//! - Retweet: 0.8 (sharing with followers)
//! - Quote: 0.75 (sharing with commentary)
//! - Reply: 0.5 (engagement)
//! - Like: 0.25 (lowest - just a click)

use bird_storage::{ResonanceScore, Storage};
use std::collections::HashMap;
use std::sync::Arc;

/// Result of refreshing resonance scores.
#[derive(Debug)]
pub struct RefreshResult {
    /// Number of scores computed.
    pub computed: usize,
    /// Number of unique tweets with interactions.
    pub unique_tweets: usize,
    /// Previous score count (before refresh).
    pub previous_count: u64,
}

/// Compute and cache all resonance scores for a user.
///
/// This clears existing scores and recomputes from scratch based on:
/// - Likes collection
/// - Bookmarks collection
/// - User's reply tweets (in user_tweets collection)
/// - User's quote tweets (in user_tweets collection)
/// - User's retweets (in user_tweets collection)
pub async fn refresh_resonance_scores(
    storage: &Arc<dyn Storage>,
    user_id: &str,
    max_per_collection: Option<u32>,
) -> anyhow::Result<RefreshResult> {
    // Get previous count for reporting
    let previous_count = storage.resonance_score_count(user_id).await?;

    // Clear existing scores
    storage.clear_resonance_scores(user_id).await?;

    // Collect interaction data (IDs only, lightweight)
    let like_ids = storage
        .get_collection_tweet_ids("likes", user_id, max_per_collection)
        .await?;

    let bookmark_ids = storage
        .get_collection_tweet_ids("bookmarks", user_id, max_per_collection)
        .await?;

    let reply_pairs = storage
        .get_user_reply_tweets(user_id, max_per_collection)
        .await?;

    let quote_pairs = storage
        .get_user_quote_tweets(user_id, max_per_collection)
        .await?;

    let retweet_pairs = storage
        .get_user_retweets(user_id, max_per_collection)
        .await?;

    // Build score map: tweet_id -> (liked, bookmarked, reply_count, quote_count, retweet_count)
    let mut score_map: HashMap<String, (bool, bool, u32, u32, u32)> = HashMap::new();

    // Process likes
    for id in &like_ids {
        let entry = score_map
            .entry(id.clone())
            .or_insert((false, false, 0, 0, 0));
        entry.0 = true; // liked
    }

    // Process bookmarks
    for id in &bookmark_ids {
        let entry = score_map
            .entry(id.clone())
            .or_insert((false, false, 0, 0, 0));
        entry.1 = true; // bookmarked
    }

    // Process replies (we care about the tweet being replied TO)
    for (_tweet_id, replied_to_id) in &reply_pairs {
        let entry = score_map
            .entry(replied_to_id.clone())
            .or_insert((false, false, 0, 0, 0));
        entry.2 += 1; // reply_count
    }

    // Process quotes (we care about the tweet being QUOTED)
    for (_tweet_id, quoted_id) in &quote_pairs {
        let entry = score_map
            .entry(quoted_id.clone())
            .or_insert((false, false, 0, 0, 0));
        entry.3 += 1; // quote_count
    }

    // Process retweets (we care about the tweet being RETWEETED)
    for (_tweet_id, retweeted_id) in &retweet_pairs {
        let entry = score_map
            .entry(retweeted_id.clone())
            .or_insert((false, false, 0, 0, 0));
        entry.4 += 1; // retweet_count
    }

    // Convert to ResonanceScore objects
    let scores: Vec<ResonanceScore> = score_map
        .into_iter()
        .map(
            |(tweet_id, (liked, bookmarked, reply_count, quote_count, retweet_count))| {
                ResonanceScore::new(
                    tweet_id,
                    user_id.to_string(),
                    liked,
                    bookmarked,
                    reply_count,
                    quote_count,
                    retweet_count,
                )
            },
        )
        .collect();

    let unique_tweets = scores.len();

    // Batch insert scores
    let computed = storage.upsert_resonance_scores(&scores).await?;

    Ok(RefreshResult {
        computed,
        unique_tweets,
        previous_count,
    })
}

/// Get interaction stats for reporting.
#[derive(Debug)]
pub struct InteractionStats {
    pub likes: usize,
    pub bookmarks: usize,
    pub replies: usize,
    pub quotes: usize,
    pub retweets: usize,
}

/// Get counts of interactions without computing scores.
pub async fn get_interaction_stats(
    storage: &Arc<dyn Storage>,
    user_id: &str,
) -> anyhow::Result<InteractionStats> {
    let likes = storage.collection_count("likes", user_id).await? as usize;
    let bookmarks = storage.collection_count("bookmarks", user_id).await? as usize;
    let replies = storage.get_user_reply_tweets(user_id, None).await?.len();
    let quotes = storage.get_user_quote_tweets(user_id, None).await?.len();
    let retweets = storage.get_user_retweets(user_id, None).await?.len();

    Ok(InteractionStats {
        likes,
        bookmarks,
        replies,
        quotes,
        retweets,
    })
}
