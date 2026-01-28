//! SurrealDB storage implementation.

use async_trait::async_trait;
use bird_core::{Error, Result, SyncState, SyncStateStore, TweetAuthor, TweetData, TweetStore};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::Path;
use surrealdb::engine::local::{Db, RocksDb};
use surrealdb::Surreal;

/// SurrealDB storage backend using embedded RocksDB.
pub struct SurrealDbStorage {
    db: Surreal<Db>,
}

/// Tweet record for SurrealDB storage (for writing - without id since SurrealDB manages it).
#[derive(Debug, Clone, Serialize)]
struct TweetRecordContent {
    tweet_id: String, // Store the tweet ID as a field for querying
    author_id: Option<String>,
    author_username: String,
    author_name: String,
    text: String,
    created_at: Option<String>,
    reply_count: Option<u64>,
    retweet_count: Option<u64>,
    like_count: Option<u64>,
    conversation_id: Option<String>,
    in_reply_to_status_id: Option<String>,
    media: Option<serde_json::Value>,
    article: Option<serde_json::Value>,
    quoted_tweet: Option<serde_json::Value>,
    fetched_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

/// Tweet record for SurrealDB storage (for reading - includes the tweet_id field).
#[derive(Debug, Clone, Deserialize)]
struct TweetRecord {
    tweet_id: String,
    author_id: Option<String>,
    author_username: String,
    author_name: String,
    text: String,
    created_at: Option<String>,
    reply_count: Option<u64>,
    retweet_count: Option<u64>,
    like_count: Option<u64>,
    conversation_id: Option<String>,
    in_reply_to_status_id: Option<String>,
    media: Option<serde_json::Value>,
    article: Option<serde_json::Value>,
    quoted_tweet: Option<serde_json::Value>,
    #[allow(dead_code)]
    fetched_at: DateTime<Utc>,
    #[allow(dead_code)]
    updated_at: DateTime<Utc>,
}

impl From<&TweetData> for TweetRecordContent {
    fn from(tweet: &TweetData) -> Self {
        let now = Utc::now();
        Self {
            tweet_id: tweet.id.clone(),
            author_id: tweet.author_id.clone(),
            author_username: tweet.author.username.clone(),
            author_name: tweet.author.name.clone(),
            text: tweet.text.clone(),
            created_at: tweet.created_at.clone(),
            reply_count: tweet.reply_count,
            retweet_count: tweet.retweet_count,
            like_count: tweet.like_count,
            conversation_id: tweet.conversation_id.clone(),
            in_reply_to_status_id: tweet.in_reply_to_status_id.clone(),
            media: tweet.media.as_ref().and_then(|m| serde_json::to_value(m).ok()),
            article: tweet.article.as_ref().and_then(|a| serde_json::to_value(a).ok()),
            quoted_tweet: tweet.quoted_tweet.as_ref().and_then(|q| serde_json::to_value(q.as_ref()).ok()),
            fetched_at: now,
            updated_at: now,
        }
    }
}

impl TryFrom<TweetRecord> for TweetData {
    type Error = Error;

    fn try_from(record: TweetRecord) -> Result<Self> {
        Ok(TweetData {
            id: record.tweet_id,
            text: record.text,
            author: TweetAuthor {
                username: record.author_username,
                name: record.author_name,
            },
            author_id: record.author_id,
            created_at: record.created_at,
            reply_count: record.reply_count,
            retweet_count: record.retweet_count,
            like_count: record.like_count,
            conversation_id: record.conversation_id,
            in_reply_to_status_id: record.in_reply_to_status_id,
            quoted_tweet: record.quoted_tweet
                .map(|v| serde_json::from_value(v))
                .transpose()
                .map_err(|e| Error::Serialization(e.to_string()))?,
            media: record.media
                .map(|v| serde_json::from_value(v))
                .transpose()
                .map_err(|e| Error::Serialization(e.to_string()))?,
            article: record.article
                .map(|v| serde_json::from_value(v))
                .transpose()
                .map_err(|e| Error::Serialization(e.to_string()))?,
            _raw: None,
        })
    }
}

/// Sync state record for SurrealDB.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SyncStateRecord {
    collection: String,
    user_id: String,
    newest_item_id: Option<String>,
    oldest_item_id: Option<String>,
    backfill_cursor: Option<String>,
    has_more_history: bool,
    last_sync_at: DateTime<Utc>,
    total_synced: u64,
}

impl From<&SyncState> for SyncStateRecord {
    fn from(state: &SyncState) -> Self {
        Self {
            collection: state.collection.clone(),
            user_id: state.user_id.clone(),
            newest_item_id: state.newest_item_id.clone(),
            oldest_item_id: state.oldest_item_id.clone(),
            backfill_cursor: state.backfill_cursor.clone(),
            has_more_history: state.has_more_history,
            last_sync_at: state.last_sync_at,
            total_synced: state.total_synced,
        }
    }
}

