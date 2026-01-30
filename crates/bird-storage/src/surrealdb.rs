//! SurrealDB storage implementation.

use async_trait::async_trait;
use bird_core::{
    Error, MentionedUser, ResonanceScore, ResonanceStore, Result, SyncState, SyncStateStore,
    TweetAuthor, TweetData, TweetStore, UserStore,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::Path;
use surrealdb::engine::any::{connect, Any};
use surrealdb::opt::auth::{Database, Namespace, Root};
use surrealdb::Surreal;

/// SurrealDB storage backend using embedded RocksDB.
pub struct SurrealDbStorage {
    db: Surreal<Any>,
}

/// Result of backfilling created_at_ts.
#[derive(Debug, Clone, Copy)]
pub struct BackfillCreatedAtResult {
    /// Number of tweets updated with parsed timestamps.
    pub updated: usize,
    /// Number of tweets set to a fallback timestamp (unparseable or missing).
    pub skipped: usize,
}

/// Aggregate counts for database status.
#[derive(Debug, Clone, Copy)]
pub struct DbStats {
    /// Total tweets stored.
    pub tweets: u64,
    /// Total collection entries.
    pub collections: u64,
    /// Total bookmark entries.
    pub bookmarks: u64,
    /// Total like entries.
    pub likes: u64,
    /// Total sync state entries.
    pub sync_states: u64,
    /// Tweets missing created_at_ts.
    pub missing_created_at_ts: u64,
    /// Oldest tweet timestamp (Unix seconds).
    pub oldest_tweet_ts: Option<i64>,
    /// Newest tweet timestamp (Unix seconds).
    pub newest_tweet_ts: Option<i64>,
}

/// Authentication configuration for remote SurrealDB connections.
#[derive(Debug, Clone)]
pub enum SurrealDbAuth {
    /// Root user authentication.
    Root { username: String, password: String },
    /// Namespace user authentication.
    Namespace { username: String, password: String },
    /// Database user authentication.
    Database { username: String, password: String },
}

/// Configuration for creating a SurrealDB storage backend.
#[derive(Debug, Clone)]
pub struct SurrealDbConfig {
    /// Connection endpoint (e.g., "rocksdb://path", "ws://host:8000", "https://cloud.surrealdb.com").
    pub endpoint: String,
    /// Namespace to use.
    pub namespace: String,
    /// Database to use.
    pub database: String,
    /// Optional authentication.
    pub auth: Option<SurrealDbAuth>,
}

impl SurrealDbConfig {
    /// Create a local RocksDB-backed configuration using the default namespace/database.
    pub fn local(path: &Path) -> Self {
        Self {
            endpoint: format!("rocksdb://{}", path.to_string_lossy()),
            namespace: "bird".to_string(),
            database: "main".to_string(),
            auth: None,
        }
    }
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
    created_at_ts: Option<i64>,
    reply_count: Option<u64>,
    retweet_count: Option<u64>,
    like_count: Option<u64>,
    conversation_id: Option<String>,
    in_reply_to_status_id: Option<String>,
    in_reply_to_user_id: Option<String>,
    mentions: Vec<MentionedUserRecord>,
    media: Option<serde_json::Value>,
    article: Option<serde_json::Value>,
    /// ID of the quoted tweet (normalized - actual tweet stored separately)
    quoted_tweet_id: Option<String>,
    /// ID of the retweeted tweet (normalized - actual tweet stored separately)
    retweeted_tweet_id: Option<String>,
    headline: Option<String>,
    fetched_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

/// Mentioned user record for storage.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct MentionedUserRecord {
    id: String,
    username: String,
    name: Option<String>,
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
    #[serde(default)]
    #[allow(dead_code)]
    created_at_ts: Option<i64>,
    reply_count: Option<u64>,
    retweet_count: Option<u64>,
    like_count: Option<u64>,
    conversation_id: Option<String>,
    in_reply_to_status_id: Option<String>,
    #[serde(default)]
    in_reply_to_user_id: Option<String>,
    #[serde(default)]
    mentions: Vec<MentionedUserRecord>,
    media: Option<serde_json::Value>,
    article: Option<serde_json::Value>,
    /// ID of the quoted tweet (normalized)
    #[serde(default)]
    quoted_tweet_id: Option<String>,
    /// ID of the retweeted tweet (normalized)
    #[serde(default)]
    retweeted_tweet_id: Option<String>,
    #[serde(default)]
    headline: Option<String>,
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
            created_at_ts: tweet
                .created_at
                .as_deref()
                .and_then(parse_twitter_timestamp),
            reply_count: tweet.reply_count,
            retweet_count: tweet.retweet_count,
            like_count: tweet.like_count,
            conversation_id: tweet.conversation_id.clone(),
            in_reply_to_status_id: tweet.in_reply_to_status_id.clone(),
            in_reply_to_user_id: tweet.in_reply_to_user_id.clone(),
            mentions: tweet
                .mentions
                .iter()
                .map(|m| MentionedUserRecord {
                    id: m.id.clone(),
                    username: m.username.clone(),
                    name: m.name.clone(),
                })
                .collect(),
            media: tweet
                .media
                .as_ref()
                .and_then(|m| serde_json::to_value(m).ok()),
            article: tweet
                .article
                .as_ref()
                .and_then(|a| serde_json::to_value(a).ok()),
            // Store just the ID - the referenced tweet is stored separately
            quoted_tweet_id: tweet.quoted_tweet.as_ref().map(|q| q.id.clone()),
            retweeted_tweet_id: tweet.retweeted_tweet.as_ref().map(|r| r.id.clone()),
            headline: tweet.headline.clone(),
            fetched_at: now,
            updated_at: now,
        }
    }
}

/// Parse Twitter's created_at string into a Unix timestamp.
fn parse_twitter_timestamp(value: &str) -> Option<i64> {
    DateTime::parse_from_str(value, "%a %b %d %H:%M:%S %z %Y")
        .ok()
        .map(|dt| dt.timestamp())
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
            in_reply_to_user_id: record.in_reply_to_user_id,
            mentions: record
                .mentions
                .into_iter()
                .map(|m| MentionedUser {
                    id: m.id,
                    username: m.username,
                    name: m.name,
                })
                .collect(),
            // quoted_tweet and retweeted_tweet are hydrated separately
            // The IDs are stored in the record but not converted here
            quoted_tweet: None,
            retweeted_tweet: None,
            media: record
                .media
                .map(serde_json::from_value)
                .transpose()
                .map_err(|e| Error::Serialization(e.to_string()))?,
            article: record
                .article
                .map(serde_json::from_value)
                .transpose()
                .map_err(|e| Error::Serialization(e.to_string()))?,
            headline: record.headline,
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
    last_rate_limited_at: Option<DateTime<Utc>>,
    last_rate_limit_backoff_ms: Option<u64>,
    last_rate_limit_retries: Option<u32>,
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
            last_rate_limited_at: state.last_rate_limited_at,
            last_rate_limit_backoff_ms: state.last_rate_limit_backoff_ms,
            last_rate_limit_retries: state.last_rate_limit_retries,
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
            last_rate_limited_at: record.last_rate_limited_at,
            last_rate_limit_backoff_ms: record.last_rate_limit_backoff_ms,
            last_rate_limit_retries: record.last_rate_limit_retries,
            total_synced: record.total_synced,
        }
    }
}

