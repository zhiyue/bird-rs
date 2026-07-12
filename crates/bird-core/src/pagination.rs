//! Pagination types for cursor-based API responses.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Options for paginated requests.
#[derive(Debug, Clone, Default)]
pub struct PaginationOptions {
    /// Starting cursor (resume from previous pagination).
    pub cursor: Option<String>,
    /// Maximum number of pages to fetch.
    pub max_pages: Option<u32>,
    /// Fetch all available pages (overrides max_pages).
    pub fetch_all: bool,
    /// Stop when encountering this tweet ID (for incremental sync).
    pub stop_at_id: Option<String>,
    /// Stop when encountering this consecutive sequence of tweet IDs.
    pub stop_at_ids: Vec<String>,
}

impl PaginationOptions {
    /// Create new pagination options.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the starting cursor.
    pub fn with_cursor(mut self, cursor: impl Into<String>) -> Self {
        self.cursor = Some(cursor.into());
        self
    }

    /// Set the maximum number of pages.
    pub fn with_max_pages(mut self, max: u32) -> Self {
        self.max_pages = Some(max);
        self
    }

    /// Enable fetching all pages.
    pub fn fetch_all(mut self) -> Self {
        self.fetch_all = true;
        self
    }

    /// Set the stop-at ID for incremental sync.
    pub fn with_stop_at_id(mut self, id: impl Into<String>) -> Self {
        self.stop_at_id = Some(id.into());
        self
    }

    /// Set the consecutive ID sequence used to stop incremental sync.
    pub fn with_stop_at_ids(mut self, ids: Vec<String>) -> Self {
        self.stop_at_ids = ids;
        self
    }
}

/// Result of a paginated API request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedResult<T> {
    /// Items returned in this page.
    pub items: Vec<T>,
    /// Cursor for the next page, if available.
    pub next_cursor: Option<String>,
    /// Whether there are more pages available.
    pub has_more: bool,
    /// Total items fetched across all pages (for multi-page requests).
    #[serde(default)]
    pub total_fetched: usize,
    /// Whether the request stopped early due to hitting a known item.
    #[serde(default)]
    pub stopped_at_known: bool,
}

impl<T> PaginatedResult<T> {
    /// Create a new paginated result.
    pub fn new(items: Vec<T>, next_cursor: Option<String>) -> Self {
        let has_more = next_cursor.is_some();
        let total_fetched = items.len();
        Self {
            items,
            next_cursor,
            has_more,
            total_fetched,
            stopped_at_known: false,
        }
    }

    /// Create an empty result with no more pages.
    pub fn empty() -> Self {
        Self {
            items: Vec::new(),
            next_cursor: None,
            has_more: false,
            total_fetched: 0,
            stopped_at_known: false,
        }
    }

    /// Mark the result as stopped at a known item.
    pub fn with_stopped_at_known(mut self) -> Self {
        self.stopped_at_known = true;
        self
    }

    /// Set the total fetched count.
    pub fn with_total_fetched(mut self, count: usize) -> Self {
        self.total_fetched = count;
        self
    }
}

/// Sync state for tracking bidirectional sync progress.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncState {
    /// Collection being synced (likes, bookmarks, timeline).
    pub collection: String,
    /// User ID for which the sync is tracked.
    pub user_id: String,
    /// ID of the newest item seen (top of the stream, for catching up on new items).
    pub newest_item_id: Option<String>,
    /// IDs from the top of the previous sync, used as a resilient forward anchor.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub forward_anchor_ids: Vec<String>,
    /// ID of the oldest item seen (how far back we've synced, for backfilling).
    pub oldest_item_id: Option<String>,
    /// Cursor to resume backfilling from (pagination cursor after oldest_item_id).
    pub backfill_cursor: Option<String>,
    /// Whether there's more history to backfill.
    pub has_more_history: bool,
    /// Whether backfill stopped at Twitter's accessible bookmark history limit.
    #[serde(default)]
    pub history_limit_reached: bool,
    /// Timestamp of the last sync.
    pub last_sync_at: DateTime<Utc>,
    /// Timestamp of the most recent rate limit event.
    pub last_rate_limited_at: Option<DateTime<Utc>>,
    /// Backoff delay used on the most recent rate limit event.
    pub last_rate_limit_backoff_ms: Option<u64>,
    /// Retry attempt count on the most recent rate limit event.
    pub last_rate_limit_retries: Option<u32>,
    /// Total number of items synced.
    pub total_synced: u64,
}

