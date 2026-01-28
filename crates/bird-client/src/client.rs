//! Twitter client implementation.

use crate::constants::{features, Operation, BEARER_TOKEN, DEFAULT_USER_AGENT, TWITTER_API_BASE};
use crate::cookies::TwitterCookies;
use crate::TwitterClientOptions;
use bird_core::{
    CurrentUser, CurrentUserResult, PaginatedResult, PaginationOptions, Result, TweetData,
};
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, CONTENT_TYPE, COOKIE, USER_AGENT};
use reqwest::Client;
use std::time::Duration;
use tokio::time::sleep;
use uuid::Uuid;

/// Configuration for rate limiting to avoid getting banned.
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Delay between page requests in milliseconds (default: 1000ms).
    pub delay_ms: u64,
    /// Maximum number of retries on rate limit (429) response.
    pub max_retries: u32,
    /// Initial backoff delay in milliseconds for 429 responses (default: 1000ms).
    pub initial_backoff_ms: u64,
    /// Maximum backoff delay in milliseconds (default: 16000ms).
    pub max_backoff_ms: u64,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            delay_ms: 1000,           // 1 second between pages
            max_retries: 4,           // Try up to 4 times on 429
            initial_backoff_ms: 1000, // Start with 1s backoff
            max_backoff_ms: 16000,    // Cap at 16s backoff
        }
    }
}

impl RateLimitConfig {
    /// Create a new rate limit config with custom delay.
    pub fn with_delay(delay_ms: u64) -> Self {
        Self {
            delay_ms,
            ..Default::default()
        }
    }

    /// No rate limiting (for testing or when you know you won't hit limits).
    pub fn none() -> Self {
        Self {
            delay_ms: 0,
            max_retries: 0,
            initial_backoff_ms: 0,
            max_backoff_ms: 0,
        }
    }
}

/// Twitter client for interacting with the GraphQL API.
pub struct TwitterClient {
    pub(crate) http_client: Client,
    #[allow(dead_code)]
    pub(crate) auth_token: String,
    pub(crate) ct0: String,
    pub(crate) cookie_header: String,
    pub(crate) user_agent: String,
    #[allow(dead_code)]
    pub(crate) timeout_ms: Option<u64>,
    pub(crate) quote_depth: u32,
    pub(crate) client_uuid: String,
    pub(crate) client_device_id: String,
    pub(crate) client_user_id: Option<String>,
}