/// Debug info about timestamps.
#[derive(Debug)]
pub struct TimestampDebugInfo {
    pub none_count: u64,
    pub zero_count: u64,
    pub valid_count: u64,
    pub distinct_count: u64,
    pub min_ts: Option<i64>,
    pub max_ts: Option<i64>,
    pub distribution: Vec<(i64, u64)>,
    /// Tweet ID with the oldest timestamp.
    pub oldest_tweet_id: Option<String>,
    /// Tweet ID with the newest timestamp.
    pub newest_tweet_id: Option<String>,
}

impl SurrealDbStorage {
    /// Create a new SurrealDB storage at the default path.
    pub async fn new_default() -> Result<Self> {
        let path = crate::default_db_path();
        Self::new_local(&path).await
    }

    /// Create a new SurrealDB storage at the specified path.
    pub async fn new(path: &Path) -> Result<Self> {
        Self::new_local(path).await
    }

    /// Create a new local SurrealDB storage at the specified path.
    pub async fn new_local(path: &Path) -> Result<Self> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| Error::Storage(format!("Failed to create directory: {}", e)))?;
        }

        let config = SurrealDbConfig::local(path);
        Self::new_with_config(&config).await
    }

    /// Create a new SurrealDB storage using a custom configuration.
    pub async fn new_with_config(config: &SurrealDbConfig) -> Result<Self> {
        let db = connect(&config.endpoint)
            .await
            .map_err(|e| Error::Storage(format!("Failed to connect to database: {}", e)))?;

        if let Some(auth) = &config.auth {
            match auth {
                SurrealDbAuth::Root { username, password } => {
                    db.signin(Root { username, password })
                        .await
                        .map_err(|e| Error::Storage(format!("Failed to sign in: {}", e)))?;
                }
                SurrealDbAuth::Namespace { username, password } => {
                    db.signin(Namespace {
                        namespace: &config.namespace,
                        username,
                        password,
                    })
                    .await
                    .map_err(|e| Error::Storage(format!("Failed to sign in: {}", e)))?;
                }
                SurrealDbAuth::Database { username, password } => {
                    db.signin(Database {
                        namespace: &config.namespace,
                        database: &config.database,
                        username,
                        password,
                    })
                    .await
                    .map_err(|e| Error::Storage(format!("Failed to sign in: {}", e)))?;
                }
            }
        }

        // Select namespace and database
        db.use_ns(&config.namespace)
            .use_db(&config.database)
            .await
            .map_err(|e| Error::Storage(format!("Failed to select database: {}", e)))?;

        let storage = Self { db };
        storage.init_schema().await?;
        Ok(storage)
    }

    /// Backfill created_at_ts for tweets missing the timestamp.
    pub async fn backfill_created_at_ts(&self, batch_size: u32) -> Result<BackfillCreatedAtResult> {
        if batch_size == 0 {
            return Err(Error::Storage(
                "Batch size must be greater than 0".to_string(),
            ));
        }

        #[derive(Deserialize)]
        struct CreatedAtRecord {
            tweet_id: String,
            created_at: Option<String>,
        }

        let mut updated = 0usize;
        let mut skipped = 0usize;

        loop {
            let mut result = self
                .db
                .query(
                    "SELECT tweet_id, created_at FROM tweet WHERE created_at_ts IS NONE LIMIT $limit",
                )
                .bind(("limit", batch_size))
                .await
                .map_err(|e| Error::Storage(format!("Failed to fetch tweets: {e}")))?;

            let records: Vec<CreatedAtRecord> = result
                .take(0)
                .map_err(|e| Error::Storage(format!("Failed to parse tweets: {e}")))?;

            if records.is_empty() {
                break;
            }

            for record in records {
                let parsed = record
                    .created_at
                    .as_deref()
                    .and_then(parse_twitter_timestamp);
                let created_at_ts = parsed.unwrap_or(0);

                if parsed.is_some() {
                    updated += 1;
                } else {
                    skipped += 1;
                }

                let query = format!(
                    "UPDATE tweet:⟨{}⟩ SET created_at_ts = $created_at_ts",
                    record.tweet_id
                );
                self.db
                    .query(&query)
                    .bind(("created_at_ts", created_at_ts))
                    .await
                    .map_err(|e| Error::Storage(format!("Failed to update tweet: {e}")))?;
            }
        }

        Ok(BackfillCreatedAtResult { updated, skipped })
    }

    /// Ensure database schema and indexes exist.
    pub async fn ensure_schema(&self) -> Result<()> {
        self.init_schema().await
    }

    /// Debug: Get distribution of created_at_ts values.
    pub async fn debug_timestamp_distribution(&self) -> Result<TimestampDebugInfo> {
        // Count tweets with no created_at_ts field
        let none_count = self
            .count_query("SELECT count() as count FROM tweet WHERE created_at_ts IS NONE GROUP ALL")
            .await?;

        // Count tweets with created_at_ts = 0
        let zero_count = self
            .count_query("SELECT count() as count FROM tweet WHERE created_at_ts = 0 GROUP ALL")
            .await?;

        // Count tweets with valid timestamps
        let valid_count = self
            .count_query(
                "SELECT count() as count FROM tweet WHERE created_at_ts IS NOT NONE AND created_at_ts > 0 GROUP ALL",
            )
            .await?;

        // Count distinct timestamps by counting groups
        let mut distinct_result = self
            .db
            .query(
                "SELECT created_at_ts, count() as count FROM tweet WHERE created_at_ts IS NOT NONE AND created_at_ts > 0 GROUP BY created_at_ts",
            )
            .await
            .map_err(|e| Error::Storage(format!("Failed to fetch distinct count: {e}")))?;

        #[derive(Deserialize)]
        struct GroupCountResult {
            #[allow(dead_code)]
            created_at_ts: i64,
            #[allow(dead_code)]
            count: u64,
        }

        let distinct_records: Vec<GroupCountResult> = distinct_result
            .take(0)
            .map_err(|e| Error::Storage(format!("Failed to parse distinct count: {e}")))?;

        let distinct_count = distinct_records.len() as u64;

        // Get actual MIN and MAX using math::min/max
        let mut minmax_result = self
            .db
            .query(
                "SELECT math::min(created_at_ts) as min_ts, math::max(created_at_ts) as max_ts FROM tweet WHERE created_at_ts IS NOT NONE AND created_at_ts > 0 GROUP ALL",
            )
            .await
            .map_err(|e| Error::Storage(format!("Failed to fetch min/max: {e}")))?;

        #[derive(Deserialize)]
        struct MinMaxRecord {
            min_ts: Option<i64>,
            max_ts: Option<i64>,
        }

        let minmax_records: Vec<MinMaxRecord> = minmax_result
            .take(0)
            .map_err(|e| Error::Storage(format!("Failed to parse min/max: {e}")))?;

        let (min_ts, max_ts) = minmax_records
            .first()
            .map(|r| (r.min_ts, r.max_ts))
            .unwrap_or((None, None));

        let mut result = self
            .db
            .query(
                "SELECT created_at_ts, count() as count FROM tweet
                 WHERE created_at_ts IS NOT NONE AND created_at_ts > 0
                 GROUP BY created_at_ts
                 ORDER BY count DESC
                 LIMIT 10",
            )
            .await
            .map_err(|e| Error::Storage(format!("Failed to fetch distribution: {e}")))?;

        #[derive(Deserialize)]
        struct DistRecord {
            created_at_ts: i64,
            count: u64,
        }

        let records: Vec<DistRecord> = result
            .take(0)
            .map_err(|e| Error::Storage(format!("Failed to parse distribution: {e}")))?;

        let distribution: Vec<(i64, u64)> = records
            .into_iter()
            .map(|r| (r.created_at_ts, r.count))
            .collect();

        // Find tweet IDs for oldest and newest timestamps
        let oldest_tweet_id = if let Some(ts) = min_ts {
            let mut oldest_result = self
                .db
                .query("SELECT tweet_id FROM tweet WHERE created_at_ts = $ts LIMIT 1")
                .bind(("ts", ts))
                .await
                .map_err(|e| Error::Storage(format!("Failed to fetch oldest tweet: {e}")))?;

            #[derive(Deserialize)]
            struct TweetIdRecord {
                tweet_id: String,
            }

            let oldest_records: Vec<TweetIdRecord> = oldest_result
                .take(0)
                .map_err(|e| Error::Storage(format!("Failed to parse oldest tweet: {e}")))?;

            oldest_records.into_iter().next().map(|r| r.tweet_id)
        } else {
            None
        };

        let newest_tweet_id = if let Some(ts) = max_ts {
            let mut newest_result = self
                .db
                .query("SELECT tweet_id FROM tweet WHERE created_at_ts = $ts LIMIT 1")
                .bind(("ts", ts))
                .await
                .map_err(|e| Error::Storage(format!("Failed to fetch newest tweet: {e}")))?;

            #[derive(Deserialize)]
            struct TweetIdRecord {
                tweet_id: String,
            }

            let newest_records: Vec<TweetIdRecord> = newest_result
                .take(0)
                .map_err(|e| Error::Storage(format!("Failed to parse newest tweet: {e}")))?;

            newest_records.into_iter().next().map(|r| r.tweet_id)
        } else {
            None
        };

        Ok(TimestampDebugInfo {
            none_count,
            zero_count,
            valid_count,
            distinct_count,
            min_ts,
            max_ts,
            distribution,
            oldest_tweet_id,
            newest_tweet_id,
        })
    }

    /// Get aggregate counts for database status.
    pub async fn stats(&self) -> Result<DbStats> {
        let tweets = self
            .count_query("SELECT count() as count FROM tweet GROUP ALL")
            .await?;
        let collections = self
            .count_query("SELECT count() as count FROM tweet_collection GROUP ALL")
            .await?;
        let bookmarks = self
            .count_query(
                "SELECT count() as count FROM tweet_collection WHERE collection = 'bookmarks' GROUP ALL",
            )
            .await?;
        let likes = self
            .count_query(
                "SELECT count() as count FROM tweet_collection WHERE collection = 'likes' GROUP ALL",
            )
            .await?;
        let sync_states = self
            .count_query("SELECT count() as count FROM sync_state GROUP ALL")
            .await?;
        let missing_created_at_ts = self
            .count_query("SELECT count() as count FROM tweet WHERE created_at_ts IS NONE GROUP ALL")
            .await?;
        let oldest_tweet_ts = self.created_at_bound(false).await?;
        let newest_tweet_ts = self.created_at_bound(true).await?;

        Ok(DbStats {
            tweets,
            collections,
            bookmarks,
            likes,
            sync_states,
            missing_created_at_ts,
            oldest_tweet_ts,
            newest_tweet_ts,
        })
    }

    async fn count_query(&self, query: &str) -> Result<u64> {
        let mut result = self
            .db
            .query(query)
            .await
            .map_err(|e| Error::Storage(format!("Failed to fetch count: {e}")))?;

        #[derive(Deserialize)]
        struct CountResult {
            count: u64,
        }

        let records: Vec<CountResult> = result
            .take(0)
            .map_err(|e| Error::Storage(format!("Failed to parse count: {e}")))?;

        Ok(records.first().map(|r| r.count).unwrap_or(0))
    }

    async fn created_at_bound(&self, newest: bool) -> Result<Option<i64>> {
        // Use math::min/max instead of ORDER BY which has issues in SurrealDB
        let func = if newest { "math::max" } else { "math::min" };
        let query = format!(
            "SELECT {}(created_at_ts) as created_at_ts FROM tweet WHERE created_at_ts IS NOT NONE AND created_at_ts > 0 GROUP ALL",
            func
        );
        let mut result = self
            .db
            .query(&query)
            .await
            .map_err(|e| Error::Storage(format!("Failed to fetch timestamp: {e}")))?;

        #[derive(Deserialize)]
        struct CreatedAtResult {
            created_at_ts: Option<i64>,
        }

        let records: Vec<CreatedAtResult> = result
            .take(0)
            .map_err(|e| Error::Storage(format!("Failed to parse timestamp: {e}")))?;

        Ok(records
            .first()
            .and_then(|r| r.created_at_ts)
            .filter(|ts| *ts > 0))
    }

    /// Initialize the database schema.
    async fn init_schema(&self) -> Result<()> {
        // Create tables and indexes using raw queries
        self.db
            .query("DEFINE TABLE IF NOT EXISTS tweet SCHEMALESS")
            .await
            .map_err(|e| Error::Storage(format!("Failed to create tweet table: {}", e)))?;

        self.db
            .query("DEFINE INDEX IF NOT EXISTS tweet_id ON tweet FIELDS tweet_id UNIQUE")
            .await
            .map_err(|e| Error::Storage(format!("Failed to create tweet index: {}", e)))?;

        self.db
            .query("DEFINE INDEX IF NOT EXISTS tweet_created_at_ts ON tweet FIELDS created_at_ts")
            .await
            .map_err(|e| {
                Error::Storage(format!("Failed to create tweet timestamp index: {}", e))
            })?;

        self.db
            .query("DEFINE TABLE IF NOT EXISTS tweet_collection SCHEMALESS")
            .await
            .map_err(|e| Error::Storage(format!("Failed to create collection table: {}", e)))?;

        self.db
            .query("DEFINE INDEX IF NOT EXISTS tweet_collection_pk ON tweet_collection FIELDS tweet_id, collection, user_id UNIQUE")
            .await
            .map_err(|e| Error::Storage(format!("Failed to create collection index: {}", e)))?;

        self.db
            .query("DEFINE INDEX IF NOT EXISTS tweet_collection_lookup ON tweet_collection FIELDS collection, user_id")
            .await
            .map_err(|e| Error::Storage(format!("Failed to create collection lookup index: {}", e)))?;

        self.db
            .query("DEFINE INDEX IF NOT EXISTS tweet_collection_added_at ON tweet_collection FIELDS collection, user_id, added_at")
            .await
            .map_err(|e| Error::Storage(format!("Failed to create collection added_at index: {}", e)))?;

        self.db
            .query("DEFINE TABLE IF NOT EXISTS sync_state SCHEMALESS")
            .await
            .map_err(|e| Error::Storage(format!("Failed to create sync_state table: {}", e)))?;

        self.db
            .query("DEFINE INDEX IF NOT EXISTS sync_state_pk ON sync_state FIELDS collection, user_id UNIQUE")
            .await
            .map_err(|e| Error::Storage(format!("Failed to create sync_state index: {}", e)))?;

        // Twitter users table for storing mentioned users
        self.db
            .query("DEFINE TABLE IF NOT EXISTS twitter_user SCHEMALESS")
            .await
            .map_err(|e| Error::Storage(format!("Failed to create twitter_user table: {}", e)))?;

        self.db
            .query("DEFINE INDEX IF NOT EXISTS twitter_user_username ON twitter_user FIELDS username_lower UNIQUE")
            .await
            .map_err(|e| Error::Storage(format!("Failed to create twitter_user username index: {}", e)))?;

        // Index for finding replies by in_reply_to_status_id
        self.db
            .query(
                "DEFINE INDEX IF NOT EXISTS tweet_reply_to ON tweet FIELDS in_reply_to_status_id",
            )
            .await
            .map_err(|e| Error::Storage(format!("Failed to create tweet reply_to index: {}", e)))?;

        // Resonance score table and indexes
        self.db
            .query("DEFINE TABLE IF NOT EXISTS resonance_score SCHEMALESS")
            .await
            .map_err(|e| {
                Error::Storage(format!("Failed to create resonance_score table: {}", e))
            })?;

        self.db
            .query("DEFINE INDEX IF NOT EXISTS resonance_score_pk ON resonance_score FIELDS tweet_id, user_id UNIQUE")
            .await
            .map_err(|e| Error::Storage(format!("Failed to create resonance_score pk index: {}", e)))?;

        self.db
            .query("DEFINE INDEX IF NOT EXISTS resonance_score_lookup ON resonance_score FIELDS user_id")
            .await
            .map_err(|e| Error::Storage(format!("Failed to create resonance_score lookup index: {}", e)))?;

        Ok(())
    }

    /// Hydrate a list of TweetRecords by fetching their referenced tweets in batch.
    /// Returns a Vec of hydrated TweetData.
    async fn hydrate_tweet_records(&self, records: Vec<TweetRecord>) -> Result<Vec<TweetData>> {
        if records.is_empty() {
            return Ok(Vec::new());
        }

        // Collect all referenced tweet IDs
        let ref_ids: Vec<String> = records
            .iter()
            .flat_map(|r| {
                let mut ids = Vec::new();
                if let Some(ref id) = r.quoted_tweet_id {
                    ids.push(id.clone());
                }
                if let Some(ref id) = r.retweeted_tweet_id {
                    ids.push(id.clone());
                }
                ids
            })
            .collect();

        // Fetch referenced tweets in a single batch query
        let ref_map: std::collections::HashMap<String, TweetData> = if !ref_ids.is_empty() {
            let ref_record_refs: Vec<String> = ref_ids
                .iter()
                .map(|id| format!("tweet:⟨{}⟩", id))
                .collect();
            let ref_query = format!("SELECT * FROM [{}]", ref_record_refs.join(", "));
            let mut ref_result = self
                .db
                .query(&ref_query)
                .await
                .map_err(|e| Error::Storage(format!("Failed to get referenced tweets: {}", e)))?;
            let ref_records: Vec<TweetRecord> = ref_result
                .take(0)
                .map_err(|e| Error::Storage(format!("Failed to parse referenced tweets: {}", e)))?;
            ref_records
                .into_iter()
                .filter_map(|r| {
                    let id = r.tweet_id.clone();
                    r.try_into().ok().map(|t: TweetData| (id, t))
                })
                .collect()
        } else {
            std::collections::HashMap::new()
        };

        // Convert and hydrate each record
        let tweets: Vec<TweetData> = records
            .into_iter()
            .filter_map(|r| {
                let quoted_id = r.quoted_tweet_id.clone();
                let retweeted_id = r.retweeted_tweet_id.clone();
                r.try_into().ok().map(|mut t: TweetData| {
                    if let Some(ref qt_id) = quoted_id {
                        t.quoted_tweet = ref_map.get(qt_id).cloned().map(Box::new);
                    }
                    if let Some(ref rt_id) = retweeted_id {
                        t.retweeted_tweet = ref_map.get(rt_id).cloned().map(Box::new);
                    }
                    t
                })
            })
            .collect();

        Ok(tweets)
    }
}

