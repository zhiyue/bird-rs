//! Storage traits for pluggable backends.

use crate::error::Result;
use crate::pagination::SyncState;
use crate::types::{MentionedUser, TweetData};
use async_trait::async_trait;

/// Trait for storing and retrieving tweets.
#[async_trait]
pub trait TweetStore: Send + Sync {
    /// Insert or update a single tweet.
    async fn upsert_tweet(&self, tweet: &TweetData) -> Result<()>;

    /// Insert or update multiple tweets. Returns the number of new tweets inserted.
    async fn upsert_tweets(&self, tweets: &[TweetData]) -> Result<usize>;

    /// Get a tweet by ID.
    async fn get_tweet(&self, id: &str) -> Result<Option<TweetData>>;

    /// Get multiple tweets by ID in a single query.
    async fn get_tweets_by_ids(&self, ids: &[&str]) -> Result<Vec<TweetData>>;

    /// Check if a tweet exists in the store.
    async fn tweet_exists(&self, id: &str) -> Result<bool>;

    /// Filter a list of IDs to return only those that already exist in the store.
    async fn filter_existing_ids(&self, ids: &[&str]) -> Result<Vec<String>>;

    /// Get tweets by collection (likes, bookmarks, etc.) for a user.
    async fn get_tweets_by_collection(
        &self,
        collection: &str,
        user_id: &str,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<Vec<TweetData>>;

    /// Get tweet IDs in a collection (lightweight, no full tweet data).
    async fn get_collection_tweet_ids(
        &self,
        collection: &str,
        user_id: &str,
        limit: Option<u32>,
    ) -> Result<Vec<String>>;

    /// Add a tweet to a collection.
    async fn add_to_collection(
        &self,
        tweet_id: &str,
        collection: &str,
        user_id: &str,
    ) -> Result<()>;

    /// Check if a tweet is in a collection.
    async fn is_in_collection(
        &self,
        tweet_id: &str,
        collection: &str,
        user_id: &str,
    ) -> Result<bool>;

    /// Get the count of tweets in a collection.
    async fn collection_count(&self, collection: &str, user_id: &str) -> Result<u64>;

    /// Get tweets from a collection within a time range (by added_at timestamp).
    async fn get_tweets_by_collection_time_range(
        &self,
        collection: &str,
        user_id: &str,
        start_time: chrono::DateTime<chrono::Utc>,
        end_time: chrono::DateTime<chrono::Utc>,
        limit: Option<u32>,
    ) -> Result<Vec<TweetData>>;

    /// Get tweets that need headlines (text > min_length chars and no headline).
    async fn get_tweets_missing_headlines(
        &self,
        min_length: usize,
        limit: Option<u32>,
    ) -> Result<Vec<TweetData>>;

    /// Update headlines for multiple tweets.
    /// Takes a slice of (tweet_id, headline) pairs.
    async fn update_tweet_headlines(&self, headlines: &[(String, String)]) -> Result<usize>;

    /// Get user's tweets that are replies (have in_reply_to_status_id).
    /// Returns (tweet_id, in_reply_to_status_id) pairs.
    async fn get_user_reply_tweets(
        &self,
        user_id: &str,
        limit: Option<u32>,
    ) -> Result<Vec<(String, String)>>;

    /// Get user's tweets that quote other tweets.
    /// Returns (tweet_id, quoted_tweet_id) pairs.
    async fn get_user_quote_tweets(
        &self,
        user_id: &str,
        limit: Option<u32>,
    ) -> Result<Vec<(String, String)>>;
}

/// Trait for storing sync state.
#[async_trait]
pub trait SyncStateStore: Send + Sync {
    /// Get the sync state for a collection and user.
    async fn get_sync_state(&self, collection: &str, user_id: &str) -> Result<Option<SyncState>>;

    /// Update (or create) the sync state.
    async fn update_sync_state(&self, state: &SyncState) -> Result<()>;

    /// Clear the sync state for a collection and user.
    async fn clear_sync_state(&self, collection: &str, user_id: &str) -> Result<()>;

    /// Get all sync states for a user.
    async fn get_all_sync_states(&self, user_id: &str) -> Result<Vec<SyncState>>;

    /// Get any user_id that has synced data (for offline operations).
    async fn get_any_synced_user_id(&self) -> Result<Option<String>>;
}

/// Trait for storing and retrieving Twitter users.
#[async_trait]
pub trait UserStore: Send + Sync {
    /// Insert or update a user from mention data.
    async fn upsert_user_from_mention(&self, user: &MentionedUser) -> Result<()>;