impl From<SyncStateRecord> for SyncState {
    fn from(record: SyncStateRecord) -> Self {
        Self {
            collection: record.collection,
            user_id: record.user_id,
            newest_item_id: record.newest_item_id,
            oldest_item_id: record.oldest_item_id,
            backfill_cursor: record.backfill_cursor,
            has_more_history: record.has_more_history,
            last_sync_at: record.last_sync_at,
            total_synced: record.total_synced,
        }
    }
}

impl SurrealDbStorage {
    /// Create a new SurrealDB storage at the default path.
    pub async fn new_default() -> Result<Self> {
        let path = crate::default_db_path();
        Self::new(&path).await
    }

    /// Create a new SurrealDB storage at the specified path.
    pub async fn new(path: &Path) -> Result<Self> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| Error::Storage(format!("Failed to create directory: {}", e)))?;
        }

        let db = Surreal::new::<RocksDb>(path)
            .await
            .map_err(|e| Error::Storage(format!("Failed to open database: {}", e)))?;

        // Select namespace and database
        db.use_ns("bird")
            .use_db("main")
            .await
            .map_err(|e| Error::Storage(format!("Failed to select database: {}", e)))?;

        let storage = Self { db };
        storage.init_schema().await?;
        Ok(storage)
    }

    /// Initialize the database schema.
    async fn init_schema(&self) -> Result<()> {
        // Create tables and indexes using raw queries
        self.db
            .query("DEFINE TABLE IF NOT EXISTS tweet SCHEMALESS")
            .await
            .map_err(|e| Error::Storage(format!("Failed to create tweet table: {}", e)))?;

        self.db
            .query("DEFINE INDEX IF NOT EXISTS tweet_id ON tweet FIELDS id UNIQUE")
            .await
            .map_err(|e| Error::Storage(format!("Failed to create tweet index: {}", e)))?;

        self.db
            .query("DEFINE TABLE IF NOT EXISTS tweet_collection SCHEMALESS")
            .await
            .map_err(|e| Error::Storage(format!("Failed to create collection table: {}", e)))?;

        self.db
            .query("DEFINE INDEX IF NOT EXISTS tweet_collection_pk ON tweet_collection FIELDS tweet_id, collection, user_id UNIQUE")
            .await
            .map_err(|e| Error::Storage(format!("Failed to create collection index: {}", e)))?;

        self.db
            .query("DEFINE TABLE IF NOT EXISTS sync_state SCHEMALESS")
            .await
            .map_err(|e| Error::Storage(format!("Failed to create sync_state table: {}", e)))?;

        self.db
            .query("DEFINE INDEX IF NOT EXISTS sync_state_pk ON sync_state FIELDS collection, user_id UNIQUE")
            .await
            .map_err(|e| Error::Storage(format!("Failed to create sync_state index: {}", e)))?;

        Ok(())
    }
}

#[async_trait]
impl TweetStore for SurrealDbStorage {
    async fn upsert_tweet(&self, tweet: &TweetData) -> Result<()> {
        let content = TweetRecordContent::from(tweet);
        let id = &tweet.id;

        // Convert to JSON Value first to avoid SurrealDB serialization issues
        let json_content = serde_json::to_value(&content)
            .map_err(|e| Error::Storage(format!("Failed to serialize tweet: {}", e)))?;

        // Build query with record ID in the query string (cannot be parameterized)
        let query = format!("UPSERT tweet:⟨{}⟩ CONTENT $content", id);
        self.db
            .query(&query)
            .bind(("content", json_content))
            .await
            .map_err(|e| Error::Storage(format!("Failed to upsert tweet: {}", e)))?;

        Ok(())
    }

    async fn upsert_tweets(&self, tweets: &[TweetData]) -> Result<usize> {
        let mut new_count = 0;

        for tweet in tweets {
            // Check if exists
            let exists = self.tweet_exists(&tweet.id).await?;
            if !exists {
                new_count += 1;
            }
            self.upsert_tweet(tweet).await?;
        }

        Ok(new_count)
    }

    async fn get_tweet(&self, id: &str) -> Result<Option<TweetData>> {
        // Query using the record ID syntax
        let query = format!("SELECT * FROM tweet:⟨{}⟩", id);
        let mut result = self.db
            .query(&query)
            .await
            .map_err(|e| Error::Storage(format!("Failed to get tweet: {}", e)))?;

        let records: Vec<TweetRecord> = result.take(0)
            .map_err(|e| Error::Storage(format!("Failed to parse tweet: {}", e)))?;

        if let Some(record) = records.into_iter().next() {
            Ok(Some(record.try_into()?))
        } else {
            Ok(None)
        }
    }