#[async_trait]
impl TweetStore for SurrealDbStorage {
    async fn upsert_tweet(&self, tweet: &TweetData) -> Result<()> {
        // First, upsert any referenced tweets (quoted/retweeted) as separate records
        if let Some(ref quoted) = tweet.quoted_tweet {
            // Recursively upsert the quoted tweet (use Box::pin for async recursion)
            Box::pin(self.upsert_tweet(quoted)).await?;
        }
        if let Some(ref retweeted) = tweet.retweeted_tweet {
            // Recursively upsert the retweeted tweet
            Box::pin(self.upsert_tweet(retweeted)).await?;
        }

        // Now upsert the main tweet (with just IDs for references)
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
        let mut result = self
            .db
            .query(&query)
            .await
            .map_err(|e| Error::Storage(format!("Failed to get tweet: {}", e)))?;

        let records: Vec<TweetRecord> = result
            .take(0)
            .map_err(|e| Error::Storage(format!("Failed to parse tweet: {}", e)))?;

        if records.is_empty() {
            return Ok(None);
        }

        // Use helper to hydrate referenced tweets
        let mut tweets = self.hydrate_tweet_records(records).await?;
        Ok(tweets.pop())
    }

    async fn tweet_exists(&self, id: &str) -> Result<bool> {
        // Query using the record ID syntax
        let query = format!("SELECT tweet_id FROM tweet:⟨{}⟩", id);
        let mut result = self
            .db
            .query(&query)
            .await
            .map_err(|e| Error::Storage(format!("Failed to check tweet: {}", e)))?;

        let records: Vec<serde_json::Value> = result
            .take(0)
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

        let mut result = self
            .db
            .query(&query)
            .await
            .map_err(|e| Error::Storage(format!("Failed to filter ids: {}", e)))?;

        #[derive(Deserialize)]
        struct IdRecord {
            tweet_id: String,
        }

        let records: Vec<IdRecord> = result
            .take(0)
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
        let limit = limit.unwrap_or(100) as usize;
        let offset = offset.unwrap_or(0) as usize;
        let collection_owned = collection.to_string();
        let user_id_owned = user_id.to_string();

        // Step 1: Get tweet_ids from tweet_collection (fast, uses index)
        let mut id_result = self
            .db
            .query(
                "SELECT tweet_id, added_at FROM tweet_collection
                 WHERE collection = $collection AND user_id = $user_id
                 ORDER BY added_at DESC",
            )
            .bind(("collection", collection_owned))
            .bind(("user_id", user_id_owned))
            .await
            .map_err(|e| Error::Storage(format!("Failed to get tweet ids: {}", e)))?;

        #[derive(Deserialize)]
        struct TweetIdRecord {
            tweet_id: String,
        }

        let id_records: Vec<TweetIdRecord> = id_result
            .take(0)
            .map_err(|e| Error::Storage(format!("Failed to parse tweet ids: {}", e)))?;

        if id_records.is_empty() {
            return Ok(Vec::new());
        }

        // Apply offset and limit
        let tweet_ids: Vec<String> = id_records
            .into_iter()
            .skip(offset)
            .take(limit)
            .map(|r| r.tweet_id)
            .collect();

        if tweet_ids.is_empty() {
            return Ok(Vec::new());
        }

        // Step 2: Fetch tweets directly using record ID syntax (instant lookups)
        let record_refs: Vec<String> = tweet_ids
            .iter()
            .map(|id| format!("tweet:⟨{}⟩", id))
            .collect();
        let query = format!("SELECT * FROM [{}]", record_refs.join(", "));

        let mut tweet_result = self
            .db
            .query(&query)
            .await
            .map_err(|e| Error::Storage(format!("Failed to get tweets: {}", e)))?;

        let records: Vec<TweetRecord> = tweet_result
            .take(0)
            .map_err(|e| Error::Storage(format!("Failed to parse tweets: {}", e)))?;

        // Collect all referenced tweet IDs for batch hydration
        let ref_ids: Vec<String> = records
            .iter()
            .flat_map(|r| {
                let mut ids = Vec::new();
                if let Some(ref id) = r.quoted_tweet_id {
                    ids.push(id.clone());
                }
                if let Some(ref id) = r.retweeted_tweet_id {
                    ids.push(id.clone());
                }
                ids
            })
            .collect();

        // Fetch referenced tweets in a single batch query
        let ref_map: std::collections::HashMap<String, TweetData> = if !ref_ids.is_empty() {
            let ref_record_refs: Vec<String> = ref_ids
                .iter()
                .map(|id| format!("tweet:⟨{}⟩", id))
                .collect();
            let ref_query = format!("SELECT * FROM [{}]", ref_record_refs.join(", "));
            let mut ref_result = self
                .db
                .query(&ref_query)
                .await
                .map_err(|e| Error::Storage(format!("Failed to get referenced tweets: {}", e)))?;
            let ref_records: Vec<TweetRecord> = ref_result
                .take(0)
                .map_err(|e| Error::Storage(format!("Failed to parse referenced tweets: {}", e)))?;
            ref_records
                .into_iter()
                .filter_map(|r| {
                    let id = r.tweet_id.clone();
                    r.try_into().ok().map(|t: TweetData| (id, t))
                })
                .collect()
        } else {
            std::collections::HashMap::new()
        };

        // Build a map for ordering, with hydration
        let tweet_map: std::collections::HashMap<String, TweetData> = records
            .into_iter()
            .filter_map(|r| {
                let id = r.tweet_id.clone();
                let quoted_id = r.quoted_tweet_id.clone();
                let retweeted_id = r.retweeted_tweet_id.clone();
                r.try_into().ok().map(|mut t: TweetData| {
                    // Hydrate from the reference map
                    if let Some(ref qt_id) = quoted_id {
                        t.quoted_tweet = ref_map.get(qt_id).cloned().map(Box::new);
                    }
                    if let Some(ref rt_id) = retweeted_id {
                        t.retweeted_tweet = ref_map.get(rt_id).cloned().map(Box::new);
                    }
                    (id, t)
                })
            })
            .collect();

        // Return tweets in the order from tweet_collection (by added_at DESC)
        let ordered_tweets: Vec<TweetData> = tweet_ids
            .into_iter()
            .filter_map(|id| tweet_map.get(&id).cloned())
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
        let exists = self.is_in_collection(tweet_id, collection, user_id).await?;

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
                    }",
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

