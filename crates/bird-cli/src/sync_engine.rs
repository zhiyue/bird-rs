//! Sync engine for synchronizing tweets to local storage.
//!
//! Supports bidirectional sync:
//! - **Forward sync**: Fetch new items since last sync (stop at newest_item_id)
//! - **Backfill sync**: Continue fetching older items (resume from backfill_cursor)

use bird_client::{Collection, PaginatedResult, PaginationOptions, RateLimitConfig, TwitterClient};
use bird_storage::{SurrealDbStorage, SyncStateStore, TweetStore};

/// Result of a sync operation.
pub struct SyncResult {
    /// Number of new tweets stored.
    pub new_tweets: usize,
    /// Total tweets fetched from API.
    pub total_fetched: usize,
    /// Whether sync stopped at a known tweet (forward sync hit newest_item_id).
    pub stopped_at_known: bool,
    /// Whether there's more history to backfill.
    pub has_more_history: bool,
    /// The sync direction that was performed.
    pub direction: SyncDirection,
}

/// Direction of sync operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncDirection {
    /// Forward sync: fetching new items.
    Forward,
    /// Backfill sync: fetching older items.
    Backfill,
    /// Full sync: complete re-sync from scratch.
    Full,
}

impl std::fmt::Display for SyncDirection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SyncDirection::Forward => write!(f, "forward"),
            SyncDirection::Backfill => write!(f, "backfill"),
            SyncDirection::Full => write!(f, "full"),
        }
    }
}

/// Options for sync operation.
pub struct SyncOptions {
    /// Full re-sync (ignore previous sync state).
    pub full: bool,
    /// Maximum number of pages to fetch.
    pub max_pages: Option<u32>,
    /// Skip backfill, only do forward sync.
    pub no_backfill: bool,
    /// Rate limit configuration.
    pub rate_limit: RateLimitConfig,
}

impl Default for SyncOptions {
    fn default() -> Self {
        Self {
            full: false,
            max_pages: Some(10), // Conservative default
            no_backfill: false,
            rate_limit: RateLimitConfig::default(),
        }
    }
}

/// Engine for syncing tweets to storage.
pub struct SyncEngine {
    client: TwitterClient,
    storage: SurrealDbStorage,
}

impl SyncEngine {
    /// Create a new sync engine.
    pub fn new(client: TwitterClient, storage: SurrealDbStorage) -> Self {
        Self { client, storage }
    }

    /// Sync a collection to storage.
    ///
    /// Bidirectional sync strategy:
    /// 1. **Forward sync** (default on incremental): Fetch new items, stop at newest_item_id
    /// 2. **Backfill sync** (if has_more_history): Resume from backfill_cursor, fetch older items
    /// 3. **Full sync** (--full flag): Ignore state, fetch everything fresh
    pub async fn sync_collection(
        &self,
        collection: Collection,
        user_id: &str,
        options: &SyncOptions,
    ) -> anyhow::Result<SyncResult> {
        // Get existing sync state (unless doing full sync)
        let sync_state = if options.full {
            None
        } else {
            self.storage
                .get_sync_state(collection.as_str(), user_id)
                .await?
        };

        // Determine sync direction
        let direction = if options.full {
            SyncDirection::Full
        } else if let Some(ref state) = sync_state {
            if state.is_first_sync() {
                // First sync is like a full sync
                SyncDirection::Full
            } else if options.no_backfill || !state.has_more_history {
                // Only forward sync (new items)
                SyncDirection::Forward
            } else {
                // We have state and more history to backfill
                // Default: do forward sync first, then backfill
                // For now, let's do forward sync
                SyncDirection::Forward
            }
        } else {
            // No state = first sync
            SyncDirection::Full
        };

        match direction {
            SyncDirection::Full => {
                self.do_full_sync(collection, user_id, options).await
            }
            SyncDirection::Forward => {
                self.do_forward_sync(collection, user_id, options, sync_state.unwrap())
                    .await
            }
            SyncDirection::Backfill => {
                self.do_backfill_sync(collection, user_id, options, sync_state.unwrap())
                    .await
            }
        }
    }

