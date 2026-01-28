//! Likes fetching with pagination.

use crate::client::TwitterClient;
use crate::constants::{features, Operation, DEFAULT_PAGE_COUNT, TWITTER_API_BASE};
use crate::operations::parse_timeline_entries;
use bird_core::{Error, PaginatedResult, PaginationOptions, Result, TweetData};
use serde_json::json;

impl TwitterClient {
    /// Fetch user's likes with pagination.
    pub(crate) async fn fetch_likes(
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

        // Use likes-specific features
        let query_id = Operation::Likes.default_query_id();
        let features_json = serde_json::to_string(&features::likes_features()).unwrap();
        let variables_json = serde_json::to_string(&variables).unwrap();

        let url = format!(
            "{}/{}/{}?variables={}&features={}",
            TWITTER_API_BASE,
            query_id,
            Operation::Likes.name(),
            urlencoding::encode(&variables_json),
            urlencoding::encode(&features_json)
        );

        let headers = self.get_headers();

        let response = self
            .http_client
            .get(&url)
            .headers(headers)
            .send()
            .await
            .map_err(|e| Error::HttpRequest(e.to_string()))?;

        if response.status() == 429 {
            return Err(Error::RateLimited);
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

        Ok(PaginatedResult::new(tweets, next_cursor))
    }
}
