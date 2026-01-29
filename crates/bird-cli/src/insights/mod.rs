//! Insights generation from synced tweets using LLM analysis.

pub mod entities;
pub mod llm;
pub mod output;
pub mod prompt;

use crate::insights::entities::InsightsResult;
use crate::insights::llm::{LlmError, LlmProvider, LlmRequest};
use crate::insights::prompt::{build_system_prompt, build_user_prompt, parse_response};
use bird_client::TweetData;
use bird_storage::Storage;
use chrono::{DateTime, Duration, Utc};
use std::sync::Arc;
use thiserror::Error;

/// Time period for insights analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimePeriod {
    Day,
    Week,
    Month,
}

impl TimePeriod {
    /// Get the start time for this period (relative to now).
    pub fn start_time(&self) -> DateTime<Utc> {
        let now = Utc::now();
        match self {
            TimePeriod::Day => now - Duration::days(1),
            TimePeriod::Week => now - Duration::weeks(1),
            TimePeriod::Month => now - Duration::days(30),
        }
    }

    /// Get a human-readable description.
    pub fn description(&self) -> &'static str {
        match self {
            TimePeriod::Day => "the last day",
            TimePeriod::Week => "the last week",
            TimePeriod::Month => "the last month",
        }
    }

    /// Get a short label.
    pub fn label(&self) -> &'static str {
        match self {
            TimePeriod::Day => "day",
            TimePeriod::Week => "week",
            TimePeriod::Month => "month",
        }
    }
}

impl std::str::FromStr for TimePeriod {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "day" | "d" | "1d" => Ok(TimePeriod::Day),
            "week" | "w" | "1w" | "7d" => Ok(TimePeriod::Week),
            "month" | "m" | "1m" | "30d" => Ok(TimePeriod::Month),
            _ => Err(format!("Invalid time period: {}. Use day, week, or month.", s)),
        }
    }
}

impl std::fmt::Display for TimePeriod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.label())
    }
}

/// Collection filter for insights.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CollectionFilter {
    /// All collections (likes + bookmarks).
    All,
    /// Only likes.
    Likes,
    /// Only bookmarks.
    Bookmarks,
}

impl std::str::FromStr for CollectionFilter {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "all" => Ok(CollectionFilter::All),
            "likes" => Ok(CollectionFilter::Likes),
            "bookmarks" => Ok(CollectionFilter::Bookmarks),
            _ => Err(format!(
                "Invalid collection filter: {}. Use all, likes, or bookmarks.",
                s
            )),
        }
    }
}

/// Errors that can occur during insights generation.
#[derive(Debug, Error)]
pub enum InsightsError {
    #[error("Storage error: {0}")]
    Storage(String),

    #[error("LLM error: {0}")]
    Llm(#[from] LlmError),

    #[error("No tweets found in the specified time period")]
    NoTweets,

    #[error("Failed to parse insights: {0}")]
    ParseError(String),
}

/// Options for generating insights.
#[derive(Debug, Clone)]
pub struct InsightsOptions {
    /// Time period to analyze.
    pub period: TimePeriod,
    /// Collection filter.
    pub collection: CollectionFilter,
    /// Maximum number of tweets to analyze.
    pub max_tweets: Option<u32>,
    /// Whether to show verbose output.
    pub verbose: bool,
}

impl Default for InsightsOptions {
    fn default() -> Self {
        Self {
            period: TimePeriod::Week,
            collection: CollectionFilter::All,
            max_tweets: None,
            verbose: false,
        }
    }
}

/// Engine for generating insights from tweets.
pub struct InsightsEngine {
    storage: Arc<dyn Storage>,
    llm: Box<dyn LlmProvider>,
}

impl InsightsEngine {
    /// Create a new insights engine.
    pub fn new(storage: Arc<dyn Storage>, llm: Box<dyn LlmProvider>) -> Self {
        Self { storage, llm }
    }

    /// Generate insights for a user.
    pub async fn generate(
        &self,
        user_id: &str,
        options: &InsightsOptions,
    ) -> Result<InsightsResult, InsightsError> {
        let start_time = options.period.start_time();
        let end_time = Utc::now();

        // Fetch tweets from the specified collections
        eprintln!("  Fetching tweets from storage...");
        let mut tweets = Vec::new();
        let max_per_collection = options.max_tweets.unwrap_or(100);

        match options.collection {
            CollectionFilter::All => {
                eprintln!("  Fetching likes...");
                let likes = self
                    .fetch_collection_tweets(
                        "likes",
                        user_id,
                        start_time,
                        end_time,
                        Some(max_per_collection / 2),
                    )
                    .await?;
                eprintln!("  Got {} likes", likes.len());
                eprintln!("  Fetching bookmarks...");
                let bookmarks = self
                    .fetch_collection_tweets(
                        "bookmarks",
                        user_id,
                        start_time,
                        end_time,
                        Some(max_per_collection / 2),
                    )
                    .await?;
                eprintln!("  Got {} bookmarks", bookmarks.len());
                tweets.extend(likes);
                tweets.extend(bookmarks);
            }
            CollectionFilter::Likes => {
                eprintln!("  Fetching likes...");
                tweets = self
                    .fetch_collection_tweets(
                        "likes",
                        user_id,
                        start_time,
                        end_time,
                        Some(max_per_collection),
                    )
                    .await?;
                eprintln!("  Got {} likes", tweets.len());
            }
            CollectionFilter::Bookmarks => {
                eprintln!("  Fetching bookmarks...");
                tweets = self
                    .fetch_collection_tweets(
                        "bookmarks",
                        user_id,
                        start_time,
                        end_time,
                        Some(max_per_collection),
                    )
                    .await?;
                eprintln!("  Got {} bookmarks", tweets.len());
            }
        }

        // Apply max_tweets limit if specified
        if let Some(max) = options.max_tweets {
            tweets.truncate(max as usize);
        }

        if tweets.is_empty() {
            return Err(InsightsError::NoTweets);
        }

        // Log tweet count for debugging
        eprintln!("  Found {} tweets to analyze", tweets.len());

        // Generate insights using LLM
        self.analyze_tweets(&tweets, options).await
    }

    /// Fetch tweets from a collection within a time range.
    async fn fetch_collection_tweets(
        &self,
        collection: &str,
        user_id: &str,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
        limit: Option<u32>,
    ) -> Result<Vec<TweetData>, InsightsError> {
        self.storage
            .get_tweets_by_collection_time_range(collection, user_id, start_time, end_time, limit)
            .await
            .map_err(|e| InsightsError::Storage(e.to_string()))
    }

    /// Analyze tweets using the LLM.
    async fn analyze_tweets(
        &self,
        tweets: &[TweetData],
        options: &InsightsOptions,
    ) -> Result<InsightsResult, InsightsError> {
        let system_prompt = build_system_prompt();
        let user_prompt = build_user_prompt(tweets, options.period.description());

        let request = LlmRequest {
            system: system_prompt,
            user: user_prompt,
            max_tokens: 4096,
            temperature: 0.7,
        };

        let response = self.llm.complete(request).await?;

        parse_response(&response.content, tweets.len())
            .map_err(InsightsError::ParseError)
    }

    /// Get the LLM provider name.
    pub fn llm_name(&self) -> &'static str {
        self.llm.name()
    }

    /// Get the LLM model.
    pub fn llm_model(&self) -> &str {
        self.llm.model()
    }
}