    /// Perform a full sync (first sync or --full flag).
    async fn do_full_sync(
        &self,
        collection: Collection,
        user_id: &str,
        options: &SyncOptions,
    ) -> anyhow::Result<SyncResult> {
        let result = self
            .fetch_collection(collection, user_id, None, options)
            .await?;

        let total_fetched = result.items.len();

        if result.items.is_empty() {
            return Ok(SyncResult {
                new_tweets: 0,
                total_fetched: 0,
                stopped_at_known: false,
                has_more_history: false,
                direction: SyncDirection::Full,
            });
        }

        // Get newest and oldest IDs
        let newest_id = result.items.first().map(|t| t.id.clone());
        let oldest_id = result.items.last().map(|t| t.id.clone());

        // Store tweets
        let new_count = self.storage.upsert_tweets(&result.items).await?;

        // Add to collection
        for tweet in &result.items {
            self.storage
                .add_to_collection(&tweet.id, collection.as_str(), user_id)
                .await?;
        }

        // Create new sync state
        let mut state = bird_client::SyncState::new(collection.as_str(), user_id);
        state.newest_item_id = newest_id;
        state.oldest_item_id = oldest_id;
        state.backfill_cursor = result.next_cursor;
        state.has_more_history = result.has_more;
        state.total_synced = total_fetched as u64;
        self.storage.update_sync_state(&state).await?;

        Ok(SyncResult {
            new_tweets: new_count,
            total_fetched,
            stopped_at_known: false,
            has_more_history: result.has_more,
            direction: SyncDirection::Full,
        })
    }

    /// Perform forward sync (catch up on new items).
    async fn do_forward_sync(
        &self,
        collection: Collection,
        user_id: &str,
        options: &SyncOptions,
        mut sync_state: bird_client::SyncState,
    ) -> anyhow::Result<SyncResult> {
        // Build pagination options - stop at newest known item
        let mut pagination = PaginationOptions::new();
        if let Some(max) = options.max_pages {
            pagination = pagination.with_max_pages(max);
        }
        if let Some(ref newest_id) = sync_state.newest_item_id {
            pagination = pagination.with_stop_at_id(newest_id.clone());
        }

        let result = self
            .fetch_collection_with_pagination(collection, user_id, &pagination, options)
            .await?;

        let total_fetched = result.items.len();

        if result.items.is_empty() {
            return Ok(SyncResult {
                new_tweets: 0,
                total_fetched: 0,
                stopped_at_known: result.stopped_at_known,
                has_more_history: sync_state.has_more_history,
                direction: SyncDirection::Forward,
            });
        }

        // Get newest ID from fetched items
        let newest_id = result.items.first().map(|t| t.id.clone());

        // Store tweets
        let new_count = self.storage.upsert_tweets(&result.items).await?;

        // Add to collection
        for tweet in &result.items {
            self.storage
                .add_to_collection(&tweet.id, collection.as_str(), user_id)
                .await?;
        }

        // Update sync state for forward sync
        sync_state.update_forward(newest_id, total_fetched as u64);
        self.storage.update_sync_state(&sync_state).await?;

        Ok(SyncResult {
            new_tweets: new_count,
            total_fetched,
            stopped_at_known: result.stopped_at_known,
            has_more_history: sync_state.has_more_history,
            direction: SyncDirection::Forward,
        })
    }