    /// Get a user by username (case-insensitive).
    async fn get_user_by_username(&self, username: &str) -> Result<Option<MentionedUser>>;

    /// Get a user by ID.
    async fn get_user_by_id(&self, id: &str) -> Result<Option<MentionedUser>>;

    /// Get tweets that mention a specific user.
    async fn get_tweets_mentioning_user(
        &self,
        user_id: &str,
        limit: Option<u32>,
    ) -> Result<Vec<TweetData>>;

    /// Get tweets that are replies to a specific user.
    async fn get_tweets_replying_to_user(
        &self,
        user_id: &str,
        limit: Option<u32>,
    ) -> Result<Vec<TweetData>>;
}

/// Cached resonance score for a tweet.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ResonanceScore {
    /// Tweet ID.
    pub tweet_id: String,
    /// User ID who interacted with this tweet.
    pub user_id: String,
    /// Total resonance score.
    pub total: f64,
    /// Whether the user liked this tweet (+0.5).
    pub liked: bool,
    /// Whether the user bookmarked this tweet (+1.0).
    pub bookmarked: bool,
    /// Number of times the user replied to this tweet (+0.25 each).
    pub reply_count: u32,
    /// Number of times the user quoted this tweet (+0.75 each).
    pub quote_count: u32,
    /// When this score was computed.
    pub computed_at: chrono::DateTime<chrono::Utc>,
}

impl ResonanceScore {
    /// Resonance weight for a like.
    pub const LIKE_WEIGHT: f64 = 0.5;
    /// Resonance weight for a bookmark.
    pub const BOOKMARK_WEIGHT: f64 = 1.0;
    /// Resonance weight for a reply.
    pub const REPLY_WEIGHT: f64 = 0.25;
    /// Resonance weight for a quote.
    pub const QUOTE_WEIGHT: f64 = 0.75;

    /// Calculate total score from components.
    pub fn calculate_total(liked: bool, bookmarked: bool, reply_count: u32, quote_count: u32) -> f64 {
        let mut total = 0.0;
        if liked {
            total += Self::LIKE_WEIGHT;
        }
        if bookmarked {
            total += Self::BOOKMARK_WEIGHT;
        }
        total += reply_count as f64 * Self::REPLY_WEIGHT;
        total += quote_count as f64 * Self::QUOTE_WEIGHT;
        total
    }

    /// Create a new resonance score.
    pub fn new(
        tweet_id: String,
        user_id: String,
        liked: bool,
        bookmarked: bool,
        reply_count: u32,
        quote_count: u32,
    ) -> Self {
        let total = Self::calculate_total(liked, bookmarked, reply_count, quote_count);
        Self {
            tweet_id,
            user_id,
            total,
            liked,
            bookmarked,
            reply_count,
            quote_count,
            computed_at: chrono::Utc::now(),
        }
    }
}

/// Trait for storing resonance scores.
#[async_trait]
pub trait ResonanceStore: Send + Sync {
    /// Get resonance score for a specific tweet.
    async fn get_resonance_score(
        &self,
        tweet_id: &str,
        user_id: &str,
    ) -> Result<Option<ResonanceScore>>;

    /// Get top resonance scores for a user, ordered by total descending.
    async fn get_top_resonance_scores(
        &self,
        user_id: &str,
        limit: u32,
        offset: Option<u32>,
    ) -> Result<Vec<ResonanceScore>>;

    /// Insert or update a single resonance score.
    async fn upsert_resonance_score(&self, score: &ResonanceScore) -> Result<()>;

    /// Insert or update multiple resonance scores.
    async fn upsert_resonance_scores(&self, scores: &[ResonanceScore]) -> Result<usize>;

    /// Clear all resonance scores for a user.
    async fn clear_resonance_scores(&self, user_id: &str) -> Result<u64>;

    /// Get count of resonance scores for a user.
    async fn resonance_score_count(&self, user_id: &str) -> Result<u64>;
}

/// Combined storage trait for convenience.
#[async_trait]
pub trait Storage: TweetStore + SyncStateStore + UserStore + ResonanceStore {}

/// Blanket implementation for types that implement all traits.
impl<T: TweetStore + SyncStateStore + UserStore + ResonanceStore> Storage for T {}
