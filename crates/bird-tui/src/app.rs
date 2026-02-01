//! Application state management for bird-tui.

use bird_storage::{ResonanceScore, Storage};
use std::collections::HashMap;
use std::sync::Arc;

/// Collection membership information for a tweet.
#[derive(Debug, Clone)]
pub struct CollectionMembership {
    pub collection: String,
    pub added_at: Option<i64>,
}

/// Rendering-friendly tweet data with computed resonance.
#[derive(Debug, Clone)]
pub struct TweetDisplayData {
    pub id: String,
    pub text: String,
    pub author_username: String,
    pub author_name: String,
    pub headline: String,
    pub collections: Vec<String>,
    pub resonance_score: ResonanceScore,
    pub created_at: Option<String>,
}

/// Which panel is currently focused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    List,
    Detail,
}

/// Main application state.
pub struct App {
    /// All tweets currently loaded (for current page).
    pub tweets: Vec<TweetDisplayData>,

    /// Cache of preloaded pages for faster pagination.
    /// Key is page number, value is list of tweets for that page.
    pub page_cache: HashMap<u32, Vec<TweetDisplayData>>,

    /// Collection membership info for all tweets in database.
    pub tweet_collections: HashMap<String, Vec<CollectionMembership>>,

    /// Resonance scores for all tweets in database.
    pub resonance_scores: HashMap<String, ResonanceScore>,

    /// Currently selected tweet index in the list.
    pub selected_index: usize,

    /// Scroll offset within the detail panel.
    pub detail_scroll_offset: usize,

    /// Current page number (0-indexed).
    pub current_page: u32,

    /// Tweets per page.
    pub page_size: u32,

    /// Total tweet count.
    pub total_count: u64,

    /// Which panel has focus.
    pub focus: Focus,

    /// Whether to show help modal.
    pub show_help: bool,

    /// Error message to display (if any).
    pub error: Option<String>,

    /// Loading state.
    pub loading: bool,

    /// Storage backend reference.
    pub storage: Arc<dyn Storage>,

    /// Current user ID.
    pub user_id: String,

    /// Scroll position in the tweet list (for scrollbar display).
    pub list_scroll_pos: u16,

    /// Frame counter for animations.
    pub frame: u32,
}

impl App {
    /// Create a new application state.
    pub fn new(storage: Arc<dyn Storage>, user_id: String) -> Self {
        Self {
            tweets: Vec::new(),
            page_cache: HashMap::new(),
            tweet_collections: HashMap::new(),
            resonance_scores: HashMap::new(),
            selected_index: 0,
            detail_scroll_offset: 0,
            current_page: 0,
            page_size: 20,
            total_count: 0,
            focus: Focus::List,
            show_help: false,
            error: None,
            loading: false,
            storage,
            user_id,
            list_scroll_pos: 0,
            frame: 0,
        }
    }

    /// Select the next tweet in the list.
    pub fn select_next(&mut self) {
        if self.tweets.is_empty() {
            return;
        }
        if self.selected_index < self.tweets.len() - 1 {
            self.selected_index += 1;
            self.detail_scroll_offset = 0;
        }
    }

    /// Select the previous tweet in the list.
    pub fn select_prev(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
            self.detail_scroll_offset = 0;
        }
    }

    /// Switch focus between list and detail panels.
    pub fn switch_focus(&mut self) {
        self.focus = match self.focus {
            Focus::List => Focus::Detail,
            Focus::Detail => Focus::List,
        };
    }

    /// Toggle help modal visibility.
    pub fn toggle_help(&mut self) {
        self.show_help = !self.show_help;
    }

    /// Get the currently selected tweet, if any.
    pub fn selected_tweet(&self) -> Option<&TweetDisplayData> {
        self.tweets.get(self.selected_index)
    }

    /// Clear all tweets and reset state.
    pub fn clear(&mut self) {
        self.tweets.clear();
        self.selected_index = 0;
        self.detail_scroll_offset = 0;
        self.error = None;
    }

    /// Set an error message.
    pub fn set_error(&mut self, message: impl Into<String>) {
        self.error = Some(message.into());
    }

    /// Clear the error message.
    pub fn clear_error(&mut self) {
        self.error = None;
    }
}