        let mut result = self
            .db
            .query(
                "SELECT tweet_id FROM tweet_collection
                 WHERE tweet_id = $tweet_id
                 AND collection = $collection
                 AND user_id = $user_id
                 LIMIT 1",
            )
            .bind(("tweet_id", tweet_id_owned))
            .bind(("collection", collection_owned))
            .bind(("user_id", user_id_owned))
            .await
            .map_err(|e| Error::Storage(format!("Failed to check collection: {}", e)))?;

        let records: Vec<serde_json::Value> = result
            .take(0)
            .map_err(|e| Error::Storage(format!("Failed to parse result: {}", e)))?;

        Ok(!records.is_empty())
    }

    async fn collection_count(&self, collection: &str, user_id: &str) -> Result<u64> {
        let collection_owned = collection.to_string();
        let user_id_owned = user_id.to_string();

        let mut result = self
            .db
            .query(
                "SELECT count() as count FROM tweet_collection
                 WHERE collection = $collection AND user_id = $user_id
                 GROUP ALL",
            )
            .bind(("collection", collection_owned))
            .bind(("user_id", user_id_owned))
            .await
            .map_err(|e| Error::Storage(format!("Failed to count collection: {}", e)))?;

        #[derive(Deserialize)]
        struct CountResult {
            count: u64,
        }

        let records: Vec<CountResult> = result
            .take(0)
            .map_err(|e| Error::Storage(format!("Failed to parse count: {}", e)))?;

        Ok(records.first().map(|r| r.count).unwrap_or(0))
    }

    async fn get_tweets_by_collection_time_range(
        &self,
        collection: &str,
        user_id: &str,
        _start_time: DateTime<Utc>,
        _end_time: DateTime<Utc>,
        limit: Option<u32>,
    ) -> Result<Vec<TweetData>> {
        // For now, just return the most recent tweets (time filtering TODO)
        self.get_tweets_by_collection(collection, user_id, limit, None)
            .await
    }

    async fn get_tweets_missing_headlines(
        &self,
        min_length: usize,
        limit: Option<u32>,
    ) -> Result<Vec<TweetData>> {
        let limit = limit.unwrap_or(100);
        let min_len = min_length as u32;

        // Query tweets where headline is None and text length exceeds threshold
        let mut result = self
            .db
            .query(
                "SELECT * FROM tweet WHERE headline IS NONE AND string::len(text) > $min_len LIMIT $limit",
            )
            .bind(("min_len", min_len))
            .bind(("limit", limit))
            .await
            .map_err(|e| Error::Storage(format!("Failed to get tweets missing headlines: {}", e)))?;

        let records: Vec<TweetRecord> = result
            .take(0)
            .map_err(|e| Error::Storage(format!("Failed to parse tweets: {}", e)))?;

        self.hydrate_tweet_records(records).await
    }

    async fn update_tweet_headlines(&self, headlines: &[(String, String)]) -> Result<usize> {
        let mut updated = 0;

        for (tweet_id, headline) in headlines {
            let query = format!(
                "UPDATE tweet:⟨{}⟩ SET headline = $headline, updated_at = $now",
                tweet_id
            );
            self.db
                .query(&query)
                .bind(("headline", headline.clone()))
                .bind(("now", Utc::now()))
                .await
                .map_err(|e| Error::Storage(format!("Failed to update headline: {}", e)))?;
            updated += 1;
        }

        Ok(updated)
    }

    async fn get_tweets_by_ids(&self, ids: &[&str]) -> Result<Vec<TweetData>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }

        // Build record ID references for SurrealDB (tweet:id format)
        let record_refs: Vec<String> = ids.iter().map(|id| format!("tweet:⟨{}⟩", id)).collect();
        let query = format!("SELECT * FROM [{}]", record_refs.join(", "));

        let mut result = self
            .db
            .query(&query)
            .await
            .map_err(|e| Error::Storage(format!("Failed to get tweets by ids: {}", e)))?;

        let records: Vec<TweetRecord> = result
            .take(0)
            .map_err(|e| Error::Storage(format!("Failed to parse tweets: {}", e)))?;

        // Use helper to hydrate referenced tweets
        self.hydrate_tweet_records(records).await
    }

    async fn get_collection_tweet_ids(
        &self,
        collection: &str,
        user_id: &str,
        limit: Option<u32>,
    ) -> Result<Vec<String>> {
        let collection_owned = collection.to_string();
        let user_id_owned = user_id.to_string();

        let query = if let Some(limit) = limit {
            format!(
                "SELECT tweet_id, added_at FROM tweet_collection WHERE collection = $collection AND user_id = $user_id ORDER BY added_at DESC LIMIT {}",
                limit
            )
        } else {
            "SELECT tweet_id, added_at FROM tweet_collection WHERE collection = $collection AND user_id = $user_id ORDER BY added_at DESC".to_string()
        };

        let mut result = self
            .db
            .query(&query)
            .bind(("collection", collection_owned))
            .bind(("user_id", user_id_owned))
            .await
            .map_err(|e| Error::Storage(format!("Failed to get collection tweet ids: {}", e)))?;

        #[derive(Deserialize)]
        struct TweetIdRecord {
            tweet_id: String,
        }

        let records: Vec<TweetIdRecord> = result
            .take(0)
            .map_err(|e| Error::Storage(format!("Failed to parse tweet ids: {}", e)))?;

        Ok(records.into_iter().map(|r| r.tweet_id).collect())
    }

    async fn get_user_reply_tweets(
        &self,
        user_id: &str,
        limit: Option<u32>,
    ) -> Result<Vec<(String, String)>> {
        // Get tweets from user_tweets collection that have in_reply_to_status_id
        let user_id_owned = user_id.to_string();
        let limit = limit.unwrap_or(1000);

        // First get tweet IDs from user_tweets collection
        let mut id_result = self
            .db
            .query(
                "SELECT tweet_id FROM tweet_collection WHERE collection = 'user_tweets' AND user_id = $user_id LIMIT $limit",
            )
            .bind(("user_id", user_id_owned.clone()))
            .bind(("limit", limit))
            .await
            .map_err(|e| Error::Storage(format!("Failed to get user tweet ids: {}", e)))?;

        #[derive(Deserialize)]
        struct TweetIdRecord {
            tweet_id: String,
        }

        let id_records: Vec<TweetIdRecord> = id_result
            .take(0)
            .map_err(|e| Error::Storage(format!("Failed to parse tweet ids: {}", e)))?;

        if id_records.is_empty() {
            return Ok(Vec::new());
        }

        // Fetch only the fields we need from these tweets
        let record_refs: Vec<String> = id_records
            .iter()
            .map(|r| format!("tweet:⟨{}⟩", r.tweet_id))
            .collect();
        let query = format!(
            "SELECT tweet_id, in_reply_to_status_id FROM [{}] WHERE in_reply_to_status_id IS NOT NONE",
            record_refs.join(", ")
        );

        let mut result = self
            .db
            .query(&query)
            .await
            .map_err(|e| Error::Storage(format!("Failed to get reply tweets: {}", e)))?;

        #[derive(Deserialize)]
        struct ReplyRecord {
            tweet_id: String,
            in_reply_to_status_id: String,
        }

        let records: Vec<ReplyRecord> = result
            .take(0)
            .map_err(|e| Error::Storage(format!("Failed to parse reply records: {}", e)))?;

        Ok(records
            .into_iter()
            .map(|r| (r.tweet_id, r.in_reply_to_status_id))
            .collect())
    }

    async fn get_user_quote_tweets(
        &self,
        user_id: &str,
        limit: Option<u32>,
    ) -> Result<Vec<(String, String)>> {
        // Get tweets from user_tweets collection that have quoted_tweet_id
        let user_id_owned = user_id.to_string();
        let limit = limit.unwrap_or(1000);

        // First get tweet IDs from user_tweets collection
        let mut id_result = self
            .db
            .query(
                "SELECT tweet_id FROM tweet_collection WHERE collection = 'user_tweets' AND user_id = $user_id LIMIT $limit",
            )
            .bind(("user_id", user_id_owned.clone()))
            .bind(("limit", limit))
            .await
            .map_err(|e| Error::Storage(format!("Failed to get user tweet ids: {}", e)))?;

        #[derive(Deserialize)]
        struct TweetIdRecord {
            tweet_id: String,
        }

        let id_records: Vec<TweetIdRecord> = id_result
            .take(0)
            .map_err(|e| Error::Storage(format!("Failed to parse tweet ids: {}", e)))?;

        if id_records.is_empty() {
            return Ok(Vec::new());
        }

        // Fetch only the fields we need from these tweets
        let record_refs: Vec<String> = id_records
            .iter()
            .map(|r| format!("tweet:⟨{}⟩", r.tweet_id))
            .collect();
        let query = format!(
            "SELECT tweet_id, quoted_tweet_id FROM [{}] WHERE quoted_tweet_id IS NOT NONE",
            record_refs.join(", ")
        );

        let mut result = self
            .db
            .query(&query)
            .await
            .map_err(|e| Error::Storage(format!("Failed to get quote tweets: {}", e)))?;

        #[derive(Deserialize)]
        struct QuoteRecord {
            tweet_id: String,
            quoted_tweet_id: String,
        }

        let records: Vec<QuoteRecord> = result
            .take(0)
            .map_err(|e| Error::Storage(format!("Failed to parse quote records: {}", e)))?;

        Ok(records
            .into_iter()
            .map(|r| (r.tweet_id, r.quoted_tweet_id))
            .collect())
    }

    async fn get_user_retweets(
        &self,
        user_id: &str,
        limit: Option<u32>,
    ) -> Result<Vec<(String, String)>> {
        // Get tweets from user_tweets collection that have retweeted_tweet_id
        let user_id_owned = user_id.to_string();
        let limit = limit.unwrap_or(1000);

        // First get tweet IDs from user_tweets collection
        let mut id_result = self
            .db
            .query(
                "SELECT tweet_id FROM tweet_collection WHERE collection = 'user_tweets' AND user_id = $user_id LIMIT $limit",
            )
            .bind(("user_id", user_id_owned.clone()))
            .bind(("limit", limit))
            .await
            .map_err(|e| Error::Storage(format!("Failed to get user tweet ids: {}", e)))?;

        #[derive(Deserialize)]
        struct TweetIdRecord {
            tweet_id: String,
        }

        let id_records: Vec<TweetIdRecord> = id_result
            .take(0)
            .map_err(|e| Error::Storage(format!("Failed to parse tweet ids: {}", e)))?;

        if id_records.is_empty() {
            return Ok(Vec::new());
        }

        // Fetch only the fields we need from these tweets
        let record_refs: Vec<String> = id_records
            .iter()
            .map(|r| format!("tweet:⟨{}⟩", r.tweet_id))
            .collect();
        let query = format!(
            "SELECT tweet_id, retweeted_tweet_id FROM [{}] WHERE retweeted_tweet_id IS NOT NONE",
            record_refs.join(", ")
        );

        let mut result = self
            .db
            .query(&query)
            .await
            .map_err(|e| Error::Storage(format!("Failed to get retweets: {}", e)))?;

        #[derive(Deserialize)]
        struct RetweetRecord {
            tweet_id: String,
            retweeted_tweet_id: String,
        }

        let records: Vec<RetweetRecord> = result
            .take(0)
            .map_err(|e| Error::Storage(format!("Failed to parse retweet records: {}", e)))?;

        Ok(records
            .into_iter()
            .map(|r| (r.tweet_id, r.retweeted_tweet_id))
            .collect())
    }
}