impl SyncState {
    /// Create a new sync state.
    pub fn new(collection: impl Into<String>, user_id: impl Into<String>) -> Self {
        Self {
            collection: collection.into(),
            user_id: user_id.into(),
            newest_item_id: None,
            forward_anchor_ids: Vec::new(),
            oldest_item_id: None,
            backfill_cursor: None,
            has_more_history: true, // Assume there's history until proven otherwise
            history_limit_reached: false,
            last_sync_at: Utc::now(),
            last_rate_limited_at: None,
            last_rate_limit_backoff_ms: None,
            last_rate_limit_retries: None,
            total_synced: 0,
        }
    }

    /// Update after catching up on new items (forward sync).
    pub fn update_forward(&mut self, newest_id: Option<String>, items_synced: u64) {
        if newest_id.is_some() {
            self.newest_item_id = newest_id;
        }
        self.last_sync_at = Utc::now();
        self.total_synced += items_synced;
    }

    /// Update after backfilling old items (backward sync).
    pub fn update_backfill(
        &mut self,
        oldest_id: Option<String>,
        cursor: Option<String>,
        has_more: bool,
        items_synced: u64,
    ) {
        if oldest_id.is_some() {
            self.oldest_item_id = oldest_id;
        }
        self.backfill_cursor = cursor;
        self.has_more_history = has_more;
        self.last_sync_at = Utc::now();
        self.total_synced += items_synced;
    }

    /// Mark backfill as complete at Twitter's accessible bookmark history limit.
    pub fn mark_history_limit_reached(&mut self) {
        self.backfill_cursor = None;
        self.has_more_history = false;
        self.history_limit_reached = true;
        self.last_sync_at = Utc::now();
    }

    /// Reset the sync state for a full re-sync.
    pub fn reset(&mut self) {
        self.newest_item_id = None;
        self.forward_anchor_ids.clear();
        self.oldest_item_id = None;
        self.backfill_cursor = None;
        self.has_more_history = true;
        self.history_limit_reached = false;
        self.total_synced = 0;
    }

    /// Check if this is the first sync (no items synced yet).
    pub fn is_first_sync(&self) -> bool {
        self.newest_item_id.is_none() && self.oldest_item_id.is_none() && self.has_more_history
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_state_defaults_forward_anchors_for_old_records() {
        let state: SyncState = serde_json::from_value(serde_json::json!({
            "collection": "bookmarks",
            "user_id": "user",
            "newest_item_id": "1",
            "oldest_item_id": "1",
            "backfill_cursor": null,
            "has_more_history": false,
            "last_sync_at": "2026-01-01T00:00:00Z",
            "last_rate_limited_at": null,
            "last_rate_limit_backoff_ms": null,
            "last_rate_limit_retries": null,
            "total_synced": 1
        }))
        .unwrap();

        assert!(state.forward_anchor_ids.is_empty());
        assert!(!state.history_limit_reached);
    }

    #[test]
    fn reset_clears_forward_anchors() {
        let mut state = SyncState::new("bookmarks", "user");
        state.forward_anchor_ids = vec!["1".to_string(), "2".to_string()];

        state.reset();

        assert!(state.forward_anchor_ids.is_empty());
    }

    #[test]
    fn empty_collection_state_is_not_treated_as_first_sync() {
        let mut state = SyncState::new("bookmarks", "user");
        state.has_more_history = false;

        assert!(!state.is_first_sync());
    }

    #[test]
    fn history_limit_converges_backfill_and_reset_clears_it() {
        let mut state = SyncState::new("bookmarks", "user");
        state.backfill_cursor = Some("cursor".to_string());

        state.mark_history_limit_reached();

        assert!(!state.has_more_history);
        assert!(state.backfill_cursor.is_none());
        assert!(state.history_limit_reached);

        state.reset();

        assert!(state.has_more_history);
        assert!(!state.history_limit_reached);
    }
}