    async fn tweet_exists(&self, id: &str) -> Result<bool> {
        // Query using the record ID syntax
        let query = format!("SELECT tweet_id FROM tweet:⟨{}⟩", id);
        let mut result = self.db
            .query(&query)
            .await
            .map_err(|e| Error::Storage(format!("Failed to check tweet: {}", e)))?;

        let records: Vec<serde_json::Value> = result.take(0)
            .map_err(|e| Error::Storage(format!("Failed to parse result: {}", e)))?;

        Ok(!records.is_empty())
    }

    async fn filter_existing_ids(&self, ids: &[&str]) -> Result<Vec<String>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }

        // Build record ID references for SurrealDB (tweet:id format)
        let record_refs: Vec<String> = ids.iter().map(|id| format!("tweet:⟨{}⟩", id)).collect();
        let query = format!("SELECT tweet_id FROM [{}]", record_refs.join(", "));

        let mut result = self.db
            .query(&query)
            .await
            .map_err(|e| Error::Storage(format!("Failed to filter ids: {}", e)))?;

        #[derive(Deserialize)]
        struct IdRecord { tweet_id: String }

        let records: Vec<IdRecord> = result.take(0)
            .map_err(|e| Error::Storage(format!("Failed to parse ids: {}", e)))?;

        Ok(records.into_iter().map(|r| r.tweet_id).collect())
    }

    async fn get_tweets_by_collection(
        &self,
        collection: &str,
        user_id: &str,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<Vec<TweetData>> {
        let limit = limit.unwrap_or(100);
        let offset = offset.unwrap_or(0);
        let collection_owned = collection.to_string();
        let user_id_owned = user_id.to_string();

        // First get ALL tweet IDs from the collection (no pagination here)
        let mut coll_result = self.db
            .query(
                "SELECT tweet_id FROM tweet_collection
                 WHERE collection = $collection AND user_id = $user_id"
            )
            .bind(("collection", collection_owned))
            .bind(("user_id", user_id_owned))
            .await
            .map_err(|e| Error::Storage(format!("Failed to get collection: {}", e)))?;

        #[derive(Deserialize)]
        struct CollectionRecord {
            tweet_id: String,
        }

        let coll_records: Vec<CollectionRecord> = coll_result
            .take(0)
            .map_err(|e| Error::Storage(format!("Failed to parse collection: {}", e)))?;

        if coll_records.is_empty() {
            return Ok(Vec::new());
        }

        // Collect tweet IDs
        let tweet_ids: Vec<String> = coll_records.iter().map(|r| r.tweet_id.clone()).collect();

        // Fetch tweets using SurrealDB record ID syntax, sorted by created_at DESC
        // Build array of record references: [tweet:id1, tweet:id2, ...]
        let record_refs: Vec<String> = tweet_ids.iter().map(|id| format!("tweet:⟨{}⟩", id)).collect();
        let query = format!(
            "SELECT * FROM [{}] ORDER BY created_at DESC LIMIT {} START {}",
            record_refs.join(", "),
            limit,
            offset
        );

        let mut tweet_result = self.db
            .query(&query)
            .await
            .map_err(|e| Error::Storage(format!("Failed to get tweets: {}", e)))?;

        let records: Vec<TweetRecord> = tweet_result
            .take(0)
            .map_err(|e| Error::Storage(format!("Failed to parse tweets: {}", e)))?;

        // Convert to TweetData (already sorted by query)
        let ordered_tweets: Vec<TweetData> = records
            .into_iter()
            .filter_map(|r| r.try_into().ok())
            .collect();

        Ok(ordered_tweets)
    }

    async fn add_to_collection(
        &self,
        tweet_id: &str,
        collection: &str,
        user_id: &str,
    ) -> Result<()> {
        let tweet_id_owned = tweet_id.to_string();
        let collection_owned = collection.to_string();
        let user_id_owned = user_id.to_string();
        let now = Utc::now();

        // Check if already exists
        let exists = self
            .is_in_collection(tweet_id, collection, user_id)
            .await?;

        if exists {
            // Update existing record
            self.db
                .query(
                    "UPDATE tweet_collection SET added_at = $added_at
                     WHERE tweet_id = $tweet_id AND collection = $collection AND user_id = $user_id"
                )
                .bind(("tweet_id", tweet_id_owned))
                .bind(("collection", collection_owned))
                .bind(("user_id", user_id_owned))
                .bind(("added_at", now))
                .await
                .map_err(|e| Error::Storage(format!("Failed to update collection: {}", e)))?;
        } else {
            // Insert new record using CREATE (SurrealDB 2.x preferred method)
            self.db
                .query(
                    "CREATE tweet_collection CONTENT {
                        tweet_id: $tweet_id,
                        collection: $collection,
                        user_id: $user_id,
                        added_at: $added_at
                    }"
                )
                .bind(("tweet_id", tweet_id_owned))
                .bind(("collection", collection_owned))
                .bind(("user_id", user_id_owned))
                .bind(("added_at", now))
                .await
                .map_err(|e| Error::Storage(format!("Failed to add to collection: {}", e)))?;
        }

        Ok(())
    }

    async fn is_in_collection(
        &self,
        tweet_id: &str,
        collection: &str,
        user_id: &str,
    ) -> Result<bool> {
        let tweet_id_owned = tweet_id.to_string();
        let collection_owned = collection.to_string();
        let user_id_owned = user_id.to_string();

        let mut result = self.db
            .query(
                "SELECT tweet_id FROM tweet_collection
                 WHERE tweet_id = $tweet_id
                 AND collection = $collection
                 AND user_id = $user_id
                 LIMIT 1"
            )
            .bind(("tweet_id", tweet_id_owned))
            .bind(("collection", collection_owned))
            .bind(("user_id", user_id_owned))
            .await
            .map_err(|e| Error::Storage(format!("Failed to check collection: {}", e)))?;

        let records: Vec<serde_json::Value> = result.take(0)
            .map_err(|e| Error::Storage(format!("Failed to parse result: {}", e)))?;

        Ok(!records.is_empty())
    }

    async fn collection_count(&self, collection: &str, user_id: &str) -> Result<u64> {
        let collection_owned = collection.to_string();
        let user_id_owned = user_id.to_string();

        let mut result = self.db
            .query(
                "SELECT count() as count FROM tweet_collection
                 WHERE collection = $collection AND user_id = $user_id
                 GROUP ALL"
            )
            .bind(("collection", collection_owned))
            .bind(("user_id", user_id_owned))
            .await
            .map_err(|e| Error::Storage(format!("Failed to count collection: {}", e)))?;

        #[derive(Deserialize)]
        struct CountResult { count: u64 }

        let records: Vec<CountResult> = result.take(0)
            .map_err(|e| Error::Storage(format!("Failed to parse count: {}", e)))?;

        Ok(records.first().map(|r| r.count).unwrap_or(0))
    }
}

