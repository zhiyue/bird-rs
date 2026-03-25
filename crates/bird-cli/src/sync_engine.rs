//! Sync engine for synchronizing tweets to local storage.
//!
//! Supports bidirectional sync:
//! - **Forward sync**: Fetch new items since last sync (stop at newest_item_id)
//! - **Backfill sync**: Continue fetching older items (resume from backfill_cursor)

use crate::storage_monitor::StorageMonitor;
use bird_client::{Collection, PaginatedResult, PaginationOptions, RateLimitConfig, TwitterClient};
use bird_storage::Storage;
use std::sync::Arc;

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
    /// Whether sync was stopped due to storage limit.
    pub stopped_at_storage_limit: bool,
    /// Final storage size in bytes (if available).
    pub final_storage_bytes: Option<u64>,
}

impl SyncResult {
    fn empty(direction: SyncDirection) -> Self {
        Self {
            new_tweets: 0,
            total_fetched: 0,
            stopped_at_known: false,
            has_more_history: false,
            direction,
            stopped_at_storage_limit: false,
            final_storage_bytes: None,
        }
    }
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

/// Callback for sync progress updates.
pub type ProgressCallback = Box<dyn Fn(&SyncProgress) + Send + Sync>;

/// Progress information during sync.
#[derive(Debug, Clone)]
pub struct SyncProgress {
    /// Tweets fetched so far.
    pub tweets_fetched: usize,
    /// New tweets stored so far.
    pub new_tweets: usize,
    /// Current storage size in bytes (if available).
    pub storage_bytes: Option<u64>,
    /// Storage size formatted (if available).
    pub storage_formatted: Option<String>,
    /// Max storage bytes (if limit set).
    pub max_storage_bytes: Option<u64>,
}

/// Auto-export configuration for sync.
#[derive(Clone)]
pub enum AutoExportConfig {
    /// Export all tweets to a single JSONL file.
    SingleFile(std::path::PathBuf),
    /// Export tweets grouped by day/month into separate JSONL files.
    Grouped {
        base_dir: std::path::PathBuf,
        group_by: AutoExportGroupBy,
    },
}

/// Grouping mode for auto-export during sync.
#[derive(Clone, Copy)]
pub enum AutoExportGroupBy {
    Day,
    Month,
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
    /// Storage monitor for size checking and circuit breaker.
    pub storage_monitor: Option<StorageMonitor>,
    /// Progress callback for real-time updates.
    pub on_progress: Option<ProgressCallback>,
    /// Auto-export configuration.
    pub auto_export: Option<AutoExportConfig>,
}

impl Default for SyncOptions {
    fn default() -> Self {
        Self {
            full: false,
            max_pages: Some(10), // Pages per batch; auto-backfill handles fetching all
            no_backfill: false,
            rate_limit: RateLimitConfig::default(),
            storage_monitor: None,
            on_progress: None,
            auto_export: None,
        }
    }
}

/// Engine for syncing tweets to storage.
pub struct SyncEngine {
    client: TwitterClient,
    storage: Arc<dyn Storage>,
}

impl SyncEngine {
    /// Create a new sync engine.
    pub fn new(client: TwitterClient, storage: Arc<dyn Storage>) -> Self {
        Self { client, storage }
    }

    /// Check storage limit and return error if exceeded.
    fn check_storage_limit(&self, options: &SyncOptions) -> Result<(), anyhow::Error> {
        if let Some(ref monitor) = options.storage_monitor {
            monitor
                .check_limit()
                .map_err(|e| anyhow::anyhow!("{}", e))?;
        }
        Ok(())
    }

    /// Report progress via callback if set.
    fn report_progress(&self, options: &SyncOptions, tweets_fetched: usize, new_tweets: usize) {
        if let Some(ref callback) = options.on_progress {
            let (storage_bytes, storage_formatted) =
                if let Some(ref monitor) = options.storage_monitor {
                    (monitor.current_size(), monitor.current_size_formatted())
                } else {
                    (None, None)
                };

            let progress = SyncProgress {
                tweets_fetched,
                new_tweets,
                storage_bytes,
                storage_formatted,
                max_storage_bytes: options.storage_monitor.as_ref().and_then(|m| m.max_bytes()),
            };
            callback(&progress);
        }
    }

