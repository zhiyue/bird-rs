//! Dynamic query ID discovery and caching.
//!
//! Twitter rotates GraphQL query IDs periodically. This module discovers fresh IDs
//! by scraping Twitter's JS bundles and caches them to disk.

use chrono::{DateTime, Utc};
use regex::Regex;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Default TTL for cached query IDs (24 hours).
const DEFAULT_TTL_SECS: u64 = 24 * 60 * 60;

/// Pages to scrape for JS bundle URLs.
const DISCOVERY_PAGES: &[&str] = &[
    "https://x.com/?lang=en",
    "https://x.com/explore",
    "https://x.com/notifications",
    "https://x.com/settings/profile",
];

/// Operations we need query IDs for.
const TARGET_OPERATIONS: &[&str] = &[
    "Likes",
    "Bookmarks",
    "BookmarkFolderTimeline",
    "TweetDetail",
    "UserTweets",
    "Following",
    "Followers",
    "SearchTimeline",
    "HomeTimeline",
    "CreateTweet",
];

/// Cached query IDs with metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryIdCache {
    /// When the cache was last fetched.
    pub fetched_at: DateTime<Utc>,
    /// TTL in seconds.
    pub ttl_secs: u64,
    /// Operation name -> query ID mapping.
    pub ids: HashMap<String, String>,
}

impl Default for QueryIdCache {
    fn default() -> Self {
        Self {
            fetched_at: DateTime::UNIX_EPOCH,
            ttl_secs: DEFAULT_TTL_SECS,
            ids: HashMap::new(),
        }
    }
}

impl QueryIdCache {
    /// Check if the cache is still fresh.
    pub fn is_fresh(&self) -> bool {
        let age = Utc::now().signed_duration_since(self.fetched_at);
        age.num_seconds() < self.ttl_secs as i64
    }

    /// Get the cache file path (~/.bird/query-ids-cache.json).
    fn cache_path() -> Option<PathBuf> {
        if let Ok(path) = std::env::var("BIRD_QUERY_IDS_CACHE") {
            return Some(PathBuf::from(path));
        }
        dirs::home_dir().map(|d| d.join(".bird").join("query-ids-cache.json"))
    }

    /// Load cache from disk.
    pub fn load_from_disk() -> Option<Self> {
        let path = Self::cache_path()?;
        let contents = std::fs::read_to_string(&path).ok()?;
        serde_json::from_str(&contents).ok()
    }

    /// Save cache to disk.
    pub fn save_to_disk(&self) -> Result<(), std::io::Error> {
        let path = match Self::cache_path() {
            Some(p) => p,
            None => return Ok(()), // Silently skip if no config dir
        };

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let contents = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(&path, contents)
    }
}

/// Manager for query ID discovery and caching.
pub struct QueryIdManager {
    /// HTTP client for fetching.
    client: Client,
    /// In-memory cache.
    cache: Arc<RwLock<QueryIdCache>>,
    /// Static fallback IDs.
    fallbacks: HashMap<String, Vec<String>>,
}

impl QueryIdManager {
    /// Create a new manager with fallback IDs.
    pub fn new(fallbacks: HashMap<String, Vec<String>>) -> Self {
        let cache = QueryIdCache::load_from_disk().unwrap_or_default();

        Self {
            client: Client::builder()
                .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36")
                .build()
                .unwrap(),
            cache: Arc::new(RwLock::new(cache)),
            fallbacks,
        }
    }

    /// Get query ID for an operation, refreshing if needed.
    pub async fn get(&self, operation: &str) -> Option<String> {
        // Try cache first
        {
            let cache = self.cache.read().await;
            if cache.is_fresh() {
                if let Some(id) = cache.ids.get(operation) {
                    return Some(id.clone());
                }
            }
        }

        // Try to refresh (non-blocking for other callers)
        let _ = self.refresh_if_stale().await;

        // Check cache again
        {
            let cache = self.cache.read().await;
            if let Some(id) = cache.ids.get(operation) {
                return Some(id.clone());
            }
        }

        // Fall back to static IDs
        self.fallbacks
            .get(operation)
            .and_then(|ids| ids.first())
            .cloned()
    }

    /// Get all query IDs to try for an operation (cached + fallbacks).
    pub async fn get_all(&self, operation: &str) -> Vec<String> {
        let mut ids = Vec::new();

        // Add cached ID first (if fresh)
        {
            let cache = self.cache.read().await;
            if let Some(id) = cache.ids.get(operation) {
                ids.push(id.clone());
            }
        }

        // Add fallback IDs
        if let Some(fallback_ids) = self.fallbacks.get(operation) {
            for id in fallback_ids {
                if !ids.contains(id) {
                    ids.push(id.clone());
                }
            }
        }

        ids
    }

