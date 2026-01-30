//! Likes fetching with pagination.

use crate::client::TwitterClient;
use crate::constants::{features, Operation, DEFAULT_PAGE_COUNT, TWITTER_API_BASE};
use crate::operations::parse_timeline_entries;
use bird_core::{Error, PaginatedResult, PaginationOptions, Result, TweetData};
use serde_json::json;

impl TwitterClient {
    /// Fetch user's likes with pagination.
    /// Uses dynamic query ID discovery with auto-refresh on stale IDs.
    pub(crate) async fn fetch_likes(
        &self,
        user_id: &str,
        options: &PaginationOptions,
    ) -> Result<PaginatedResult<TweetData>> {
        // First attempt with current query IDs
        match self.fetch_likes_with_ids(user_id, options).await {
            Ok(result) => Ok(result),
            Err(e) => {
                // If query ID error, try refreshing and retrying
                let should_refresh = matches!(&e, Error::ApiError(msg) if msg.contains("Query: Unspecified") || msg.contains("All query IDs failed"));

                if should_refresh {
                    // Try to refresh query IDs from Twitter's JS bundles
                    if self.query_id_manager.refresh().await.is_ok() {
                        // Retry with fresh IDs
                        return self.fetch_likes_with_ids(user_id, options).await;
                    }
                }
                Err(e)
            }
        }
    }

    /// Internal: fetch likes using current query IDs.
    async fn fetch_likes_with_ids(
        &self,
        user_id: &str,
        options: &PaginationOptions,
    ) -> Result<PaginatedResult<TweetData>> {
        let mut variables = json!({
            "userId": user_id,
            "count": DEFAULT_PAGE_COUNT,
            "includePromotedContent": false,
            "withClientEventToken": false,
            "withBirdwatchNotes": false,
            "withVoice": true,
            "withV2Timeline": true
        });

        if let Some(ref cursor) = options.cursor {
            variables["cursor"] = json!(cursor);
        }

        let features_json = serde_json::to_string(&features::likes_features()).unwrap();
        let variables_json = serde_json::to_string(&variables).unwrap();

        // Get query IDs (cached + fallbacks)
        let query_ids = self.get_query_ids(Operation::Likes.name()).await;
        let headers = self.get_headers();
        let mut last_error = None;
        let mut had_404 = false;

        for query_id in &query_ids {
            let url = format!(
                "{}/{}/{}?variables={}&features={}",
                TWITTER_API_BASE,
                query_id,
                Operation::Likes.name(),
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

            let json: serde_json::Value = response
                .json()
                .await
                .map_err(|e| Error::JsonParse(e.to_string()))?;

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
                    return Err(Error::ApiError(message.to_string()));
                }
            }

            // Parse the response - try multiple paths as API response structure varies
            let instructions = json
                .pointer("/data/user/result/timeline/timeline/instructions")
                .or_else(|| json.pointer("/data/user/result/timeline_v2/timeline/instructions"))
                .and_then(|i| i.as_array());

            let entries = instructions
                .and_then(|instructions| {
                    instructions.iter().find_map(|inst| {
                        let inst_type = inst.get("type").and_then(|t| t.as_str());
                        if inst_type == Some("TimelineAddEntries") {
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
