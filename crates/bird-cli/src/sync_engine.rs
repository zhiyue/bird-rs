//! Sync engine for synchronizing tweets to local storage.
//!
//! Supports bidirectional sync:
//! - **Forward sync**: Fetch new items since last sync (stop at the saved sync point)
//! - **Backfill sync**: Continue fetching older items (resume from backfill_cursor)

use crate::commands::export::{append_unique_jsonl, read_existing_jsonl_ids};
use crate::storage_monitor::StorageMonitor;
use bird_client::{Collection, PaginatedResult, PaginationOptions, RateLimitConfig, TwitterClient};
use bird_storage::Storage;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::sleep;

/// Default page ceiling for sync operations.
pub(crate) const DEFAULT_SYNC_MAX_PAGES: u32 = 1_000_000;

/// Number of IDs kept from the top of a mutable collection.
const FORWARD_ANCHOR_SIZE: usize = 20;

/// Number of known IDs needed to migrate an existing collection safely.
const FORWARD_ANCHOR_MATCH_SIZE: usize = 20;

/// Result of a sync operation.
pub struct SyncResult {
    /// Number of new tweets stored.
    pub new_tweets: usize,
    /// Total tweets fetched from API.
    pub total_fetched: usize,
    /// Whether sync stopped at the saved forward sync point.
    pub stopped_at_known: bool,
    /// Whether there's more history to backfill.
    pub has_more_history: bool,
    /// Whether backfill stopped at Twitter's accessible history limit.
    pub history_limit_reached: bool,
    /// The sync direction that was performed.
    pub direction: SyncDirection,
    /// Whether sync was stopped due to storage limit.
    pub stopped_at_storage_limit: bool,
    /// Final storage size in bytes (if available).
    pub final_storage_bytes: Option<u64>,
    pages_fetched: u32,
}

impl SyncResult {
    fn empty(direction: SyncDirection) -> Self {
        Self {
            new_tweets: 0,
            total_fetched: 0,
            stopped_at_known: false,
            has_more_history: false,
            history_limit_reached: false,
            direction,
            stopped_at_storage_limit: false,
            final_storage_bytes: None,
            pages_fetched: 0,
        }
    }
}

#[derive(Clone, Copy, Default)]
struct ProgressOffset {
    fetched: usize,
    new: usize,
}

enum ForwardStopMatcher {
    None,
    Single(String),
    Anchor {
        anchor_ids: HashSet<String>,
        seen_ids: HashSet<String>,
        threshold: usize,
    },
    KnownRun {
        known_ids: HashSet<String>,
        run: usize,
        threshold: usize,
    },
}

impl ForwardStopMatcher {
    fn from_anchor(anchor_ids: &[String]) -> Self {
        if anchor_ids.is_empty() {
            return Self::None;
        }

        let threshold = if anchor_ids.len() >= FORWARD_ANCHOR_MATCH_SIZE {
            anchor_ids.len() - 1
        } else {
            anchor_ids.len()
        };
        Self::Anchor {
            anchor_ids: anchor_ids.iter().cloned().collect(),
            seen_ids: HashSet::new(),
            threshold,
        }
    }

    fn from_known_ids(known_ids: HashSet<String>) -> Self {
        if known_ids.len() < FORWARD_ANCHOR_MATCH_SIZE {
            return Self::None;
        }

        Self::KnownRun {
            known_ids,
            run: 0,
            threshold: FORWARD_ANCHOR_MATCH_SIZE,
        }
    }

    fn push(&mut self, id: &str) -> bool {
        match self {
            Self::None => false,
            Self::Single(stop_id) => id == stop_id,
            Self::Anchor {
                anchor_ids,
                seen_ids,
                threshold,
            } => {
                if anchor_ids.contains(id) {
                    seen_ids.insert(id.to_string());
                }
                seen_ids.len() >= *threshold
            }
            Self::KnownRun {
                known_ids,
                run,
                threshold,
            } => {
                if known_ids.contains(id) {
                    *run += 1;
                } else {
                    *run = 0;
                }
                *run >= *threshold
            }
        }
    }