    /// Refresh cache if stale.
    pub async fn refresh_if_stale(&self) -> Result<bool, QueryIdError> {
        {
            let cache = self.cache.read().await;
            if cache.is_fresh() {
                return Ok(false);
            }
        }
        self.refresh().await?;
        Ok(true)
    }

    /// Force refresh from Twitter's JS bundles.
    pub async fn refresh(&self) -> Result<(), QueryIdError> {
        let ids = self.discover_query_ids().await?;

        let mut cache = self.cache.write().await;
        cache.fetched_at = Utc::now();
        cache.ids = ids;

        // Save to disk (ignore errors)
        let _ = cache.save_to_disk();

        Ok(())
    }

    /// Discover query IDs from Twitter's JS bundles.
    async fn discover_query_ids(&self) -> Result<HashMap<String, String>, QueryIdError> {
        // Find bundle URLs from discovery pages
        let bundle_urls = self.find_bundle_urls().await?;

        if bundle_urls.is_empty() {
            return Err(QueryIdError::NoBundlesFound);
        }

        // Fetch bundles and extract IDs
        let mut all_ids: HashMap<String, String> = HashMap::new();

        // Fetch bundles in batches of 6 (like steipete/bird)
        for chunk in bundle_urls.chunks(6) {
            let futures: Vec<_> = chunk
                .iter()
                .map(|url| self.extract_ids_from_bundle(url))
                .collect();

            let results = futures::future::join_all(futures).await;

            for ids in results.into_iter().flatten() {
                for (op, id) in ids {
                    // Only keep first found ID per operation
                    all_ids.entry(op).or_insert(id);
                }
            }

            // Early exit if we have all target operations
            if TARGET_OPERATIONS.iter().all(|op| all_ids.contains_key(*op)) {
                break;
            }
        }

        Ok(all_ids)
    }

    /// Find JS bundle URLs from discovery pages.
    async fn find_bundle_urls(&self) -> Result<Vec<String>, QueryIdError> {
        let bundle_pattern = Regex::new(
            r#"https://abs\.twimg\.com/responsive-web/client-web(?:-legacy)?/[A-Za-z0-9._-]+\.js"#,
        )
        .unwrap();

        let mut bundle_urls: Vec<String> = Vec::new();

        for page_url in DISCOVERY_PAGES {
            let html = match self.client.get(*page_url).send().await {
                Ok(resp) => resp.text().await.unwrap_or_default(),
                Err(_) => continue,
            };

            for cap in bundle_pattern.captures_iter(&html) {
                let url = cap.get(0).unwrap().as_str().to_string();
                if !bundle_urls.contains(&url) {
                    bundle_urls.push(url);
                }
            }

            // Limit to reasonable number of bundles
            if bundle_urls.len() >= 50 {
                break;
            }
        }

        Ok(bundle_urls)
    }

    /// Extract query IDs from a single JS bundle.
    async fn extract_ids_from_bundle(
        &self,
        url: &str,
    ) -> Result<HashMap<String, String>, QueryIdError> {
        let js = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| QueryIdError::FetchError(e.to_string()))?
            .text()
            .await
            .map_err(|e| QueryIdError::FetchError(e.to_string()))?;

        Ok(extract_query_ids_from_js(&js))
    }
}

