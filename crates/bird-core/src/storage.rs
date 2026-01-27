//! Storage traits for pluggable backends.

use crate::error::Result;
use crate::pagination::SyncState;
use crate::types::TweetData;
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
}

/// Combined storage trait for convenience.
#[async_trait]
pub trait Storage: TweetStore + SyncStateStore {}

/// Blanket implementation for types that implement both traits.
impl<T: TweetStore + SyncStateStore> Storage for T {}