    fn saw_anchor(&self, id: &str) -> bool {
        matches!(self, Self::Anchor { seen_ids, .. } if seen_ids.contains(id))
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
            max_pages: Some(DEFAULT_SYNC_MAX_PAGES),
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
    auto_export_ids: Mutex<HashMap<PathBuf, HashSet<String>>>,
}

impl SyncEngine {
    /// Create a new sync engine.
    pub fn new(client: TwitterClient, storage: Arc<dyn Storage>) -> Self {
        Self {
            client,
            storage,
            auto_export_ids: Mutex::new(HashMap::new()),
        }
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
    ) -> anyhow::Result<()> {
        let config = match &options.auto_export {
            Some(c) => c,
            None => return Ok(()),
        };

        match config {
            AutoExportConfig::SingleFile(path) => {
                self.append_auto_export(path, tweets)?;
            }
            AutoExportConfig::Grouped { base_dir, group_by } => {
                let mut groups = std::collections::HashMap::<
                    std::path::PathBuf,
                    Vec<bird_client::TweetData>,
                >::new();
                for tweet in tweets {
                    let key = extract_group_key_for_sync(&tweet.created_at, *group_by);
                    let file_path = base_dir.join(format!("{}.jsonl", key));
                    groups.entry(file_path).or_default().push(tweet.clone());
                }
                for (path, grouped_tweets) in groups {
                    self.append_auto_export(&path, &grouped_tweets)?;
                }
            }
        }
        Ok(())
    }

