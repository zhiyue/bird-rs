//! Application state management for bird-tui.

use bird_storage::{ResonanceScore, Storage};
use ratatui::style::Color;
use ratatui::widgets::TableState;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Instant, Duration};

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

/// Available color themes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeMode {
    Dark,
    Light,
}

/// Color theme configuration.
#[derive(Debug, Clone)]
pub struct Theme {
    pub mode: ThemeMode,
    pub primary: Color,
    pub highlight: Color,
    pub border: Color,
    pub text: Color,
    pub background: Color,
}

impl Theme {
    /// Create a dark theme.
    pub fn dark() -> Self {
        Self {
            mode: ThemeMode::Dark,
            primary: Color::Cyan,
            highlight: Color::Blue,
            border: Color::DarkGray,
            text: Color::White,
            background: Color::Black,
        }
    }

    /// Create a light theme.
    pub fn light() -> Self {
        Self {
            mode: ThemeMode::Light,
            primary: Color::Blue,
            highlight: Color::Yellow,
            border: Color::White,
            text: Color::Black,
            background: Color::White,
        }
    }

    /// Toggle between dark and light themes.
    pub fn toggle(&mut self) {
        *self = match self.mode {
            ThemeMode::Dark => Self::light(),
            ThemeMode::Light => Self::dark(),
        };
    }
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

    /// Table state for managing selection and scroll position.
    pub table_state: TableState,

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

    /// Current color theme.
    pub theme: Theme,

    /// Whether search modal is shown.
    pub show_search: bool,

    /// Search input query string.
    pub search_query: String,

    /// Filtered tweet indices based on search query.
    pub filtered_indices: Vec<usize>,

    /// Last time search input was modified (for debouncing).
    pub search_input_time: Instant,

    /// Last search query that was processed (for result caching).
    pub last_processed_search: String,
}

impl App {
    /// Create a new application state.
    pub fn new(storage: Arc<dyn Storage>, user_id: String) -> Self {
        Self {
            tweets: Vec::new(),
            page_cache: HashMap::new(),
            tweet_collections: HashMap::new(),
            resonance_scores: HashMap::new(),
            table_state: TableState::new().with_selected(Some(0)),
            detail_scroll_offset: 0,
            current_page: 0,
            page_size: 50,
            total_count: 0,
            focus: Focus::List,
            show_help: false,
            error: None,
            loading: false,
            storage,
            user_id,
            list_scroll_pos: 0,
            frame: 0,
            theme: Theme::dark(),
            show_search: false,
            search_query: String::new(),
            filtered_indices: Vec::new(),
            search_input_time: Instant::now(),
            last_processed_search: String::new(),
        }
    }

    /// Get the currently selected tweet index.
    pub fn selected_index(&self) -> usize {
        self.table_state.selected().unwrap_or(0)
    }

    /// Select the next tweet in the list.
    pub fn select_next(&mut self) {
        if self.tweets.is_empty() {
            return;
        }
        let current = self.selected_index();
        if current < self.tweets.len() - 1 {
            self.table_state.select(Some(current + 1));
            self.detail_scroll_offset = 0;
        }
    }

    /// Select the previous tweet in the list.
    pub fn select_prev(&mut self) {
        let current = self.selected_index();
        if current > 0 {
            self.table_state.select(Some(current - 1));
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
        self.tweets.get(self.selected_index())
    }

    /// Clear all tweets and reset state.
    pub fn clear(&mut self) {
        self.tweets.clear();
        self.table_state.select(Some(0));
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

    /// Toggle search modal.
    pub fn toggle_search(&mut self) {
        self.show_search = !self.show_search;
        if !self.show_search {
            self.search_query.clear();
        }
        self.update_search();
    }

    /// Mark search input as modified (for debouncing).
    pub fn mark_search_modified(&mut self) {
        self.search_input_time = Instant::now();
    }

    /// Check if enough time has passed to apply search filter (debounced).
    pub fn should_apply_search(&self) -> bool {
        self.search_input_time.elapsed() >= Duration::from_millis(100)
    }

    /// Update search results based on current query (with debouncing and caching).
    pub fn update_search(&mut self) {
        // Skip if search hasn't stabilized (debouncing)
        if !self.should_apply_search() {
            return;
        }

        let query = self.search_query.to_lowercase();

        // Skip if query hasn't changed (caching)
        if query == self.last_processed_search {
            return;
        }

        if query.is_empty() {
            // Show all tweets if search is empty
            self.filtered_indices = (0..self.tweets.len()).collect();
        } else {
            // Filter tweets by text content, author, or headline
            self.filtered_indices = self
                .tweets
                .iter()
                .enumerate()
                .filter(|(_, tweet)| {
                    let text_lower = tweet.text.to_lowercase();
                    let author_lower = tweet.author_username.to_lowercase();
                    let headline_lower = tweet.headline.to_lowercase();

                    text_lower.contains(&query)
                        || author_lower.contains(&query)
                        || headline_lower.contains(&query)
                })
                .map(|(i, _)| i)
                .collect();
        }

        // Cache the processed query
        self.last_processed_search = query;

        // Reset selection to first result
        self.table_state.select(Some(0));
    }

    /// Get tweets to display (filtered if search is active).
    pub fn display_tweets(&self) -> Vec<&TweetDisplayData> {
        if self.show_search && !self.filtered_indices.is_empty() {
            self.filtered_indices
                .iter()
                .filter_map(|&i| self.tweets.get(i))
                .collect()
        } else if self.show_search {
            // Show empty results if search is active but no matches
            Vec::new()
        } else {
            // Show all tweets
            self.tweets.iter().collect()
        }
    }
}