#[async_trait]
impl SyncStateStore for SurrealDbStorage {
    async fn get_sync_state(&self, collection: &str, user_id: &str) -> Result<Option<SyncState>> {
        let collection_owned = collection.to_string();
        let user_id_owned = user_id.to_string();

        let mut result = self.db
            .query(
                "SELECT * FROM sync_state
                 WHERE collection = $collection AND user_id = $user_id
                 LIMIT 1"
            )
            .bind(("collection", collection_owned))
            .bind(("user_id", user_id_owned))
            .await
            .map_err(|e| Error::Storage(format!("Failed to get sync state: {}", e)))?;

        let records: Vec<SyncStateRecord> = result.take(0)
            .map_err(|e| Error::Storage(format!("Failed to parse sync state: {}", e)))?;

        Ok(records.into_iter().next().map(|r| r.into()))
    }

    async fn update_sync_state(&self, state: &SyncState) -> Result<()> {
        let record = SyncStateRecord::from(state);
        let collection_owned = state.collection.clone();
        let user_id_owned = state.user_id.clone();

        self.db
            .query(
                "UPSERT sync_state CONTENT $record
                 WHERE collection = $collection AND user_id = $user_id"
            )
            .bind(("record", record))
            .bind(("collection", collection_owned))
            .bind(("user_id", user_id_owned))
            .await
            .map_err(|e| Error::Storage(format!("Failed to update sync state: {}", e)))?;

        Ok(())
    }

    async fn clear_sync_state(&self, collection: &str, user_id: &str) -> Result<()> {
        let collection_owned = collection.to_string();
        let user_id_owned = user_id.to_string();

        self.db
            .query(
                "DELETE FROM sync_state
                 WHERE collection = $collection AND user_id = $user_id"
            )
            .bind(("collection", collection_owned))
            .bind(("user_id", user_id_owned))
            .await
            .map_err(|e| Error::Storage(format!("Failed to clear sync state: {}", e)))?;

        Ok(())
    }

    async fn get_all_sync_states(&self, user_id: &str) -> Result<Vec<SyncState>> {
        let user_id_owned = user_id.to_string();

        let mut result = self.db
            .query("SELECT * FROM sync_state WHERE user_id = $user_id")
            .bind(("user_id", user_id_owned))
            .await
            .map_err(|e| Error::Storage(format!("Failed to get sync states: {}", e)))?;

        let records: Vec<SyncStateRecord> = result.take(0)
            .map_err(|e| Error::Storage(format!("Failed to parse sync states: {}", e)))?;

        Ok(records.into_iter().map(|r| r.into()).collect())
    }
}