    fn append_auto_export(
        &self,
        path: &std::path::Path,
        tweets: &[bird_client::TweetData],
    ) -> anyhow::Result<()> {
        let mut cache = self
            .auto_export_ids
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let path = path.to_path_buf();
        if !cache.contains_key(&path) {
            let existing_ids = read_existing_jsonl_ids(&path)?;
            cache.insert(path.clone(), existing_ids);
        }
        if let Some(existing_ids) = cache.get_mut(&path) {
            append_unique_jsonl(tweets, &path, existing_ids)?;
        }
        Ok(())
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
    /// 1. **Forward sync** (default on incremental): Fetch new items, stop at the saved sync point
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

        let page_budget = options.max_pages.unwrap_or(DEFAULT_SYNC_MAX_PAGES);
        let mut result = match direction {
            SyncDirection::Full => {
                self.do_full_sync(collection, user_id, options, page_budget)
                    .await?
            }
            SyncDirection::Forward => {
                self.do_forward_sync(
                    collection,
                    user_id,
                    options,
                    sync_state.unwrap(),
                    page_budget,
                )
                .await?
            }
            SyncDirection::Backfill => {
                self.do_backfill_sync(
                    collection,
                    user_id,
                    options,
                    sync_state.unwrap(),
                    page_budget,
                    ProgressOffset::default(),
                )
                .await?
            }
        };

        let remaining_pages = page_budget.saturating_sub(result.pages_fetched);
        if direction == SyncDirection::Forward
            && !options.no_backfill
            && result.has_more_history
            && !result.stopped_at_storage_limit
            && remaining_pages > 0
        {
            if let Some(state) = self
                .storage
                .get_sync_state(collection.as_str(), user_id)
                .await?
                .filter(|state| state.has_more_history)
            {
                let backfill_result = self
                    .do_backfill_sync(
                        collection,
                        user_id,
                        options,
                        state,
                        remaining_pages,
                        ProgressOffset {
                            fetched: result.total_fetched,
                            new: result.new_tweets,
                        },
                    )
                    .await?;

                result.new_tweets += backfill_result.new_tweets;
                result.total_fetched += backfill_result.total_fetched;
                result.pages_fetched += backfill_result.pages_fetched;
                result.has_more_history = backfill_result.has_more_history;
                result.history_limit_reached = backfill_result.history_limit_reached;
                result.stopped_at_storage_limit = backfill_result.stopped_at_storage_limit;
                result.final_storage_bytes = backfill_result.final_storage_bytes;
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
        page_budget: u32,
    ) -> anyhow::Result<SyncResult> {
        let mut state = bird_client::SyncState::new(collection.as_str(), user_id);
        let mut cursor = None;
        let mut total_fetched = 0usize;
        let mut new_tweets = 0usize;
        let mut pages_fetched = 0u32;
        let mut stopped_at_storage_limit = false;
        let mut seen_cursors = HashSet::new();
        let mut top_ids = Vec::with_capacity(FORWARD_ANCHOR_SIZE);

        while pages_fetched < page_budget {
            reject_repeated_cursor(&cursor, &mut seen_cursors)?;
            let result = match self
                .fetch_collection_page(collection, user_id, cursor, options)
                .await
            {
                Ok(result) => result,
                Err(error) if is_bookmark_history_limit(&error) => {
                    state.mark_history_limit_reached();
                    apply_rate_limit_info(&mut state, &options.rate_limit);
                    self.storage.update_sync_state(&state).await?;
                    break;
                }
                Err(error) => return Err(error),
            };
            pages_fetched += 1;

            if result.items.is_empty() {
                state.backfill_cursor = None;
                state.has_more_history = false;
                apply_rate_limit_info(&mut state, &options.rate_limit);
                self.storage.update_sync_state(&state).await?;
                break;
            }

            if state.newest_item_id.is_none() {
                state.newest_item_id = result.items.first().map(|tweet| tweet.id.clone());
            }
            if uses_forward_anchor(collection) && top_ids.len() < FORWARD_ANCHOR_SIZE {
                for tweet in &result.items {
                    if top_ids.len() == FORWARD_ANCHOR_SIZE {
                        break;
                    }
                    top_ids.push(tweet.id.clone());
                }
                state.forward_anchor_ids = top_ids.clone();
            }

            let oldest_id = result.items.last().map(|tweet| tweet.id.clone());
            let item_count = result.items.len();
            let new_count = self
                .store_page(collection, user_id, options, &result.items)
                .await?;
            total_fetched += item_count;
            new_tweets += new_count;

            state.update_backfill(
                oldest_id,
                result.next_cursor.clone(),
                result.has_more,
                item_count as u64,
            );
            apply_rate_limit_info(&mut state, &options.rate_limit);
            self.storage.update_sync_state(&state).await?;
            self.report_progress(options, total_fetched, new_tweets);

            stopped_at_storage_limit = self.check_storage_limit(options).is_err();
            if stopped_at_storage_limit || !result.has_more {
                break;
            }
            self.wait_before_next_page(options, item_count).await;
            cursor = result.next_cursor;
        }

        Ok(SyncResult {
            new_tweets,
            total_fetched,
            stopped_at_known: false,
            has_more_history: state.has_more_history,
            history_limit_reached: state.history_limit_reached,
            direction: SyncDirection::Full,
            stopped_at_storage_limit,
            final_storage_bytes: self.current_storage_size(options),
            pages_fetched,
        })
    }

    /// Perform forward sync (catch up on new items).
    async fn do_forward_sync(
        &self,
        collection: Collection,
        user_id: &str,
        options: &SyncOptions,
        mut sync_state: bird_client::SyncState,
        page_budget: u32,
    ) -> anyhow::Result<SyncResult> {
        let previous_anchor_ids = sync_state.forward_anchor_ids.clone();
        let legacy_anchor = uses_forward_anchor(collection) && previous_anchor_ids.is_empty();
        let mut matcher = if uses_forward_anchor(collection) {
            if legacy_anchor {
                let known_ids = self
                    .storage
                    .get_collection_tweet_ids(collection.as_str(), user_id, None)
                    .await?
                    .into_iter()
                    .collect();
                eprintln!(
                    "Rebuilding the {} sync anchor; each fetched page will be saved.",
                    collection.as_str()
                );
                ForwardStopMatcher::from_known_ids(known_ids)
            } else {
                ForwardStopMatcher::from_anchor(&previous_anchor_ids)
            }
        } else if let Some(newest_id) = sync_state.newest_item_id.clone() {
            ForwardStopMatcher::Single(newest_id)
        } else {
            ForwardStopMatcher::None
        };
        let mut cursor = None;
        let mut top_ids = Vec::with_capacity(FORWARD_ANCHOR_SIZE);
        let mut newest_id = None;
        let mut total_fetched = 0usize;
        let mut new_tweets = 0usize;
        let mut pages_fetched = 0u32;
        let mut stopped_at_known = false;
        let mut has_more = true;
        let mut stopped_at_storage_limit = false;
        let mut seen_cursors = HashSet::new();

        while pages_fetched < page_budget {
            reject_repeated_cursor(&cursor, &mut seen_cursors)?;
            let result = self
                .fetch_collection_page(collection, user_id, cursor, options)
                .await?;
            pages_fetched += 1;
            has_more = result.has_more;

            if newest_id.is_none() {
                newest_id = result.items.first().map(|tweet| tweet.id.clone());
            }
            for tweet in &result.items {
                if top_ids.len() < FORWARD_ANCHOR_SIZE {
                    top_ids.push(tweet.id.clone());
                }
                if matcher.push(&tweet.id) {
                    stopped_at_known = true;
                }
            }

            if !result.items.is_empty() {
                let item_count = result.items.len();
                let new_count = self
                    .store_page(collection, user_id, options, &result.items)
                    .await?;
                total_fetched += item_count;
                new_tweets += new_count;
                self.report_progress(options, total_fetched, new_tweets);
                stopped_at_storage_limit = self.check_storage_limit(options).is_err();
            }

            if stopped_at_known
                || stopped_at_storage_limit
                || result.items.is_empty()
                || !result.has_more
            {
                break;
            }
            self.wait_before_next_page(options, result.items.len())
                .await;
            cursor = result.next_cursor;
        }

        if stopped_at_storage_limit {
            return Ok(SyncResult {
                new_tweets,
                total_fetched,
                stopped_at_known: false,
                has_more_history: sync_state.has_more_history,
                history_limit_reached: sync_state.history_limit_reached,
                direction: SyncDirection::Forward,
                stopped_at_storage_limit: true,
                final_storage_bytes: self.current_storage_size(options),
                pages_fetched,
            });
        }

        if !stopped_at_known && has_more {
            anyhow::bail!(
                "Forward sync reached the page limit before a safe sync point. \
                 Fetched pages were saved locally, but the forward checkpoint was not advanced."
            );
        }

        if uses_forward_anchor(collection) && newest_id.is_none() {
            sync_state.newest_item_id = None;
            sync_state.forward_anchor_ids.clear();
        } else {
            sync_state.update_forward(newest_id, total_fetched as u64);
            if uses_forward_anchor(collection) {
                sync_state.forward_anchor_ids = build_forward_anchor_ids(
                    &top_ids,
                    &previous_anchor_ids,
                    stopped_at_known,
                    &matcher,
                );
            }
        }
        apply_rate_limit_info(&mut sync_state, &options.rate_limit);
        self.storage.update_sync_state(&sync_state).await?;

        Ok(SyncResult {
            new_tweets,
            total_fetched,
            stopped_at_known,
            has_more_history: sync_state.has_more_history,
            history_limit_reached: sync_state.history_limit_reached,
            direction: SyncDirection::Forward,
            stopped_at_storage_limit,
            final_storage_bytes: self.current_storage_size(options),
            pages_fetched,
        })
    }

    /// Perform backfill sync (fetch older items).
    async fn do_backfill_sync(
        &self,
        collection: Collection,
        user_id: &str,
        options: &SyncOptions,
        mut sync_state: bird_client::SyncState,
        page_budget: u32,
        progress_offset: ProgressOffset,
    ) -> anyhow::Result<SyncResult> {
        let mut cursor = sync_state.backfill_cursor.clone();
        let mut total_fetched = 0usize;
        let mut new_tweets = 0usize;
        let mut pages_fetched = 0u32;
        let mut stopped_at_storage_limit = false;
        let mut seen_cursors = HashSet::new();

        while pages_fetched < page_budget && sync_state.has_more_history {
            reject_repeated_cursor(&cursor, &mut seen_cursors)?;
            let result = match self
                .fetch_collection_page(collection, user_id, cursor, options)
                .await
            {
                Ok(result) => result,
                Err(error) if is_bookmark_history_limit(&error) => {
                    sync_state.mark_history_limit_reached();
                    apply_rate_limit_info(&mut sync_state, &options.rate_limit);
                    self.storage.update_sync_state(&sync_state).await?;
                    break;
                }
                Err(error) => return Err(error),
            };
            pages_fetched += 1;

            if result.items.is_empty() {
                sync_state.has_more_history = false;
                sync_state.backfill_cursor = None;
                apply_rate_limit_info(&mut sync_state, &options.rate_limit);
                self.storage.update_sync_state(&sync_state).await?;
                break;
            }

            let oldest_id = result.items.last().map(|tweet| tweet.id.clone());
            let item_count = result.items.len();
            let new_count = self
                .store_page(collection, user_id, options, &result.items)
                .await?;
            total_fetched += item_count;
            new_tweets += new_count;

            sync_state.update_backfill(
                oldest_id,
                result.next_cursor.clone(),
                result.has_more,
                item_count as u64,
            );
            apply_rate_limit_info(&mut sync_state, &options.rate_limit);
            self.storage.update_sync_state(&sync_state).await?;
            self.report_progress(
                options,
                progress_offset.fetched + total_fetched,
                progress_offset.new + new_tweets,
            );

            stopped_at_storage_limit = self.check_storage_limit(options).is_err();
            if stopped_at_storage_limit || !result.has_more {
                break;
            }
            self.wait_before_next_page(options, item_count).await;
            cursor = result.next_cursor;
        }

        Ok(SyncResult {
            new_tweets,
            total_fetched,
            stopped_at_known: false,
            has_more_history: sync_state.has_more_history,
            history_limit_reached: sync_state.history_limit_reached,
            direction: SyncDirection::Backfill,
            stopped_at_storage_limit,
            final_storage_bytes: self.current_storage_size(options),
            pages_fetched,
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
                self.do_backfill_sync(
                    collection,
                    user_id,
                    options,
                    state,
                    options.max_pages.unwrap_or(DEFAULT_SYNC_MAX_PAGES),
                    ProgressOffset::default(),
                )
                .await
            }
            Some(state) => {
                let mut result = SyncResult::empty(SyncDirection::Backfill);
                result.history_limit_reached = state.history_limit_reached;
                Ok(result)
            }
            None => {
                // No sync state, need to do initial sync first
                Err(anyhow::anyhow!(
                    "No sync state found. Run `bird sync {}` first.",
                    collection.as_str()
                ))
            }
        }
    }

    async fn store_page(
        &self,
        collection: Collection,
        user_id: &str,
        options: &SyncOptions,
        tweets: &[bird_client::TweetData],
    ) -> anyhow::Result<usize> {
        let new_count = self.storage.upsert_tweets(tweets).await?;
        for tweet in tweets {
            self.storage
                .add_to_collection(&tweet.id, collection.as_str(), user_id)
                .await?;
        }
        self.auto_export_tweets(options, tweets)?;
        Ok(new_count)
    }

    async fn wait_before_next_page(&self, options: &SyncOptions, item_count: usize) {
        if options.rate_limit.delay_per_tweet_ms > 0 && item_count > 0 {
            let delay_ms = item_count as u64 * options.rate_limit.delay_per_tweet_ms;
            sleep(Duration::from_millis(delay_ms)).await;
        }
    }

    /// Fetch one API page so it can be stored before requesting the next page.
    async fn fetch_collection_page(
        &self,
        collection: Collection,
        user_id: &str,
        cursor: Option<String>,
        options: &SyncOptions,
    ) -> anyhow::Result<PaginatedResult<bird_client::TweetData>> {
        let mut pagination = PaginationOptions::new().with_max_pages(1);
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
                .map_err(anyhow::Error::new),
            Collection::Bookmarks => self
                .client
                .get_bookmarks_paginated_with_rate_limit(pagination, &options.rate_limit)
                .await
                .map_err(anyhow::Error::new),
            Collection::UserTweets => self
                .client
                .get_user_tweets_paginated_with_rate_limit(user_id, pagination, &options.rate_limit)
                .await
                .map_err(anyhow::Error::new),
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

fn uses_forward_anchor(collection: Collection) -> bool {
    matches!(collection, Collection::Bookmarks | Collection::Likes)
}

fn is_bookmark_history_limit(error: &anyhow::Error) -> bool {
    matches!(
        error.downcast_ref::<bird_client::Error>(),
        Some(bird_client::Error::BookmarkHistoryLimitReached)
    )
}

fn reject_repeated_cursor(
    cursor: &Option<String>,
    seen_cursors: &mut HashSet<String>,
) -> anyhow::Result<()> {
    if let Some(cursor) = cursor {
        if !seen_cursors.insert(cursor.clone()) {
            anyhow::bail!(
                "Pagination cursor repeated before a confirmed timeline end; saved progress was preserved"
            );
        }
    }
    Ok(())
}

fn build_forward_anchor_ids(
    top_ids: &[String],
    previous_anchor_ids: &[String],
    stopped_at_known: bool,
    matcher: &ForwardStopMatcher,
) -> Vec<String> {
    let mut anchor_ids = Vec::with_capacity(FORWARD_ANCHOR_SIZE);
    for id in top_ids {
        if !anchor_ids.contains(id) {
            anchor_ids.push(id.clone());
            if anchor_ids.len() == FORWARD_ANCHOR_SIZE {
                return anchor_ids;
            }
        }
    }
    if stopped_at_known {
        for id in previous_anchor_ids {
            if matcher.saw_anchor(id) && !anchor_ids.contains(id) {
                anchor_ids.push(id.clone());
                if anchor_ids.len() == FORWARD_ANCHOR_SIZE {
                    break;
                }
            }
        }
    }
    anchor_ids
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

#[cfg(test)]
mod tests {
    use super::*;
    use bird_client::{TweetAuthor, TwitterClientOptions, TwitterCookies};
    use bird_storage::{MemoryStorage, TweetStore};

    fn make_tweet(id: &str) -> bird_client::TweetData {
        bird_client::TweetData {
            id: id.to_string(),
            text: format!("tweet {}", id),
            author: TweetAuthor {
                username: "tester".to_string(),
                name: "Tester".to_string(),
            },
            author_id: None,
            created_at: None,
            reply_count: None,
            retweet_count: None,
            like_count: None,
            conversation_id: None,
            in_reply_to_status_id: None,
            in_reply_to_user_id: None,
            mentions: Vec::new(),
            quoted_tweet: None,
            retweeted_tweet: None,
            media: None,
            article: None,
            headline: None,
            _raw: None,
        }
    }

    #[test]
    fn forward_anchor_moves_forward_with_new_items() {
        let top = vec!["new-2".to_string(), "new-1".to_string()];
        let previous = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let mut matcher = ForwardStopMatcher::from_anchor(&previous);
        for id in &previous {
            matcher.push(id);
        }

        assert_eq!(
            build_forward_anchor_ids(&top, &previous, true, &matcher),
            vec!["new-2", "new-1", "a", "b", "c"]
        );
    }

    #[test]
    fn forward_anchor_is_rebuilt_when_old_sequence_moved() {
        let top = vec![
            "a".to_string(),
            "new".to_string(),
            "b".to_string(),
            "c".to_string(),
        ];
        let previous = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let matcher = ForwardStopMatcher::from_anchor(&previous);

        assert_eq!(
            build_forward_anchor_ids(&top, &previous, false, &matcher),
            vec!["a", "new", "b", "c"]
        );
    }

    #[test]
    fn forward_anchor_is_limited_to_top_window() {
        let top = (0..FORWARD_ANCHOR_SIZE + 5)
            .map(|index| index.to_string())
            .collect::<Vec<_>>();
        let matcher = ForwardStopMatcher::None;

        assert_eq!(
            build_forward_anchor_ids(&top, &[], false, &matcher).len(),
            FORWARD_ANCHOR_SIZE
        );
    }

    #[test]
    fn forward_anchor_drops_previous_ids_not_seen_again() {
        let top = vec!["new".to_string()];
        let previous = vec![
            "alive-a".to_string(),
            "gone".to_string(),
            "alive-b".to_string(),
        ];
        let mut matcher = ForwardStopMatcher::from_anchor(&previous);
        matcher.push("alive-a");
        matcher.push("alive-b");

        assert_eq!(
            build_forward_anchor_ids(&top, &previous, true, &matcher),
            vec!["new", "alive-a", "alive-b"]
        );
    }

    #[test]
    fn anchor_matcher_survives_one_moved_id() {
        let anchor = (0..FORWARD_ANCHOR_SIZE)
            .map(|index| index.to_string())
            .collect::<Vec<_>>();
        let mut matcher = ForwardStopMatcher::from_anchor(&anchor);
        let mut current = std::iter::once("0".to_string())
            .chain(std::iter::once("new".to_string()))
            .chain((1..FORWARD_ANCHOR_SIZE).map(|index| index.to_string()));

        assert!(current.any(|id| matcher.push(&id)));
    }

    #[test]
    fn anchor_matcher_survives_one_deleted_id() {
        let anchor = (0..FORWARD_ANCHOR_SIZE)
            .map(|index| index.to_string())
            .collect::<Vec<_>>();
        let mut matcher = ForwardStopMatcher::from_anchor(&anchor);

        assert!((0..FORWARD_ANCHOR_SIZE)
            .filter(|index| *index != 9)
            .any(|index| matcher.push(&index.to_string())));
    }

    #[test]
    fn legacy_matcher_skips_a_rebookmarked_item_before_new_items() {
        let known_ids = (0..=20).map(|index| index.to_string()).collect();
        let mut matcher = ForwardStopMatcher::from_known_ids(known_ids);
        let mut current = std::iter::once("0".to_string())
            .chain(std::iter::once("new".to_string()))
            .chain((1..=20).map(|index| index.to_string()));

        assert!(current.any(|id| matcher.push(&id)));
    }

    #[test]
    fn likes_use_the_resilient_forward_anchor() {
        assert!(uses_forward_anchor(Collection::Likes));
        assert!(uses_forward_anchor(Collection::Bookmarks));
        assert!(!uses_forward_anchor(Collection::UserTweets));
    }

    #[test]
    fn repeated_cursor_is_rejected() {
        let mut seen = HashSet::new();
        let cursor = Some("cursor".to_string());

        assert!(reject_repeated_cursor(&cursor, &mut seen).is_ok());
        assert!(reject_repeated_cursor(&cursor, &mut seen).is_err());
    }

    #[test]
    fn bookmark_history_limit_survives_anyhow_conversion() {
        let error = anyhow::Error::new(bird_client::Error::BookmarkHistoryLimitReached);

        assert!(is_bookmark_history_limit(&error));
    }

    #[tokio::test]
    async fn store_page_persists_tweets_and_collection_membership() {
        let storage = Arc::new(MemoryStorage::new());
        let client = TwitterClient::new(TwitterClientOptions {
            cookies: TwitterCookies::new("auth".to_string(), "ct0".to_string()),
            timeout_ms: None,
            quote_depth: Some(0),
        });
        let engine = SyncEngine::new(client, storage.clone());
        let options = SyncOptions {
            rate_limit: RateLimitConfig::none(),
            ..SyncOptions::default()
        };
        let tweets = vec![make_tweet("1"), make_tweet("2")];

        let new_count = engine
            .store_page(Collection::Bookmarks, "user", &options, &tweets)
            .await
            .unwrap();

        assert_eq!(new_count, 2);
        assert!(storage.get_tweet("1").await.unwrap().is_some());
        assert!(storage
            .is_in_collection("2", "bookmarks", "user")
            .await
            .unwrap());
    }

    #[test]
    fn append_unique_jsonl_skips_existing_tweet_ids() {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join("bookmarks.jsonl");
        let tweets = vec![make_tweet("1"), make_tweet("2")];
        let mut existing_ids = HashSet::new();

        assert_eq!(
            append_unique_jsonl(&tweets, &path, &mut existing_ids).unwrap(),
            (2, 2, 0)
        );
        assert_eq!(
            append_unique_jsonl(&tweets, &path, &mut existing_ids).unwrap(),
            (2, 0, 2)
        );

        let lines = std::fs::read_to_string(path).unwrap();
        assert_eq!(lines.lines().count(), 2);
    }
}
