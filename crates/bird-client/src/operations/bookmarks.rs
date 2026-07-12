//! Bookmarks fetching with pagination.

use crate::client::TwitterClient;
use crate::constants::{features, Operation, TWITTER_API_BASE};
use crate::operations::parse_timeline_entries;
use bird_core::{Error, PaginatedResult, PaginationOptions, Result, TweetData};
use serde_json::json;

const BOOKMARK_PAGE_COUNT: u32 = 100;

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
                    if let Err(refresh_err) = self.query_id_manager.refresh().await {
                        return Err(Error::ApiError(format!(
                            "{}; failed to refresh bookmark query IDs: {}",
                            e, refresh_err
                        )));
                    }

                    match self.fetch_bookmarks_with_ids(options).await {
                        Ok(result) => return Ok(result),
                        Err(retry_err) => {
                            return Err(classify_bookmark_retry_error(
                                retry_err,
                                options.cursor.is_some(),
                            ));
                        }
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
            "count": BOOKMARK_PAGE_COUNT,
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
        let mut had_query_unspecified = false;

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
                        had_query_unspecified = true;
                        last_error = Some(message.to_string());
                        continue;
                    }
                    // Transient server errors: try next query ID instead of failing
                    if message.contains("Internal server error") || message.contains("server error")
                    {
                        last_error = Some(message.to_string());
                        continue;
                    }
                    return Err(Error::ApiError(message.to_string()));
                }
            }

            return parse_bookmark_timeline(&json, self.quote_depth);
        }

        // If we had 404s, include that in the error message
        let error_msg = final_bookmark_error(had_query_unspecified, had_404, last_error);

        Err(Error::ApiError(error_msg))
    }
}

fn final_bookmark_error(
    had_query_unspecified: bool,
    had_404: bool,
    last_error: Option<String>,
) -> String {
    if had_query_unspecified {
        "All query IDs failed: Query: Unspecified".to_string()
    } else if had_404 {
        format!(
            "All query IDs failed (had 404s): {}",
            last_error.unwrap_or_default()
        )
    } else {
        last_error.unwrap_or_else(|| "All query IDs failed".to_string())
    }
}

fn classify_bookmark_retry_error(error: Error, has_cursor: bool) -> Error {
    if has_cursor
        && matches!(&error, Error::ApiError(message) if message.contains("Query: Unspecified"))
    {
        Error::BookmarkHistoryLimitReached
    } else {
        error
    }
}

fn parse_bookmark_timeline(
    json: &serde_json::Value,
    quote_depth: u32,
) -> Result<PaginatedResult<TweetData>> {
    let instructions = json
        .pointer("/data/bookmark_timeline_v2/timeline/instructions")
        .or_else(|| json.pointer("/data/bookmark_timeline/timeline/instructions"))
        .and_then(|value| value.as_array())
        .ok_or_else(|| Error::JsonParse("missing bookmark timeline instructions".to_string()))?;

    let entries = instructions.iter().find_map(|instruction| {
        if instruction.get("type").and_then(|value| value.as_str()) == Some("TimelineAddEntries") {
            instruction
                .get("entries")
                .and_then(|value| value.as_array())
        } else {
            None
        }
    });

    let Some(entries) = entries else {
        let terminated = instructions.iter().any(|instruction| {
            instruction.get("type").and_then(|value| value.as_str())
                == Some("TimelineTerminateTimeline")
        });
        return if terminated {
            Ok(PaginatedResult::empty())
        } else {
            Err(Error::JsonParse(
                "missing bookmark timeline entries".to_string(),
            ))
        };
    };

    let (tweets, next_cursor) = parse_timeline_entries(entries, quote_depth);
    Ok(PaginatedResult::new(tweets, next_cursor))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_bookmark_timeline_accepts_empty_entries() {
        let json = json!({
            "data": {
                "bookmark_timeline_v2": {
                    "timeline": {
                        "instructions": [{
                            "type": "TimelineAddEntries",
                            "entries": []
                        }]
                    }
                }
            }
        });

        let result = parse_bookmark_timeline(&json, 0).unwrap();

        assert!(result.items.is_empty());
        assert!(!result.has_more);
    }

    #[test]
    fn parse_bookmark_timeline_rejects_missing_timeline() {
        let error = parse_bookmark_timeline(&json!({"data": {}}), 0).unwrap_err();

        assert!(matches!(error, Error::JsonParse(_)));
    }

    #[test]
    fn parse_bookmark_timeline_accepts_explicit_termination() {
        let json = json!({
            "data": {
                "bookmark_timeline_v2": {
                    "timeline": {
                        "instructions": [{
                            "type": "TimelineTerminateTimeline"
                        }]
                    }
                }
            }
        });

        let result = parse_bookmark_timeline(&json, 0).unwrap();

        assert!(result.items.is_empty());
        assert!(!result.has_more);
    }

    #[test]
    fn cursor_rejection_after_refresh_is_a_history_limit() {
        let error =
            classify_bookmark_retry_error(Error::ApiError("Query: Unspecified".to_string()), true);

        assert!(matches!(error, Error::BookmarkHistoryLimitReached));
    }

    #[test]
    fn first_page_query_failure_remains_an_api_error() {
        let error =
            classify_bookmark_retry_error(Error::ApiError("Query: Unspecified".to_string()), false);

        assert!(matches!(error, Error::ApiError(_)));
    }

    #[test]
    fn query_limit_signal_is_not_overwritten_by_a_later_404() {
        let message = final_bookmark_error(true, true, Some("HTTP 404".to_string()));

        assert!(message.contains("Query: Unspecified"));
    }
}