    /// Perform backfill sync (fetch older items).
    async fn do_backfill_sync(
        &self,
        collection: Collection,
        user_id: &str,
        options: &SyncOptions,
        mut sync_state: bird_client::SyncState,
    ) -> anyhow::Result<SyncResult> {
        // Resume from backfill cursor
        let cursor = sync_state.backfill_cursor.clone();

        let result = self
            .fetch_collection(collection, user_id, cursor, options)
            .await?;

        let total_fetched = result.items.len();

        if result.items.is_empty() {
            // No more items to backfill
            sync_state.has_more_history = false;
            sync_state.backfill_cursor = None;
            self.storage.update_sync_state(&sync_state).await?;

            return Ok(SyncResult {
                new_tweets: 0,
                total_fetched: 0,
                stopped_at_known: false,
                has_more_history: false,
                direction: SyncDirection::Backfill,
            });
        }

        // Get oldest ID from fetched items
        let oldest_id = result.items.last().map(|t| t.id.clone());

        // Store tweets
        let new_count = self.storage.upsert_tweets(&result.items).await?;

        // Add to collection
        for tweet in &result.items {
            self.storage
                .add_to_collection(&tweet.id, collection.as_str(), user_id)
                .await?;
        }

        // Update sync state for backfill
        sync_state.update_backfill(
            oldest_id,
            result.next_cursor,
            result.has_more,
            total_fetched as u64,
        );
        self.storage.update_sync_state(&sync_state).await?;

        Ok(SyncResult {
            new_tweets: new_count,
            total_fetched,
            stopped_at_known: false,
            has_more_history: result.has_more,
            direction: SyncDirection::Backfill,
        })
    }

    /// Perform backfill sync explicitly (for `bird sync backfill likes`).
    pub async fn backfill_collection(
        &self,
        collection: Collection,
        user_id: &str,
        options: &SyncOptions,
    ) -> anyhow::Result<SyncResult> {
        let sync_state = self
            .storage
            .get_sync_state(collection.as_str(), user_id)
            .await?;

        match sync_state {
            Some(state) if state.has_more_history => {
                self.do_backfill_sync(collection, user_id, options, state)
                    .await
            }
            Some(_) => Ok(SyncResult {
                new_tweets: 0,
                total_fetched: 0,
                stopped_at_known: false,
                has_more_history: false,
                direction: SyncDirection::Backfill,
            }),
            None => {
                // No sync state, need to do initial sync first
                Err(anyhow::anyhow!(
                    "No sync state found. Run `bird sync {}` first.",
                    collection.as_str()
                ))
            }
        }
    }

    /// Fetch collection items with given cursor.
    async fn fetch_collection(
        &self,
        collection: Collection,
        user_id: &str,
        cursor: Option<String>,
        options: &SyncOptions,
    ) -> anyhow::Result<PaginatedResult<bird_client::TweetData>> {
        let mut pagination = PaginationOptions::new();
        if let Some(max) = options.max_pages {
            pagination = pagination.with_max_pages(max);
        } else {
            pagination = pagination.fetch_all();
        }
        if let Some(c) = cursor {
            pagination = pagination.with_cursor(c);
        }

        self.fetch_collection_with_pagination(collection, user_id, &pagination, options)
            .await
    }

    /// Fetch collection items with custom pagination.
    async fn fetch_collection_with_pagination(
        &self,
        collection: Collection,
        user_id: &str,
        pagination: &PaginationOptions,
        options: &SyncOptions,
    ) -> anyhow::Result<PaginatedResult<bird_client::TweetData>> {
        match collection {
            Collection::Likes => self
                .client
                .get_all_likes_with_rate_limit(user_id, pagination.max_pages, &options.rate_limit)
                .await
                .map_err(|e| anyhow::anyhow!("{}", e)),
            Collection::Bookmarks => self
                .client
                .get_all_bookmarks_with_rate_limit(pagination.max_pages, &options.rate_limit)
                .await
                .map_err(|e| anyhow::anyhow!("{}", e)),
            Collection::UserTweets => self
                .client
                .get_all_user_tweets_with_rate_limit(user_id, pagination.max_pages, &options.rate_limit)
                .await
                .map_err(|e| anyhow::anyhow!("{}", e)),
            Collection::Timeline => {
                Err(anyhow::anyhow!("{} sync not yet implemented", collection.as_str()))
            }
        }
    }
}