#[async_trait]
impl SyncStateStore for SurrealDbStorage {
    async fn get_sync_state(&self, collection: &str, user_id: &str) -> Result<Option<SyncState>> {
        let collection_owned = collection.to_string();
        let user_id_owned = user_id.to_string();

        let mut result = self
            .db
            .query(
                "SELECT * FROM sync_state
                 WHERE collection = $collection AND user_id = $user_id
                 LIMIT 1",
            )
            .bind(("collection", collection_owned))
            .bind(("user_id", user_id_owned))
            .await
            .map_err(|e| Error::Storage(format!("Failed to get sync state: {}", e)))?;

        let records: Vec<SyncStateRecord> = result
            .take(0)
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
                 WHERE collection = $collection AND user_id = $user_id",
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
                 WHERE collection = $collection AND user_id = $user_id",
            )
            .bind(("collection", collection_owned))
            .bind(("user_id", user_id_owned))
            .await
            .map_err(|e| Error::Storage(format!("Failed to clear sync state: {}", e)))?;

        Ok(())
    }

    async fn get_all_sync_states(&self, user_id: &str) -> Result<Vec<SyncState>> {
        let user_id_owned = user_id.to_string();

        let mut result = self
            .db
            .query("SELECT * FROM sync_state WHERE user_id = $user_id")
            .bind(("user_id", user_id_owned))
            .await
            .map_err(|e| Error::Storage(format!("Failed to get sync states: {}", e)))?;

        let records: Vec<SyncStateRecord> = result
            .take(0)
            .map_err(|e| Error::Storage(format!("Failed to parse sync states: {}", e)))?;

        Ok(records.into_iter().map(|r| r.into()).collect())
    }

    async fn get_any_synced_user_id(&self) -> Result<Option<String>> {
        let mut result = self
            .db
            .query("SELECT user_id FROM sync_state LIMIT 1")
            .await
            .map_err(|e| Error::Storage(format!("Failed to get user_id: {}", e)))?;

        #[derive(Deserialize)]
        struct UserIdRecord {
            user_id: String,
        }

        let records: Vec<UserIdRecord> = result
            .take(0)
            .map_err(|e| Error::Storage(format!("Failed to parse user_id: {}", e)))?;

        Ok(records.into_iter().next().map(|r| r.user_id))
    }
}

