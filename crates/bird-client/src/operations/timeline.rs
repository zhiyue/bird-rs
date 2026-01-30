//! Timeline fetching with pagination.

use crate::client::TwitterClient;
use crate::constants::{Operation, DEFAULT_PAGE_COUNT};
use crate::operations::parse_timeline_entries;
use bird_core::{Error, PaginatedResult, PaginationOptions, Result, TweetData};
use serde_json::json;

impl TwitterClient {
    /// Fetch home timeline with pagination.
    pub(crate) async fn fetch_timeline(
        &self,
        options: &PaginationOptions,
    ) -> Result<PaginatedResult<TweetData>> {
        let mut variables = json!({
            "count": DEFAULT_PAGE_COUNT,
            "includePromotedContent": false,
            "latestControlAvailable": true,
            "requestContext": "launch",
            "withCommunity": true
        });

        if let Some(ref cursor) = options.cursor {
            variables["cursor"] = json!(cursor);
        }

        let url = self.build_graphql_url(Operation::HomeLatestTimeline, &variables);
        let headers = self.get_headers();

        let response = self
            .http_client
            .get(&url)
            .headers(headers)
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

        if !response.status().is_success() {
            return Err(Error::ApiError(format!("HTTP {}", response.status())));
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
                return Err(Error::ApiError(message.to_string()));
            }
        }

        // Parse the response
        let entries = json
            .pointer("/data/home/home_timeline_urt/instructions")
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

        Ok(PaginatedResult::new(tweets, next_cursor))
    }
}