    /// Append tweets to JSONL file(s) if auto-export is enabled.
    fn auto_export_tweets(
        &self,
        options: &SyncOptions,
        tweets: &[bird_client::TweetData],
    ) {
        let config = match &options.auto_export {
            Some(c) => c,
            None => return,
        };

        match config {
            AutoExportConfig::SingleFile(path) => {
                if let Some(parent) = path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                if let Ok(mut file) = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)
                {
                    use std::io::Write;
                    for tweet in tweets {
                        if let Ok(line) = serde_json::to_string(tweet) {
                            let _ = writeln!(file, "{}", line);
                        }
                    }
                }
            }
            AutoExportConfig::Grouped { base_dir, group_by } => {
                use std::io::Write;
                let _ = std::fs::create_dir_all(base_dir);
                for tweet in tweets {
                    let key = extract_group_key_for_sync(&tweet.created_at, *group_by);
                    let file_path = base_dir.join(format!("{}.jsonl", key));
                    if let Ok(mut file) = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(&file_path)
                    {
                        if let Ok(line) = serde_json::to_string(tweet) {
                            let _ = writeln!(file, "{}", line);
                        }
                    }
                }
            }
        }
    }

    /// Get current storage size from monitor.
    fn current_storage_size(&self, options: &SyncOptions) -> Option<u64> {
        options
            .storage_monitor
            .as_ref()
            .and_then(|m| m.current_size())
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
        // Check storage limit before starting
        self.check_storage_limit(options)?;

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

        let mut result = match direction {
            SyncDirection::Full => self.do_full_sync(collection, user_id, options).await?,
            SyncDirection::Forward => {
                self.do_forward_sync(collection, user_id, options, sync_state.unwrap())
                    .await?
            }
            SyncDirection::Backfill => {
                self.do_backfill_sync(collection, user_id, options, sync_state.unwrap())
                    .await?
            }
        };

        // Auto-backfill: keep fetching older items in batches until all history is synced.
        // Each batch is limited to BACKFILL_BATCH_PAGES pages so progress is reported between batches.
        const BACKFILL_BATCH_PAGES: u32 = 10;
        if !options.no_backfill && result.has_more_history && !result.stopped_at_storage_limit {
            // Use batched options so each round fetches a limited number of pages
            let batch_options = SyncOptions {
                max_pages: Some(BACKFILL_BATCH_PAGES),
                full: options.full,
                no_backfill: options.no_backfill,
                rate_limit: options.rate_limit.clone(),
                storage_monitor: options.storage_monitor.clone(),
                on_progress: None, // We'll report progress ourselves below
                auto_export: options.auto_export.clone(),
            };

            loop {
                let state = self
                    .storage
                    .get_sync_state(collection.as_str(), user_id)
                    .await?;

                match state {
                    Some(s) if s.has_more_history => {
                        let backfill_result = self
                            .do_backfill_sync(collection, user_id, &batch_options, s)
                            .await?;

                        result.new_tweets += backfill_result.new_tweets;
                        result.total_fetched += backfill_result.total_fetched;
                        result.has_more_history = backfill_result.has_more_history;
                        result.stopped_at_storage_limit = backfill_result.stopped_at_storage_limit;
                        result.final_storage_bytes = backfill_result.final_storage_bytes;

                        // Report cumulative progress after each batch
                        self.report_progress(options, result.total_fetched, result.new_tweets);

                        if !backfill_result.has_more_history
                            || backfill_result.stopped_at_storage_limit
                        {
                            break;
                        }
                    }
                    _ => break,
                }
            }
        }

        Ok(result)
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
            return Ok(SyncResult::empty(SyncDirection::Full));
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

        // Auto-export to JSONL if enabled
        self.auto_export_tweets(options, &result.items);

        // Report progress after storing
        self.report_progress(options, total_fetched, new_count);

        // Check storage limit after storing
        let stopped_at_storage_limit = self.check_storage_limit(options).is_err();

        // Create new sync state
        let mut state = bird_client::SyncState::new(collection.as_str(), user_id);
        state.newest_item_id = newest_id;
        state.oldest_item_id = oldest_id;
        state.backfill_cursor = result.next_cursor;
        state.has_more_history = result.has_more && !stopped_at_storage_limit;
        state.total_synced = total_fetched as u64;
        apply_rate_limit_info(&mut state, &options.rate_limit);
        self.storage.update_sync_state(&state).await?;

        Ok(SyncResult {
            new_tweets: new_count,
            total_fetched,
            stopped_at_known: false,
            has_more_history: result.has_more && !stopped_at_storage_limit,
            direction: SyncDirection::Full,
            stopped_at_storage_limit,
            final_storage_bytes: self.current_storage_size(options),
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
                stopped_at_known: result.stopped_at_known,
                has_more_history: sync_state.has_more_history,
                direction: SyncDirection::Forward,
                ..SyncResult::empty(SyncDirection::Forward)
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

        // Auto-export to JSONL if enabled
        self.auto_export_tweets(options, &result.items);

        // Report progress after storing
        self.report_progress(options, total_fetched, new_count);

        // Check storage limit after storing
        let stopped_at_storage_limit = self.check_storage_limit(options).is_err();

        // Update sync state for forward sync
        sync_state.update_forward(newest_id, total_fetched as u64);
        if stopped_at_storage_limit {
            sync_state.has_more_history = false;
        }
        apply_rate_limit_info(&mut sync_state, &options.rate_limit);
        self.storage.update_sync_state(&sync_state).await?;

        Ok(SyncResult {
            new_tweets: new_count,
            total_fetched,
            stopped_at_known: result.stopped_at_known,
            has_more_history: sync_state.has_more_history && !stopped_at_storage_limit,
            direction: SyncDirection::Forward,
            stopped_at_storage_limit,
            final_storage_bytes: self.current_storage_size(options),
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

            return Ok(SyncResult::empty(SyncDirection::Backfill));
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

        // Auto-export to JSONL if enabled
        self.auto_export_tweets(options, &result.items);

        // Report progress after storing
        self.report_progress(options, total_fetched, new_count);

        // Check storage limit after storing
        let stopped_at_storage_limit = self.check_storage_limit(options).is_err();

        // Update sync state for backfill
        sync_state.update_backfill(
            oldest_id,
            result.next_cursor,
            result.has_more && !stopped_at_storage_limit,
            total_fetched as u64,
        );
        apply_rate_limit_info(&mut sync_state, &options.rate_limit);
        self.storage.update_sync_state(&sync_state).await?;

        Ok(SyncResult {
            new_tweets: new_count,
            total_fetched,
            stopped_at_known: false,
            has_more_history: result.has_more && !stopped_at_storage_limit,
            direction: SyncDirection::Backfill,
            stopped_at_storage_limit,
            final_storage_bytes: self.current_storage_size(options),
        })
    }

    /// Perform backfill sync explicitly (for `bird sync backfill likes`).
    pub async fn backfill_collection(
        &self,
        collection: Collection,
        user_id: &str,
        options: &SyncOptions,
    ) -> anyhow::Result<SyncResult> {
        // Check storage limit before starting
        self.check_storage_limit(options)?;

        let sync_state = self
            .storage
            .get_sync_state(collection.as_str(), user_id)
            .await?;

        match sync_state {
            Some(state) if state.has_more_history => {
                self.do_backfill_sync(collection, user_id, options, state)
                    .await
            }
            Some(_) => Ok(SyncResult::empty(SyncDirection::Backfill)),
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
                .get_likes_paginated_with_rate_limit(user_id, pagination, &options.rate_limit)
                .await
                .map_err(|e| anyhow::anyhow!("{}", e)),
            Collection::Bookmarks => self
                .client
                .get_bookmarks_paginated_with_rate_limit(pagination, &options.rate_limit)
                .await
                .map_err(|e| anyhow::anyhow!("{}", e)),
            Collection::UserTweets => self
                .client
                .get_user_tweets_paginated_with_rate_limit(user_id, pagination, &options.rate_limit)
                .await
                .map_err(|e| anyhow::anyhow!("{}", e)),
            Collection::Timeline => Err(anyhow::anyhow!(
                "{} sync not yet implemented",
                collection.as_str()
            )),
        }
    }
}

fn apply_rate_limit_info(state: &mut bird_client::SyncState, rate_limit: &RateLimitConfig) {
    let info = rate_limit.last_rate_limit_info();
    if info.last_rate_limited_at.is_some() {
        state.last_rate_limited_at = info.last_rate_limited_at;
        state.last_rate_limit_backoff_ms = info.last_backoff_ms;
        state.last_rate_limit_retries = info.last_retries;
    }
}

/// Extract a grouping key from a tweet's created_at for auto-export.
fn extract_group_key_for_sync(created_at: &Option<String>, group_by: AutoExportGroupBy) -> String {
    if let Some(ts) = created_at.as_ref() {
        // Twitter format: "Wed Oct 10 20:19:24 +0000 2018"
        if let Ok(dt) = chrono::DateTime::parse_from_str(ts, "%a %b %d %H:%M:%S %z %Y") {
            let date = dt.date_naive();
            return match group_by {
                AutoExportGroupBy::Day => date.format("%Y-%m-%d").to_string(),
                AutoExportGroupBy::Month => date.format("%Y-%m").to_string(),
            };
        }
    }
    "unknown".to_string()
}