/// Twitter user record for storage.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct TwitterUserRecord {
    user_id: String,
    username: String,
    username_lower: String,
    name: Option<String>,
    updated_at: DateTime<Utc>,
}

#[async_trait]
impl UserStore for SurrealDbStorage {
    async fn upsert_user_from_mention(&self, user: &MentionedUser) -> Result<()> {
        let record = TwitterUserRecord {
            user_id: user.id.clone(),
            username: user.username.clone(),
            username_lower: user.username.to_lowercase(),
            name: user.name.clone(),
            updated_at: Utc::now(),
        };

        let json_content = serde_json::to_value(&record)
            .map_err(|e| Error::Storage(format!("Failed to serialize user: {}", e)))?;

        // Use user_id as the record ID
        let query = format!("UPSERT twitter_user:⟨{}⟩ CONTENT $content", user.id);
        self.db
            .query(&query)
            .bind(("content", json_content))
            .await
            .map_err(|e| Error::Storage(format!("Failed to upsert user: {}", e)))?;

        Ok(())
    }

    async fn get_user_by_username(&self, username: &str) -> Result<Option<MentionedUser>> {
        let username_lower = username.to_lowercase();

        let mut result = self
            .db
            .query("SELECT * FROM twitter_user WHERE username_lower = $username LIMIT 1")
            .bind(("username", username_lower))
            .await
            .map_err(|e| Error::Storage(format!("Failed to get user: {}", e)))?;

        let records: Vec<TwitterUserRecord> = result
            .take(0)
            .map_err(|e| Error::Storage(format!("Failed to parse user: {}", e)))?;

        Ok(records.into_iter().next().map(|r| MentionedUser {
            id: r.user_id,
            username: r.username,
            name: r.name,
        }))
    }

