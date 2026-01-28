//! User tweets fetching with pagination.

use crate::client::TwitterClient;
use crate::constants::{features, Operation, DEFAULT_PAGE_COUNT, TWITTER_API_BASE};
use crate::operations::parse_timeline_entries;
use bird_core::{Error, PaginatedResult, PaginationOptions, Result, TweetData};
use serde_json::json;

impl TwitterClient {
    /// Fetch a user's own tweets with pagination.
    pub(crate) async fn fetch_user_tweets(
        &self,
        user_id: &str,
        options: &PaginationOptions,
    ) -> Result<PaginatedResult<TweetData>> {
        let mut variables = json!({
            "userId": user_id,
            "count": DEFAULT_PAGE_COUNT,
            "includePromotedContent": false,
            "withQuickPromoteEligibilityTweetFields": true,
            "withVoice": true,
            "withV2Timeline": true
        });

        if let Some(ref cursor) = options.cursor {
            variables["cursor"] = json!(cursor);
        }

        let features = features::likes_features(); // User tweets uses same features as likes

        // URL-encode the parameters
        let variables_str = serde_json::to_string(&variables).unwrap();
        let features_str = serde_json::to_string(&features).unwrap();
        let params = format!(
            "variables={}&features={}",
            urlencoding::encode(&variables_str),
            urlencoding::encode(&features_str)
        );

        // Try multiple query IDs in case some are rotated
        let query_ids = [
            Operation::UserTweets.default_query_id(),
            "QJzLa8bJN3yP2ZyMvJJWTw",
        ];

        let headers = self.get_headers();
        let mut last_error = None;

        for query_id in query_ids {
            let url = format!("{}/{}/UserTweets?{}", TWITTER_API_BASE, query_id, params);

            let response = self
                .http_client
                .get(&url)
                .headers(headers.clone())
                .send()
                .await
                .map_err(|e| Error::HttpRequest(e.to_string()))?;

            if response.status() == 429 {
                return Err(Error::RateLimited);
            }

            if response.status() == 404 {
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
                    if message.contains("Query: Unspecified") {
                        last_error = Some(message.to_string());
                        continue;
                    }
                    return Err(Error::ApiError(message.to_string()));
                }
            }

            // Parse the response - user tweets use a different path
            let entries = json
                .pointer("/data/user/result/timeline_v2/timeline/instructions")
                .or_else(|| json.pointer("/data/user/result/timeline/timeline/instructions"))
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

        Err(Error::ApiError(
            last_error.unwrap_or_else(|| "All query IDs failed".to_string()),
        ))
    }
}
