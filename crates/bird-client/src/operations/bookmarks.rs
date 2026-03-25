//! Bookmarks fetching with pagination.

use crate::client::TwitterClient;
use crate::constants::{features, Operation, DEFAULT_PAGE_COUNT, TWITTER_API_BASE};
use crate::operations::parse_timeline_entries;
use bird_core::{Error, PaginatedResult, PaginationOptions, Result, TweetData};
use serde_json::json;

impl TwitterClient {
    /// Fetch user's bookmarks with pagination.
    /// Uses dynamic query ID discovery with auto-refresh on stale IDs.
    pub(crate) async fn fetch_bookmarks(
        &self,
        options: &PaginationOptions,
    ) -> Result<PaginatedResult<TweetData>> {
        // First attempt with current query IDs
        match self.fetch_bookmarks_with_ids(options).await {
            Ok(result) => Ok(result),
            Err(e) => {
                let is_query_error = matches!(&e, Error::ApiError(msg) if msg.contains("Query: Unspecified") || msg.contains("All query IDs failed"));

                if is_query_error {
                    // Try refreshing query IDs from Twitter's JS bundles
                    if self.query_id_manager.refresh().await.is_ok() {
                        match self.fetch_bookmarks_with_ids(options).await {
                            Ok(result) => return Ok(result),
                            Err(retry_err) => {
                                // If still "Query: Unspecified" after refresh, treat as end of results
                                // (Twitter API has a ~3000 bookmark pagination depth limit)
                                if matches!(&retry_err, Error::ApiError(msg) if msg.contains("Query: Unspecified")) {
                                    eprintln!("Bookmark pagination limit reached (Query: Unspecified after refresh)");
                                    return Ok(PaginatedResult::empty());
                                }
                                return Err(retry_err);
                            }
                        }
                    }
                    // If refresh failed, also treat as end of results when paginating
                    if options.cursor.is_some() {
                        eprintln!("Bookmark pagination limit reached (Query: Unspecified)");
                        return Ok(PaginatedResult::empty());
                    }
                }
                Err(e)
            }
        }
    }

    /// Internal: fetch bookmarks using current query IDs.
    async fn fetch_bookmarks_with_ids(
        &self,
        options: &PaginationOptions,
    ) -> Result<PaginatedResult<TweetData>> {
        let mut variables = json!({
            "count": DEFAULT_PAGE_COUNT,
            "includePromotedContent": false,
            "cursor": ""
        });

        if let Some(ref cursor) = options.cursor {
            variables["cursor"] = json!(cursor);
        }

        let features_json = serde_json::to_string(&features::bookmarks_features()).unwrap();
        let variables_json = serde_json::to_string(&variables).unwrap();

        // Get query IDs (cached + fallbacks)
        let query_ids = self.get_query_ids(Operation::Bookmarks.name()).await;
        let headers = self.get_headers();
        let mut last_error = None;
        let mut had_404 = false;

        for query_id in &query_ids {
            let url = format!(
                "{}/{}/{}?variables={}&features={}",
                TWITTER_API_BASE,
                query_id,
                Operation::Bookmarks.name(),
                urlencoding::encode(&variables_json),
                urlencoding::encode(&features_json)
            );

            let response = self
                .http_client
                .get(&url)
                .headers(headers.clone())
                .send()
                .await
                .map_err(|e| Error::HttpRequest(e.to_string()))?;

            if response.status() == 429 {
                let reset_at = response
                    .headers()
                    .get("x-rate-limit-reset")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse::<i64>().ok());
                return Err(Error::RateLimited(reset_at));
            }

            if response.status() == 404 {
                had_404 = true;
                last_error = Some("HTTP 404".to_string());
                continue;
            }

            if !response.status().is_success() {
                last_error = Some(format!("HTTP {}", response.status()));
                continue;
            }

            // Read as text first for better error diagnostics
            let text = response
                .text()
                .await
                .map_err(|e| Error::JsonParse(format!("failed to read response body: {}", e)))?;

            let json: serde_json::Value = serde_json::from_str(&text).map_err(|e| {
                let preview = if text.len() > 200 {
                    &text[..200]
                } else {
                    &text
                };
                Error::JsonParse(format!("{} (response preview: {})", e, preview))
            })?;

            // Check for API errors
            if let Some(errors) = json.get("errors").and_then(|e| e.as_array()) {
                if let Some(first_error) = errors.first() {
                    let message = first_error
                        .get("message")
                        .and_then(|m| m.as_str())
                        .unwrap_or("Unknown error");
                    // If it's a query-specific error, try next query ID
                    if message.contains("Query: Unspecified") {
                        last_error = Some(message.to_string());
                        continue;
                    }
                    // Transient server errors: try next query ID instead of failing
                    if message.contains("Internal server error")
                        || message.contains("server error")
                    {
                        last_error = Some(message.to_string());
                        continue;
                    }
                    return Err(Error::ApiError(message.to_string()));
                }
            }

            // Parse the response - try both v2 and v1 paths
            let entries = json
                .pointer("/data/bookmark_timeline_v2/timeline/instructions")
                .or_else(|| json.pointer("/data/bookmark_timeline/timeline/instructions"))
                .and_then(|instructions| instructions.as_array())
                .and_then(|instructions| {
                    instructions.iter().find_map(|inst| {
                        if inst.get("type").and_then(|t| t.as_str()) == Some("TimelineAddEntries") {
                            inst.get("entries").and_then(|e| e.as_array())
                        } else {
                            None
                        }
                    })
                })
                .map(|e| e.to_vec())
                .unwrap_or_default();

            let (tweets, next_cursor) = parse_timeline_entries(&entries, self.quote_depth);

            return Ok(PaginatedResult::new(tweets, next_cursor));
        }

        // If we had 404s, include that in the error message
        let error_msg = if had_404 {
            format!(
                "All query IDs failed (had 404s): {}",
                last_error.unwrap_or_default()
            )
        } else {
            last_error.unwrap_or_else(|| "All query IDs failed".to_string())
        };

        Err(Error::ApiError(error_msg))
    }
}
