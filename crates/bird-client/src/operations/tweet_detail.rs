//! Tweet detail fetching implementation.

use crate::client::TwitterClient;
use crate::constants::Operation;
use bird_core::{Error, MediaType, Result, TweetArticle, TweetAuthor, TweetData, TweetMedia};
use serde_json::{json, Value};

impl TwitterClient {
    /// Fetch a tweet by ID.
    pub(crate) async fn get_tweet_detail(&self, tweet_id: &str) -> Result<TweetData> {
        let variables = json!({
            "tweetId": tweet_id,
            "withCommunity": false,
            "includePromotedContent": false,
            "withVoice": false
        });

        let url = self.build_graphql_url(Operation::TweetDetail, &variables);
        let headers = self.get_headers();

        let response = self.http_client.get(&url).headers(headers).send().await
            .map_err(|e| Error::HttpRequest(e.to_string()))?;

        if response.status() == 404 {
            return Err(Error::TweetNotFound(tweet_id.to_string()));
        }

        if response.status() == 429 {
            return Err(Error::RateLimited);
        }

        if !response.status().is_success() {
            return Err(Error::ApiError(format!("HTTP {}", response.status())));
        }

        let json: Value = response.json().await
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

        // Parse the tweet from the response
        parse_tweet_from_response(&json, tweet_id, self.quote_depth)
    }
}

/// Parse a tweet from the GraphQL response.
fn parse_tweet_from_response(json: &Value, tweet_id: &str, quote_depth: u32) -> Result<TweetData> {
    // Navigate to the tweet result
    let tweet_result = json
        .pointer("/data/tweetResult/result")
        .or_else(|| {
            // Alternative path for some responses
            json.pointer("/data/tweet/result")
        })
        .ok_or_else(|| Error::TweetNotFound(tweet_id.to_string()))?;

    parse_tweet_result(tweet_result, quote_depth, 0)
}