    async fn get_user_by_id(&self, id: &str) -> Result<Option<MentionedUser>> {
        let query = format!("SELECT * FROM twitter_user:⟨{}⟩", id);
        let mut result = self
            .db
            .query(&query)
            .await
            .map_err(|e| Error::Storage(format!("Failed to get user: {}", e)))?;

        let records: Vec<TwitterUserRecord> = result
            .take(0)
            .map_err(|e| Error::Storage(format!("Failed to parse user: {}", e)))?;

        Ok(records.into_iter().next().map(|r| MentionedUser {
            id: r.user_id,
            username: r.username,
            name: r.name,
        }))
    }

    async fn get_tweets_mentioning_user(
        &self,
        user_id: &str,
        limit: Option<u32>,
    ) -> Result<Vec<TweetData>> {
        let limit = limit.unwrap_or(20);
        let user_id_owned = user_id.to_string();

        // Query tweets where mentions array contains a user with this ID
        let mut result = self
            .db
            .query(
                "SELECT * FROM tweet WHERE mentions[*].id CONTAINS $user_id
                 ORDER BY created_at DESC LIMIT $limit",
            )
            .bind(("user_id", user_id_owned))
            .bind(("limit", limit))
            .await
            .map_err(|e| Error::Storage(format!("Failed to get tweets: {}", e)))?;

        let records: Vec<TweetRecord> = result
            .take(0)
            .map_err(|e| Error::Storage(format!("Failed to parse tweets: {}", e)))?;

        self.hydrate_tweet_records(records).await
    }

