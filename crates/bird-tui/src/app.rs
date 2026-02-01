//! Application state management for bird-tui.

use bird_storage::{ResonanceScore, Storage};
use chrono::Datelike;
use ratatui::style::Color;
use ratatui::widgets::TableState;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use time::{Date, Month};

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
    pub author_id: Option<String>,
    pub headline: String,
    pub collections: Vec<String>,
    pub resonance_score: ResonanceScore,
    pub created_at: Option<String>,
    /// Parsed timestamp in local time (for calendar filtering/sorting).
    pub created_at_local: Option<chrono::DateTime<chrono::Local>>,
    /// Tweet interaction counts (from Twitter data).
    pub like_count: Option<u64>,
    pub retweet_count: Option<u64>,
    pub reply_count: Option<u64>,
    /// Your interaction stats with this author's tweets
    pub author_liked_count: u32,
    pub author_quoted_count: u32,
    pub author_retweeted_count: u32,
}

/// Which panel is currently focused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    List,
    Detail,
}

/// Which panel is currently focused in calendar mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalendarFocus {
    Calendar,
    Tweets,
}

/// Calendar range selection for filtering tweets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalendarRange {
    Day,
    Week,
    Month,
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

    /// Whether calendar view is shown.
    pub show_calendar: bool,

    /// Currently selected date in calendar (for filtering tweets).
    pub calendar_selected_date: Option<Date>,

    /// Which month is displayed in calendar.
    pub calendar_display_month: Date,

    /// Which panel is currently focused in calendar mode.
    pub calendar_focus: CalendarFocus,

    /// Calendar range selection (day/week/month).
    pub calendar_range: CalendarRange,

    /// All tweets loaded for calendar mode (unfiltered).
    pub calendar_all_tweets: Vec<TweetDisplayData>,

    /// Tweets filtered to the current calendar range.
    pub calendar_tweets: Vec<TweetDisplayData>,

    /// Table state for calendar tweet list selection.
    pub calendar_table_state: TableState,

    /// Whether calendar data needs to reload from storage.
    pub calendar_needs_reload: bool,

    /// Whether calendar filter needs to re-run.
    pub calendar_needs_filter: bool,
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
            show_calendar: false,
            calendar_selected_date: None,
            calendar_display_month: {
                let today = chrono::Local::now();
                Date::from_calendar_date(
                    today.year(),
                    Month::try_from(today.month() as u8).unwrap(),
                    1,
                )
                .unwrap()
            },
            calendar_focus: CalendarFocus::Calendar,
            calendar_range: CalendarRange::Month,
            calendar_all_tweets: Vec::new(),
            calendar_tweets: Vec::new(),
            calendar_table_state: TableState::new().with_selected(Some(0)),
            calendar_needs_reload: false,
            calendar_needs_filter: false,
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

    /// Toggle calendar view.
    pub fn toggle_calendar(&mut self) {
        self.show_calendar = !self.show_calendar;
        if self.show_calendar {
            let today = Self::today_date();
            let selected = self.calendar_selected_date.unwrap_or(today);
            self.calendar_selected_date = Some(Self::clamp_to_today(selected));
            self.calendar_focus = CalendarFocus::Calendar;
            self.calendar_range = CalendarRange::Month;
            self.calendar_needs_reload = self.calendar_all_tweets.is_empty();
            self.calendar_needs_filter = true;
            self.calendar_display_month =
                Self::first_day_of_month(self.calendar_selected_date.unwrap_or(today));
        } else {
            self.calendar_selected_date = None;
        }
    }

    /// Navigate calendar to previous month.
    pub fn calendar_prev_month(&mut self) {
        let (year, month_num) = if self.calendar_display_month.month() as u32 == 1 {
            (self.calendar_display_month.year() - 1, 12)
        } else {
            (
                self.calendar_display_month.year(),
                self.calendar_display_month.month() as u32 - 1,
            )
        };
        if let Ok(month) = Month::try_from(month_num as u8) {
            if let Ok(date) = Date::from_calendar_date(year, month, 1) {
                self.calendar_display_month = date;
                self.adjust_selected_date_for_month();
                self.calendar_needs_filter = true;
            }
        }
    }

    /// Navigate calendar to next month.
    pub fn calendar_next_month(&mut self) {
        let (year, month_num) = if self.calendar_display_month.month() as u32 == 12 {
            (self.calendar_display_month.year() + 1, 1)
        } else {
            (
                self.calendar_display_month.year(),
                self.calendar_display_month.month() as u32 + 1,
            )
        };
        let today = Self::today_date();
        let today_year = today.year();
        let today_month = today.month() as u32;

        if year > today_year || (year == today_year && month_num > today_month) {
            return;
        }

        if let Ok(month) = Month::try_from(month_num as u8) {
            if let Ok(date) = Date::from_calendar_date(year, month, 1) {
                self.calendar_display_month = date;
                self.adjust_selected_date_for_month();
                self.calendar_needs_filter = true;
            }
        }
    }

    /// Move the calendar selection by a number of days.
    pub fn calendar_move_days(&mut self, delta_days: i64) {
        let today = Self::today_date();
        let selected = self.calendar_selected_date.unwrap_or(today);
        let Some(selected_naive) = Self::date_to_naive(selected) else {
            return;
        };
        let Some(today_naive) = Self::date_to_naive(today) else {
            return;
        };

        let mut candidate = selected_naive + chrono::Duration::days(delta_days);
        if candidate > today_naive {
            candidate = today_naive;
        }

        if let Some(candidate_date) = Self::naive_to_date(candidate) {
            self.calendar_selected_date = Some(candidate_date);
            self.calendar_display_month = Self::first_day_of_month(candidate_date);
            self.calendar_needs_filter = true;
        }
    }

    /// Move the calendar selection by a number of weeks.
    pub fn calendar_move_weeks(&mut self, delta_weeks: i64) {
        self.calendar_move_days(delta_weeks.saturating_mul(7));
    }

    /// Jump to today's date in the calendar.
    pub fn calendar_go_today(&mut self) {
        let today = Self::today_date();
        self.calendar_selected_date = Some(today);
        self.calendar_display_month = Self::first_day_of_month(today);
        self.calendar_needs_filter = true;
    }

    /// Toggle focus between calendar and tweet list in calendar mode.
    pub fn calendar_toggle_focus(&mut self) {
        self.calendar_focus = match self.calendar_focus {
            CalendarFocus::Calendar => CalendarFocus::Tweets,
            CalendarFocus::Tweets => CalendarFocus::Calendar,
        };
    }

    /// Set the calendar range type.
    pub fn calendar_set_range(&mut self, range: CalendarRange) {
        if self.calendar_range != range {
            self.calendar_range = range;
            self.calendar_needs_filter = true;
        }
    }

    /// Get the currently selected tweet in calendar list, if any.
    pub fn calendar_selected_tweet(&self) -> Option<&TweetDisplayData> {
        let index = self.calendar_table_state.selected().unwrap_or(0);
        self.calendar_tweets.get(index)
    }

    /// Select the next tweet in the calendar list.
    pub fn calendar_select_next(&mut self) {
        if self.calendar_tweets.is_empty() {
            return;
        }
        if self.calendar_table_state.selected().is_none() {
            self.calendar_table_state.select(Some(0));
            return;
        }
        let current = self.calendar_table_state.selected().unwrap_or(0);
        if current < self.calendar_tweets.len().saturating_sub(1) {
            self.calendar_table_state.select(Some(current + 1));
        }
    }

    /// Select the previous tweet in the calendar list.
    pub fn calendar_select_prev(&mut self) {
        if self.calendar_table_state.selected().is_none() {
            self.calendar_table_state.select(Some(0));
            return;
        }
        let current = self.calendar_table_state.selected().unwrap_or(0);
        if current > 0 {
            self.calendar_table_state.select(Some(current - 1));
        }
    }

    /// Reset calendar tweet selection to the first item.
    pub fn calendar_reset_selection(&mut self) {
        if self.calendar_tweets.is_empty() {
            self.calendar_table_state.select(None);
        } else {
            self.calendar_table_state.select(Some(0));
        }
    }

    /// Get the active calendar range as naive dates (inclusive).
    pub fn calendar_range_naive(&self) -> Option<(chrono::NaiveDate, chrono::NaiveDate)> {
        let today = Self::today_date();
        let today_naive = Self::date_to_naive(today)?;
        let selected = Self::clamp_to_today(self.calendar_selected_date?);
        let selected_naive = Self::date_to_naive(selected)?;

        match self.calendar_range {
            CalendarRange::Day => Some((selected_naive, selected_naive)),
            CalendarRange::Week => {
                let weekday_offset = selected.weekday().number_days_from_sunday() as i64;
                let start = selected_naive - chrono::Duration::days(weekday_offset);
                let mut end = start + chrono::Duration::days(6);
                if end > today_naive {
                    end = today_naive;
                }
                Some((start, end))
            }
            CalendarRange::Month => {
                let display = self.calendar_display_month;
                let display_naive = Self::date_to_naive(display)?;
                let days = days_in_month(display.year(), display.month() as u32);
                let start = chrono::NaiveDate::from_ymd_opt(
                    display_naive.year(),
                    display_naive.month(),
                    1,
                )?;
                let mut end = chrono::NaiveDate::from_ymd_opt(
                    display_naive.year(),
                    display_naive.month(),
                    days,
                )?;
                if end > today_naive {
                    end = today_naive;
                }
                Some((start, end))
            }
        }
    }

    fn adjust_selected_date_for_month(&mut self) {
        let Some(selected) = self.calendar_selected_date else {
            return;
        };
        let days = days_in_month(
            self.calendar_display_month.year(),
            self.calendar_display_month.month() as u32,
        );
        let mut day = selected.day();
        if day as u32 > days {
            day = days as u8;
        }

        if let Ok(date) = Date::from_calendar_date(
            self.calendar_display_month.year(),
            self.calendar_display_month.month(),
            day,
        ) {
            let clamped = Self::clamp_to_today(date);
            self.calendar_selected_date = Some(clamped);
            self.calendar_display_month = Self::first_day_of_month(clamped);
        }
    }

    fn today_date() -> Date {
        let today = chrono::Local::now().date_naive();
        let month = Month::try_from(today.month() as u8).unwrap_or(Month::January);
        Date::from_calendar_date(today.year(), month, today.day() as u8)
            .unwrap_or_else(|_| Date::from_calendar_date(1970, Month::January, 1).unwrap())
    }

    fn clamp_to_today(date: Date) -> Date {
        let today = Self::today_date();
        if date > today {
            today
        } else {
            date
        }
    }

    fn first_day_of_month(date: Date) -> Date {
        Date::from_calendar_date(date.year(), date.month(), 1).unwrap_or(date)
    }

    fn date_to_naive(date: Date) -> Option<chrono::NaiveDate> {
        chrono::NaiveDate::from_ymd_opt(date.year(), date.month() as u32, date.day() as u32)
    }

    fn naive_to_date(date: chrono::NaiveDate) -> Option<Date> {
        let month = Month::try_from(date.month() as u8).ok()?;
        Date::from_calendar_date(date.year(), month, date.day() as u8).ok()
    }
}

/// Get number of days in a month.
fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 {
                29
            } else {
                28
            }
        }
        _ => 0,
    }
}