/// Extract query IDs from minified JavaScript.
fn extract_query_ids_from_js(js: &str) -> HashMap<String, String> {
    let mut ids: HashMap<String, String> = HashMap::new();

    // Pattern 1: e.exports={queryId:"...",operationName:"..."}
    let pattern1 = Regex::new(
        r#"e\.exports=\{queryId\s*:\s*["']([^"']+)["']\s*,\s*operationName\s*:\s*["']([^"']+)["']"#,
    )
    .unwrap();

    // Pattern 2: e.exports={operationName:"...",queryId:"..."}
    let pattern2 = Regex::new(
        r#"e\.exports=\{operationName\s*:\s*["']([^"']+)["']\s*,\s*queryId\s*:\s*["']([^"']+)["']"#,
    )
    .unwrap();

    // Pattern 3: Loose match - queryId first (with bounded lookahead)
    let pattern3 =
        Regex::new(r#"queryId\s*[:=]\s*["']([A-Za-z0-9_-]+)["'].{0,200}?operationName\s*[:=]\s*["']([A-Za-z0-9_]+)["']"#)
            .unwrap();

    // Pattern 4: Loose match - operationName first (with bounded lookahead)
    let pattern4 =
        Regex::new(r#"operationName\s*[:=]\s*["']([A-Za-z0-9_]+)["'].{0,200}?queryId\s*[:=]\s*["']([A-Za-z0-9_-]+)["']"#)
            .unwrap();

    // Pattern 1: queryId first
    for cap in pattern1.captures_iter(js) {
        let query_id = cap.get(1).unwrap().as_str().to_string();
        let operation = cap.get(2).unwrap().as_str().to_string();
        if is_target_operation(&operation) && is_valid_query_id(&query_id) {
            ids.entry(operation).or_insert(query_id);
        }
    }

    // Pattern 2: operationName first
    for cap in pattern2.captures_iter(js) {
        let operation = cap.get(1).unwrap().as_str().to_string();
        let query_id = cap.get(2).unwrap().as_str().to_string();
        if is_target_operation(&operation) && is_valid_query_id(&query_id) {
            ids.entry(operation).or_insert(query_id);
        }
    }

    // Pattern 3: Loose match - queryId first
    for cap in pattern3.captures_iter(js) {
        let query_id = cap.get(1).unwrap().as_str().to_string();
        let operation = cap.get(2).unwrap().as_str().to_string();
        if is_target_operation(&operation) && is_valid_query_id(&query_id) {
            ids.entry(operation).or_insert(query_id);
        }
    }

    // Pattern 4: Loose match - operationName first
    for cap in pattern4.captures_iter(js) {
        let operation = cap.get(1).unwrap().as_str().to_string();
        let query_id = cap.get(2).unwrap().as_str().to_string();
        if is_target_operation(&operation) && is_valid_query_id(&query_id) {
            ids.entry(operation).or_insert(query_id);
        }
    }

    ids
}

/// Check if an operation is one we care about.
fn is_target_operation(op: &str) -> bool {
    TARGET_OPERATIONS.contains(&op)
}

/// Validate query ID format (base64-like alphanumeric).
fn is_valid_query_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 50
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Errors that can occur during query ID operations.
#[derive(Debug)]
pub enum QueryIdError {
    /// No JS bundles found on discovery pages.
    NoBundlesFound,
    /// Failed to fetch a resource.
    FetchError(String),
}

impl std::fmt::Display for QueryIdError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QueryIdError::NoBundlesFound => write!(f, "No JS bundles found on Twitter"),
            QueryIdError::FetchError(e) => write!(f, "Failed to fetch: {}", e),
        }
    }
}

impl std::error::Error for QueryIdError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_query_ids_pattern1() {
        let js = r#"e.exports={queryId:"ETJflBunfqNa1uE1mBPCaw",operationName:"Likes",foo:"bar"}"#;
        let ids = extract_query_ids_from_js(js);
        assert_eq!(
            ids.get("Likes"),
            Some(&"ETJflBunfqNa1uE1mBPCaw".to_string())
        );
    }

    #[test]
    fn test_extract_query_ids_pattern2() {
        let js = r#"e.exports={operationName:"Bookmarks",queryId:"RV1g3b8n_SGOHwkqKYSCFw"}"#;
        let ids = extract_query_ids_from_js(js);
        assert_eq!(
            ids.get("Bookmarks"),
            Some(&"RV1g3b8n_SGOHwkqKYSCFw".to_string())
        );
    }

    #[test]
    fn test_extract_query_ids_pattern3() {
        let js = r#"something,queryId:"abc123",other:"x",operationName:"TweetDetail""#;
        let ids = extract_query_ids_from_js(js);
        assert_eq!(ids.get("TweetDetail"), Some(&"abc123".to_string()));
    }

    #[test]
    fn test_is_valid_query_id() {
        assert!(is_valid_query_id("ETJflBunfqNa1uE1mBPCaw"));
        assert!(is_valid_query_id("abc_123-XYZ"));
        assert!(!is_valid_query_id(""));
        assert!(!is_valid_query_id("has spaces"));
        assert!(!is_valid_query_id("has.dots"));
    }

    #[test]
    fn test_cache_freshness() {
        let mut cache = QueryIdCache::default();
        assert!(!cache.is_fresh()); // Epoch time is stale

        cache.fetched_at = Utc::now();
        assert!(cache.is_fresh()); // Just fetched is fresh
    }
}