    async fn get_tweets_replying_to_user(
        &self,
        user_id: &str,
        limit: Option<u32>,
    ) -> Result<Vec<TweetData>> {
        let limit = limit.unwrap_or(20);
        let user_id_owned = user_id.to_string();

        let mut result = self
            .db
            .query(
                "SELECT * FROM tweet WHERE in_reply_to_user_id = $user_id
                 ORDER BY created_at DESC LIMIT $limit",
            )
            .bind(("user_id", user_id_owned))
            .bind(("limit", limit))
            .await
            .map_err(|e| Error::Storage(format!("Failed to get tweets: {}", e)))?;

        let records: Vec<TweetRecord> = result
            .take(0)
            .map_err(|e| Error::Storage(format!("Failed to parse tweets: {}", e)))?;

        self.hydrate_tweet_records(records).await
    }
}

/// Resonance score record for SurrealDB storage.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ResonanceScoreRecord {
    tweet_id: String,
    user_id: String,
    total: f64,
    liked: bool,
    bookmarked: bool,
    reply_count: u32,
    quote_count: u32,
    #[serde(default)]
    retweet_count: u32,
    computed_at: DateTime<Utc>,
}

impl From<&ResonanceScore> for ResonanceScoreRecord {
    fn from(score: &ResonanceScore) -> Self {
        Self {
            tweet_id: score.tweet_id.clone(),
            user_id: score.user_id.clone(),
            total: score.total,
            liked: score.liked,
            bookmarked: score.bookmarked,
            reply_count: score.reply_count,
            quote_count: score.quote_count,
            retweet_count: score.retweet_count,
            computed_at: score.computed_at,
        }
    }
}

impl From<ResonanceScoreRecord> for ResonanceScore {
    fn from(record: ResonanceScoreRecord) -> Self {
        Self {
            tweet_id: record.tweet_id,
            user_id: record.user_id,
            total: record.total,
            liked: record.liked,
            bookmarked: record.bookmarked,
            reply_count: record.reply_count,
            quote_count: record.quote_count,
            retweet_count: record.retweet_count,
            computed_at: record.computed_at,
        }
    }
}

#[async_trait]
impl ResonanceStore for SurrealDbStorage {
    async fn get_resonance_score(
        &self,
        tweet_id: &str,
        user_id: &str,
    ) -> Result<Option<ResonanceScore>> {
        let tweet_id_owned = tweet_id.to_string();
        let user_id_owned = user_id.to_string();

        let mut result = self
            .db
            .query(
                "SELECT * FROM resonance_score
                 WHERE tweet_id = $tweet_id AND user_id = $user_id
                 LIMIT 1",
            )
            .bind(("tweet_id", tweet_id_owned))
            .bind(("user_id", user_id_owned))
            .await
            .map_err(|e| Error::Storage(format!("Failed to get resonance score: {}", e)))?;

        let records: Vec<ResonanceScoreRecord> = result
            .take(0)
            .map_err(|e| Error::Storage(format!("Failed to parse resonance score: {}", e)))?;

        Ok(records.into_iter().next().map(|r| r.into()))
    }

    async fn get_top_resonance_scores(
        &self,
        user_id: &str,
        limit: u32,
        offset: Option<u32>,
    ) -> Result<Vec<ResonanceScore>> {
        let user_id_owned = user_id.to_string();
        let offset = offset.unwrap_or(0);

        let mut result = self
            .db
            .query(
                "SELECT * FROM resonance_score
                 WHERE user_id = $user_id
                 ORDER BY total DESC
                 LIMIT $limit START $offset",
            )
            .bind(("user_id", user_id_owned))
            .bind(("limit", limit))
            .bind(("offset", offset))
            .await
            .map_err(|e| Error::Storage(format!("Failed to get top resonance scores: {}", e)))?;

        let records: Vec<ResonanceScoreRecord> = result
            .take(0)
            .map_err(|e| Error::Storage(format!("Failed to parse resonance scores: {}", e)))?;

        Ok(records.into_iter().map(|r| r.into()).collect())
    }

    async fn upsert_resonance_score(&self, score: &ResonanceScore) -> Result<()> {
        let record = ResonanceScoreRecord::from(score);

        let json_content = serde_json::to_value(&record)
            .map_err(|e| Error::Storage(format!("Failed to serialize resonance score: {}", e)))?;

        // Check if exists
        let exists = self
            .get_resonance_score(&score.tweet_id, &score.user_id)
            .await?
            .is_some();

        if exists {
            // Update existing record
            self.db
                .query(
                    "UPDATE resonance_score CONTENT $content
                     WHERE tweet_id = $tweet_id AND user_id = $user_id",
                )
                .bind(("content", json_content))
                .bind(("tweet_id", score.tweet_id.clone()))
                .bind(("user_id", score.user_id.clone()))
                .await
                .map_err(|e| Error::Storage(format!("Failed to update resonance score: {}", e)))?;
        } else {
            // Insert new record
            self.db
                .query("CREATE resonance_score CONTENT $content")
                .bind(("content", json_content))
                .await
                .map_err(|e| Error::Storage(format!("Failed to create resonance score: {}", e)))?;
        }

        Ok(())
    }

    async fn upsert_resonance_scores(&self, scores: &[ResonanceScore]) -> Result<usize> {
        let mut count = 0;
        for score in scores {
            self.upsert_resonance_score(score).await?;
            count += 1;
        }
        Ok(count)
    }

    async fn clear_resonance_scores(&self, user_id: &str) -> Result<u64> {
        let user_id_owned = user_id.to_string();

        // Get count before deleting
        let count = self.resonance_score_count(user_id).await?;

        self.db
            .query("DELETE FROM resonance_score WHERE user_id = $user_id")
            .bind(("user_id", user_id_owned))
            .await
            .map_err(|e| Error::Storage(format!("Failed to clear resonance scores: {}", e)))?;

        Ok(count)
    }

    async fn resonance_score_count(&self, user_id: &str) -> Result<u64> {
        let user_id_owned = user_id.to_string();

        let mut result = self
            .db
            .query(
                "SELECT count() as count FROM resonance_score
                 WHERE user_id = $user_id
                 GROUP ALL",
            )
            .bind(("user_id", user_id_owned))
            .await
            .map_err(|e| Error::Storage(format!("Failed to count resonance scores: {}", e)))?;

        #[derive(Deserialize)]
        struct CountResult {
            count: u64,
        }

        let records: Vec<CountResult> = result
            .take(0)
            .map_err(|e| Error::Storage(format!("Failed to parse count: {}", e)))?;

        Ok(records.first().map(|r| r.count).unwrap_or(0))
    }
}