impl TwitterClient {
    /// Create a new Twitter client with the given options.
    pub fn new(options: TwitterClientOptions) -> Self {
        let http_client = Client::builder()
            .timeout(Duration::from_millis(options.timeout_ms.unwrap_or(30000)))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            http_client,
            auth_token: options.cookies.auth_token,
            ct0: options.cookies.ct0,
            cookie_header: options.cookies.cookie_header,
            user_agent: DEFAULT_USER_AGENT.to_string(),
            timeout_ms: options.timeout_ms,
            quote_depth: options.quote_depth.unwrap_or(1),
            client_uuid: Uuid::new_v4().to_string(),
            client_device_id: Uuid::new_v4().to_string(),
            client_user_id: None,
        }
    }

    /// Create a new Twitter client from cookies.
    pub fn from_cookies(cookies: TwitterCookies) -> Self {
        Self::new(TwitterClientOptions {
            cookies,
            timeout_ms: None,
            quote_depth: None,
        })
    }

    /// Get the default headers for API requests.
    pub(crate) fn get_headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();

        headers.insert(ACCEPT, HeaderValue::from_static("*/*"));
        headers.insert(
            "accept-language",
            HeaderValue::from_static("en-US,en;q=0.9"),
        );
        headers.insert("authorization", HeaderValue::from_static(BEARER_TOKEN));
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert("x-csrf-token", HeaderValue::from_str(&self.ct0).unwrap());
        headers.insert(
            "x-twitter-auth-type",
            HeaderValue::from_static("OAuth2Session"),
        );
        headers.insert("x-twitter-active-user", HeaderValue::from_static("yes"));
        headers.insert("x-twitter-client-language", HeaderValue::from_static("en"));
        headers.insert(
            "x-client-uuid",
            HeaderValue::from_str(&self.client_uuid).unwrap(),
        );
        headers.insert(
            "x-twitter-client-deviceid",
            HeaderValue::from_str(&self.client_device_id).unwrap(),
        );
        headers.insert(
            "x-client-transaction-id",
            HeaderValue::from_str(&self.create_transaction_id()).unwrap(),
        );
        headers.insert(COOKIE, HeaderValue::from_str(&self.cookie_header).unwrap());
        headers.insert(USER_AGENT, HeaderValue::from_str(&self.user_agent).unwrap());
        headers.insert("origin", HeaderValue::from_static("https://x.com"));
        headers.insert("referer", HeaderValue::from_static("https://x.com/"));

        if let Some(ref user_id) = self.client_user_id {
            headers.insert(
                "x-twitter-client-user-id",
                HeaderValue::from_str(user_id).unwrap(),
            );
        }

        headers
    }

    /// Create a random transaction ID.
    fn create_transaction_id(&self) -> String {
        use std::fmt::Write;
        let bytes: [u8; 16] = rand::random();
        let mut hex = String::with_capacity(32);
        for byte in bytes {
            write!(hex, "{:02x}", byte).unwrap();
        }
        hex
    }

    /// Build a GraphQL URL for the given operation.
    pub(crate) fn build_graphql_url(
        &self,
        operation: Operation,
        variables: &serde_json::Value,
    ) -> String {
        let query_id = operation.default_query_id();
        let features_json = serde_json::to_string(&features::default_features()).unwrap();
        let variables_json = serde_json::to_string(variables).unwrap();

        format!(
            "{}/{}/{}?variables={}&features={}",
            TWITTER_API_BASE,
            query_id,
            operation.name(),
            urlencoding::encode(&variables_json),
            urlencoding::encode(&features_json)
        )
    }

    /// Build a GraphQL URL with field toggles for operations that require them.
    pub(crate) fn build_graphql_url_with_field_toggles(
        &self,
        operation: Operation,
        variables: &serde_json::Value,
    ) -> String {
        let query_id = operation.default_query_id();
        let features_json = serde_json::to_string(&features::tweet_detail_features()).unwrap();
        let variables_json = serde_json::to_string(variables).unwrap();
        let field_toggles = serde_json::json!({
            "withArticlePlainText": true,
            "withArticleRichContentState": true
        });
        let field_toggles_json = serde_json::to_string(&field_toggles).unwrap();

        format!(
            "{}/{}/{}?variables={}&features={}&fieldToggles={}",
            TWITTER_API_BASE,
            query_id,
            operation.name(),
            urlencoding::encode(&variables_json),
            urlencoding::encode(&features_json),
            urlencoding::encode(&field_toggles_json)
        )
    }

    /// Get the current authenticated user.
    /// Tries multiple endpoints in order until one succeeds.
    /// Falls back to extracting user ID from cookie if all endpoints fail.
    pub async fn get_current_user(&mut self) -> CurrentUserResult {
        // Try multiple endpoints like the upstream TypeScript bird project
        let candidate_urls = [
            "https://x.com/i/api/account/settings.json",
            "https://api.twitter.com/1.1/account/settings.json",
            "https://x.com/i/api/account/verify_credentials.json?skip_status=true&include_entities=false",
            "https://api.twitter.com/1.1/account/verify_credentials.json?skip_status=true&include_entities=false",
        ];

        let screen_name_regex = regex::Regex::new(r#""screen_name":"([^"]+)""#).unwrap();
        let user_id_regex = regex::Regex::new(r#""user_id"\s*:\s*"(\d+)""#).unwrap();
        let user_id_str_regex = regex::Regex::new(r#""user_id_str"\s*:\s*"(\d+)""#).unwrap();
        let id_str_regex = regex::Regex::new(r#""id_str"\s*:\s*"(\d+)""#).unwrap();
        let name_regex = regex::Regex::new(r#""name":"([^"\\]*(?:\\.[^"\\]*)*)""#).unwrap();

        let mut last_error = String::new();

        for url in candidate_urls {
            let response = match self
                .http_client
                .get(url)
                .headers(self.get_headers())
                .send()
                .await
            {
                Ok(resp) => resp,
                Err(e) => {
                    last_error = e.to_string();
                    continue;
                }
            };

            if !response.status().is_success() {
                last_error = format!("HTTP {}", response.status());
                continue;
            }

            let text = match response.text().await {
                Ok(t) => t,
                Err(e) => {
                    last_error = e.to_string();
                    continue;
                }
            };

            let username = screen_name_regex
                .captures(&text)
                .and_then(|c| c.get(1))
                .map(|m| m.as_str().to_string());

            // Try multiple patterns for user ID
            let id = user_id_regex
                .captures(&text)
                .or_else(|| user_id_str_regex.captures(&text))
                .or_else(|| id_str_regex.captures(&text))
                .and_then(|c| c.get(1))
                .map(|m| m.as_str().to_string());

            let name = name_regex
                .captures(&text)
                .and_then(|c| c.get(1))
                .map(|m| m.as_str().to_string());

            if let (Some(id), Some(username)) = (id, username) {
                self.client_user_id = Some(id.clone());
                return CurrentUserResult::Success(CurrentUser {
                    id,
                    username: username.clone(),
                    name: name.unwrap_or(username),
                });
            }

            last_error = "Failed to parse user info from response".to_string();
        }

        // Fallback: Try to extract user ID from twid cookie (format: u%3D{user_id} or u={user_id})
        // This allows likes/bookmarks to work even if account settings endpoint fails
        let twid_regex = regex::Regex::new(r"twid=u(?:%3D|=)(\d+)").unwrap();
        if let Some(captures) = twid_regex.captures(&self.cookie_header) {
            if let Some(user_id) = captures.get(1).map(|m| m.as_str().to_string()) {
                self.client_user_id = Some(user_id.clone());
                return CurrentUserResult::Success(CurrentUser {
                    id: user_id,
                    username: "unknown".to_string(),
                    name: "Unknown User".to_string(),
                });
            }
        }

        CurrentUserResult::Error(last_error)
    }

    /// Get the current user ID (must call get_current_user first).
    pub fn current_user_id(&self) -> Option<&str> {
        self.client_user_id.as_deref()
    }

    /// Get a tweet by ID.
    pub async fn get_tweet(&self, tweet_id: &str) -> Result<TweetData> {
        self.get_tweet_detail(tweet_id).await
    }

    /// Get user's likes with pagination.
    pub async fn get_likes(
        &self,
        user_id: &str,
        options: &PaginationOptions,
    ) -> Result<PaginatedResult<TweetData>> {
        self.fetch_likes(user_id, options).await
    }

    /// Get user's likes with multi-page pagination support.
    pub async fn get_likes_paginated_with_rate_limit(
        &self,
        user_id: &str,
        options: &PaginationOptions,
        rate_limit: &RateLimitConfig,
    ) -> Result<PaginatedResult<TweetData>> {
        Self::fetch_all_pages_with_rate_limit(
            |cursor| async {
                let opts = if let Some(c) = cursor {
                    PaginationOptions::new().with_cursor(c)
                } else {
                    PaginationOptions::new()
                };
                self.fetch_likes(user_id, &opts).await
            },
            options,
            rate_limit,
        )
        .await
    }

    /// Get user's bookmarks with pagination.
    pub async fn get_bookmarks(
        &self,
        options: &PaginationOptions,
    ) -> Result<PaginatedResult<TweetData>> {
        self.fetch_bookmarks(options).await
    }

    /// Get user's bookmarks with multi-page pagination support.
    pub async fn get_bookmarks_paginated_with_rate_limit(
        &self,
        options: &PaginationOptions,
        rate_limit: &RateLimitConfig,
    ) -> Result<PaginatedResult<TweetData>> {
        Self::fetch_all_pages_with_rate_limit(
            |cursor| async {
                let opts = if let Some(c) = cursor {
                    PaginationOptions::new().with_cursor(c)
                } else {
                    PaginationOptions::new()
                };
                self.fetch_bookmarks(&opts).await
            },
            options,
            rate_limit,
        )
        .await
    }

    /// Get home timeline with pagination.
    pub async fn get_timeline(
        &self,
        options: &PaginationOptions,
    ) -> Result<PaginatedResult<TweetData>> {
        self.fetch_timeline(options).await
    }

    /// Get user's own tweets with pagination.
    pub async fn get_user_tweets(
        &self,
        user_id: &str,
        options: &PaginationOptions,
    ) -> Result<PaginatedResult<TweetData>> {
        self.fetch_user_tweets(user_id, options).await
    }

    /// Get user's own tweets with multi-page pagination support.
    pub async fn get_user_tweets_paginated_with_rate_limit(
        &self,
        user_id: &str,
        options: &PaginationOptions,
        rate_limit: &RateLimitConfig,
    ) -> Result<PaginatedResult<TweetData>> {
        Self::fetch_all_pages_with_rate_limit(
            |cursor| async {
                let opts = if let Some(c) = cursor {
                    PaginationOptions::new().with_cursor(c)
                } else {
                    PaginationOptions::new()
                };
                self.fetch_user_tweets(user_id, &opts).await
            },
            options,
            rate_limit,
        )
        .await
    }

    /// Fetch all pages of likes (convenience method).
    pub async fn get_all_likes(
        &self,
        user_id: &str,
        max_pages: Option<u32>,
    ) -> Result<PaginatedResult<TweetData>> {
        self.get_all_likes_with_rate_limit(user_id, max_pages, &RateLimitConfig::default())
            .await
    }

    /// Fetch all pages of likes with custom rate limit config.
    pub async fn get_all_likes_with_rate_limit(
        &self,
        user_id: &str,
        max_pages: Option<u32>,
        rate_limit: &RateLimitConfig,
    ) -> Result<PaginatedResult<TweetData>> {
        let mut options = PaginationOptions::new();
        if let Some(max) = max_pages {
            options = options.with_max_pages(max);
        } else {
            options = options.fetch_all();
        }
        self.get_likes_paginated_with_rate_limit(user_id, &options, rate_limit)
            .await
    }

    /// Fetch all pages of bookmarks (convenience method).
    pub async fn get_all_bookmarks(
        &self,
        max_pages: Option<u32>,
    ) -> Result<PaginatedResult<TweetData>> {
        self.get_all_bookmarks_with_rate_limit(max_pages, &RateLimitConfig::default())
            .await
    }

    /// Fetch all pages of bookmarks with custom rate limit config.
    pub async fn get_all_bookmarks_with_rate_limit(
        &self,
        max_pages: Option<u32>,
        rate_limit: &RateLimitConfig,
    ) -> Result<PaginatedResult<TweetData>> {
        let mut options = PaginationOptions::new();
        if let Some(max) = max_pages {
            options = options.with_max_pages(max);
        } else {
            options = options.fetch_all();
        }
        self.get_bookmarks_paginated_with_rate_limit(&options, rate_limit)
            .await
    }

    /// Fetch all pages of user's own tweets (convenience method).
    pub async fn get_all_user_tweets(
        &self,
        user_id: &str,
        max_pages: Option<u32>,
    ) -> Result<PaginatedResult<TweetData>> {
        self.get_all_user_tweets_with_rate_limit(user_id, max_pages, &RateLimitConfig::default())
            .await
    }

    /// Fetch all pages of user's own tweets with custom rate limit config.
    pub async fn get_all_user_tweets_with_rate_limit(
        &self,
        user_id: &str,
        max_pages: Option<u32>,
        rate_limit: &RateLimitConfig,
    ) -> Result<PaginatedResult<TweetData>> {
        let mut options = PaginationOptions::new();
        if let Some(max) = max_pages {
            options = options.with_max_pages(max);
        } else {
            options = options.fetch_all();
        }
        self.get_user_tweets_paginated_with_rate_limit(user_id, &options, rate_limit)
            .await
    }

    /// Internal helper to fetch all pages with rate limiting.
    async fn fetch_all_pages_with_rate_limit<F, Fut>(
        fetch_fn: F,
        options: &PaginationOptions,
        rate_limit: &RateLimitConfig,
    ) -> Result<PaginatedResult<TweetData>>
    where
        F: Fn(Option<String>) -> Fut,
        Fut: std::future::Future<Output = Result<PaginatedResult<TweetData>>>,
    {
        let mut all_items = Vec::new();
        let mut cursor = options.cursor.clone();
        let mut pages_fetched = 0u32;
        let mut stopped_at_known = false;

        loop {
            // Check max pages
            if let Some(max) = options.max_pages {
                if pages_fetched >= max {
                    break;
                }
            }

            // Rate limit: delay between pages (skip on first page)
            if pages_fetched > 0 && rate_limit.delay_ms > 0 {
                sleep(Duration::from_millis(rate_limit.delay_ms)).await;
            }

            // Fetch with retry on rate limit
            let result = Self::fetch_with_backoff(&fetch_fn, cursor.clone(), rate_limit).await?;

            // Check for stop_at_id
            if let Some(ref stop_id) = options.stop_at_id {
                let mut should_stop = false;
                for item in &result.items {
                    if item.id == *stop_id {
                        stopped_at_known = true;
                        should_stop = true;
                        break;
                    }
                    all_items.push(item.clone());
                }
                if should_stop {
                    break;
                }
            } else {
                all_items.extend(result.items);
            }

            pages_fetched += 1;

            if !result.has_more {
                break;
            }

            cursor = result.next_cursor;
            if cursor.is_none() {
                break;
            }
        }

        let total = all_items.len();
        let mut result = PaginatedResult::new(all_items, cursor);
        result.total_fetched = total;
        if stopped_at_known {
            result = result.with_stopped_at_known();
        }
        Ok(result)
    }

    /// Fetch with exponential backoff on rate limit errors.
    async fn fetch_with_backoff<F, Fut>(
        fetch_fn: &F,
        cursor: Option<String>,
        rate_limit: &RateLimitConfig,
    ) -> Result<PaginatedResult<TweetData>>
    where
        F: Fn(Option<String>) -> Fut,
        Fut: std::future::Future<Output = Result<PaginatedResult<TweetData>>>,
    {
        let mut backoff_ms = rate_limit.initial_backoff_ms;
        let mut retries = 0;

        loop {
            match fetch_fn(cursor.clone()).await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    // Check if it's a rate limit error (429)
                    let is_rate_limit = e.to_string().contains("429")
                        || e.to_string().to_lowercase().contains("rate limit");

                    if is_rate_limit && retries < rate_limit.max_retries {
                        retries += 1;
                        eprintln!(
                            "Rate limited, backing off for {}ms (attempt {}/{})",
                            backoff_ms, retries, rate_limit.max_retries
                        );
                        sleep(Duration::from_millis(backoff_ms)).await;
                        // Exponential backoff with cap
                        backoff_ms = (backoff_ms * 2).min(rate_limit.max_backoff_ms);
                    } else {
                        return Err(e);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bird_core::TweetAuthor;

    fn make_tweet(id: &str) -> TweetData {
        TweetData {
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
            media: None,
            article: None,
            _raw: None,
        }
    }

    #[tokio::test]
    async fn fetch_all_pages_stops_at_known_id() {
        let pages = [
            (
                None,
                PaginatedResult::new(
                    vec![make_tweet("5"), make_tweet("4"), make_tweet("3")],
                    Some("c1".to_string()),
                ),
            ),
            (
                Some("c1".to_string()),
                PaginatedResult::new(vec![make_tweet("2"), make_tweet("1")], None),
            ),
        ];

        let options = PaginationOptions::new()
            .with_stop_at_id("3")
            .with_max_pages(10);
        let rate_limit = RateLimitConfig::none();

        let result = TwitterClient::fetch_all_pages_with_rate_limit(
            |cursor| {
                let page = pages
                    .iter()
                    .find(|(c, _)| c.as_ref() == cursor.as_ref())
                    .map(|(_, p)| p.clone())
                    .unwrap_or_else(PaginatedResult::empty);
                async move { Ok(page) }
            },
            &options,
            &rate_limit,
        )
        .await
        .unwrap();

        assert_eq!(result.items.len(), 2);
        assert!(result.items.iter().all(|t| t.id == "5" || t.id == "4"));
        assert!(result.stopped_at_known);
        assert_eq!(result.total_fetched, 2);
    }

    #[tokio::test]
    async fn fetch_all_pages_respects_initial_cursor() {
        let pages = [
            (
                None,
                PaginatedResult::new(
                    vec![make_tweet("3"), make_tweet("2")],
                    Some("c1".to_string()),
                ),
            ),
            (
                Some("c1".to_string()),
                PaginatedResult::new(vec![make_tweet("1")], None),
            ),
        ];

        let options = PaginationOptions::new().with_cursor("c1").with_max_pages(1);
        let rate_limit = RateLimitConfig::none();

        let result = TwitterClient::fetch_all_pages_with_rate_limit(
            |cursor| {
                let page = pages
                    .iter()
                    .find(|(c, _)| c.as_ref() == cursor.as_ref())
                    .map(|(_, p)| p.clone())
                    .unwrap_or_else(PaginatedResult::empty);
                async move { Ok(page) }
            },
            &options,
            &rate_limit,
        )
        .await
        .unwrap();

        assert_eq!(result.items.len(), 1);
        assert_eq!(result.items[0].id, "1");
    }
}

// Add rand dependency for transaction ID generation
mod rand {
    pub fn random<T: Default + AsMut<[u8]>>() -> T {
        let mut value = T::default();
        getrandom::getrandom(value.as_mut()).expect("Failed to generate random bytes");
        value
    }
}

// Add getrandom as a dependency
extern crate getrandom;