/// Parse a tweet result object.
pub(crate) fn parse_tweet_result(
    result: &Value,
    max_quote_depth: u32,
    current_depth: u32,
) -> Result<TweetData> {
    // Handle tombstone (deleted/unavailable tweets)
    if result.get("__typename").and_then(|t| t.as_str()) == Some("TweetTombstone") {
        return Err(Error::TweetNotFound("Tweet is unavailable".to_string()));
    }

    // Handle TweetWithVisibilityResults wrapper
    let tweet_result = if result.get("__typename").and_then(|t| t.as_str())
        == Some("TweetWithVisibilityResults")
    {
        result.get("tweet").unwrap_or(result)
    } else {
        result
    };

    // Get tweet ID
    let id = tweet_result
        .get("rest_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| Error::ApiError("Missing tweet ID".to_string()))?
        .to_string();

    // Get legacy data
    let legacy = tweet_result.get("legacy");

    // Get tweet text
    let text = get_tweet_text(tweet_result, legacy);

    // Get author info
    let author = parse_author(tweet_result)?;

    // Get author ID
    let author_id = tweet_result
        .pointer("/core/user_results/result/rest_id")
        .or_else(|| tweet_result.pointer("/core/user_results/result/id"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // Get timestamps and counts from legacy
    let (
        created_at,
        reply_count,
        retweet_count,
        like_count,
        conversation_id,
        in_reply_to_status_id,
    ) = if let Some(leg) = legacy {
        (
            leg.get("created_at")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            leg.get("reply_count").and_then(|v| v.as_u64()),
            leg.get("retweet_count").and_then(|v| v.as_u64()),
            leg.get("favorite_count").and_then(|v| v.as_u64()),
            leg.get("conversation_id_str")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            leg.get("in_reply_to_status_id_str")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string()),
        )
    } else {
        (None, None, None, None, None, None)
    };

    // Parse media
    let media = parse_media(legacy);

    // Parse article
    let article = parse_article(tweet_result);

    // Parse quoted tweet (with depth limit)
    let quoted_tweet = if current_depth < max_quote_depth {
        tweet_result
            .pointer("/quoted_status_result/result")
            .and_then(|q| parse_tweet_result(q, max_quote_depth, current_depth + 1).ok())
            .map(Box::new)
    } else {
        None
    };

    Ok(TweetData {
        id,
        text,
        author,
        author_id,
        created_at,
        reply_count,
        retweet_count,
        like_count,
        conversation_id,
        in_reply_to_status_id,
        quoted_tweet,
        media,
        article,
        _raw: None,
    })
}

/// Extract tweet text, handling note tweets and regular tweets.
fn get_tweet_text(tweet_result: &Value, legacy: Option<&Value>) -> String {
    // Try note_tweet first (for long tweets)
    if let Some(note_text) = tweet_result
        .pointer("/note_tweet/note_tweet_results/result/text")
        .or_else(|| tweet_result.pointer("/note_tweet/note_tweet_results/result/richtext/text"))
        .or_else(|| tweet_result.pointer("/note_tweet/note_tweet_results/result/rich_text/text"))
        .and_then(|v| v.as_str())
    {
        return note_text.to_string();
    }

    // Fall back to legacy full_text
    legacy
        .and_then(|l| l.get("full_text"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

/// Parse author information from tweet result.
fn parse_author(tweet_result: &Value) -> Result<TweetAuthor> {
    let user_result = tweet_result
        .pointer("/core/user_results/result")
        .ok_or_else(|| Error::ApiError("Missing author info".to_string()))?;

    let username = user_result
        .pointer("/legacy/screen_name")
        .or_else(|| user_result.pointer("/core/screen_name"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    let name = user_result
        .pointer("/legacy/name")
        .or_else(|| user_result.pointer("/core/name"))
        .and_then(|v| v.as_str())
        .unwrap_or(&username)
        .to_string();

    Ok(TweetAuthor { username, name })
}

/// Parse media attachments from legacy data.
fn parse_media(legacy: Option<&Value>) -> Option<Vec<TweetMedia>> {
    let legacy = legacy?;

    // Try extended_entities first, then entities
    let media_array = legacy
        .pointer("/extended_entities/media")
        .or_else(|| legacy.pointer("/entities/media"))
        .and_then(|v| v.as_array())?;

    if media_array.is_empty() {
        return None;
    }

    let media: Vec<TweetMedia> = media_array
        .iter()
        .filter_map(|m| {
            let media_type = match m.get("type").and_then(|t| t.as_str())? {
                "photo" => MediaType::Photo,
                "video" => MediaType::Video,
                "animated_gif" => MediaType::AnimatedGif,
                _ => return None,
            };

            let url = m
                .get("media_url_https")
                .and_then(|u| u.as_str())?
                .to_string();

            let (width, height) = m
                .pointer("/sizes/large")
                .or_else(|| m.pointer("/sizes/medium"))
                .map(|s| {
                    (
                        s.get("w").and_then(|v| v.as_u64()).map(|n| n as u32),
                        s.get("h").and_then(|v| v.as_u64()).map(|n| n as u32),
                    )
                })
                .unwrap_or((None, None));

            // Get video URL for video/gif types
            let video_url = if matches!(media_type, MediaType::Video | MediaType::AnimatedGif) {
                m.pointer("/video_info/variants")
                    .and_then(|v| v.as_array())
                    .and_then(|variants| {
                        variants
                            .iter()
                            .filter(|v| {
                                v.get("content_type")
                                    .and_then(|c| c.as_str())
                                    .map(|c| c.contains("mp4"))
                                    .unwrap_or(false)
                            })
                            .max_by_key(|v| v.get("bitrate").and_then(|b| b.as_u64()).unwrap_or(0))
                            .and_then(|v| v.get("url").and_then(|u| u.as_str()))
                            .map(|s| s.to_string())
                    })
            } else {
                None
            };

            let duration_ms = m
                .pointer("/video_info/duration_millis")
                .and_then(|d| d.as_u64());

            Some(TweetMedia {
                media_type,
                url: url.clone(),
                preview_url: Some(url),
                width,
                height,
                video_url,
                duration_ms,
            })
        })
        .collect();

    if media.is_empty() {
        None
    } else {
        Some(media)
    }
}

/// Parse article metadata from tweet result.
fn parse_article(tweet_result: &Value) -> Option<TweetArticle> {
    let article = tweet_result.get("article")?;

    let title = article
        .get("title")
        .or_else(|| article.pointer("/article_results/result/title"))
        .and_then(|t| t.as_str())?
        .to_string();

    let preview_text = article
        .get("preview_text")
        .or_else(|| article.pointer("/article_results/result/preview_text"))
        .and_then(|t| t.as_str())
        .map(|s| s.to_string());

    Some(TweetArticle {
        title,
        preview_text,
    })
}

/// Parse timeline entries to extract tweets and cursor.
pub(crate) fn parse_timeline_entries(
    entries: &[Value],
    quote_depth: u32,
) -> (Vec<TweetData>, Option<String>) {
    let mut tweets = Vec::new();
    let mut next_cursor = None;

    for entry in entries {
        let entry_type = entry
            .get("entryId")
            .and_then(|e| e.as_str())
            .unwrap_or("");

        // Handle cursor entries
        if entry_type.starts_with("cursor-bottom") {
            next_cursor = entry
                .pointer("/content/value")
                .or_else(|| entry.pointer("/content/itemContent/value"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            continue;
        }

        // Skip non-tweet entries
        if !entry_type.starts_with("tweet-") && !entry_type.starts_with("list-tweet-") {
            continue;
        }

        // Extract tweet from entry
        let tweet_result = entry
            .pointer("/content/itemContent/tweet_results/result")
            .or_else(|| entry.pointer("/content/content/tweetResult/result"));

        if let Some(result) = tweet_result {
            if let Ok(tweet) = parse_tweet_result(result, quote_depth, 0) {
                tweets.push(tweet);
            }
        }
    }

    (tweets, next_cursor)
}
