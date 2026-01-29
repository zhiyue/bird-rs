//! Headline generation for long tweets.
//!
//! This module provides functionality to generate concise headlines for long tweets
//! using an LLM. Headlines are cached in the database for reuse.

use super::llm::{LlmProvider, LlmRequest, LlmResult};
use bird_client::TweetData;
use std::collections::HashMap;

/// Character threshold for tweets that need headlines.
/// Tweets with text longer than this threshold will have headlines generated.
pub const HEADLINE_THRESHOLD: usize = 200;

/// Check if a tweet needs a headline generated.
pub fn needs_headline(tweet: &TweetData) -> bool {
    tweet.headline.is_none() && tweet.text.chars().count() > HEADLINE_THRESHOLD
}

/// Filter tweets that need headlines generated.
pub fn filter_tweets_needing_headlines(tweets: &[TweetData]) -> Vec<&TweetData> {
    tweets.iter().filter(|t| needs_headline(t)).collect()
}

/// Collect all tweets (including quoted tweets) that need headlines.
pub fn collect_all_tweets_needing_headlines(tweets: &[TweetData]) -> Vec<&TweetData> {
    let mut result = Vec::new();

    for tweet in tweets {
        if needs_headline(tweet) {
            result.push(tweet);
        }

        // Also check quoted tweets
        if let Some(ref quoted) = tweet.quoted_tweet {
            if needs_headline(quoted) {
                result.push(quoted.as_ref());
            }
        }
    }

    result
}

/// Generate headlines for multiple tweets in a single batch LLM call.
/// Returns a map of tweet_id -> headline.
pub async fn generate_headlines(
    tweets: &[&TweetData],
    llm: &dyn LlmProvider,
) -> LlmResult<HashMap<String, String>> {
    if tweets.is_empty() {
        return Ok(HashMap::new());
    }

    let system_prompt = build_headline_system_prompt();
    let user_prompt = build_headline_user_prompt(tweets);

    let request = LlmRequest {
        system: system_prompt,
        user: user_prompt,
        max_tokens: 2048,
        temperature: 0.3, // Lower temperature for more consistent headlines
    };

    let response = llm.complete(request).await?;
    parse_headline_response(&response.content, tweets)
}

/// Build the system prompt for headline generation.
fn build_headline_system_prompt() -> String {
    r#"You are a headline generator. Your task is to create concise, informative headlines for tweets.

Guidelines:
- Headlines should be 10-20 words, capturing the main point or value of the tweet
- Focus on what makes the tweet interesting or useful
- Use clear, descriptive language
- Don't include hashtags or @ mentions in headlines
- Don't start with "Tweet about" or similar meta-descriptions

Output your response as valid JSON with this structure:
{
  "headlines": {
    "TWEET_ID_1": "Headline for first tweet",
    "TWEET_ID_2": "Headline for second tweet"
  }
}

Important:
- Include a headline for EVERY tweet ID provided
- Ensure valid JSON output (no trailing commas, proper escaping)
- Use the exact tweet IDs provided"#.to_string()
}

/// Build the user prompt with tweet content for headline generation.
fn build_headline_user_prompt(tweets: &[&TweetData]) -> String {
    let mut prompt = format!(
        "Generate headlines for the following {} tweets:\n\n",
        tweets.len()
    );

    for tweet in tweets {
        prompt.push_str(&format!(
            "---\nTweet ID: {}\n@{}: {}\n",
            tweet.id, tweet.author.username, tweet.text
        ));
    }

    prompt.push_str("\n---\nProvide headlines as JSON:");
    prompt
}

/// Parse the LLM response into a map of tweet_id -> headline.
fn parse_headline_response(
    response: &str,
    tweets: &[&TweetData],
) -> LlmResult<HashMap<String, String>> {
    // Try to extract JSON from the response
    let json_str = extract_json(response)?;

    // Parse the JSON
    let parsed: serde_json::Value = serde_json::from_str(&json_str)
        .map_err(|e| super::llm::LlmError::ParseError(format!("Failed to parse JSON: {}", e)))?;

    let mut headlines = HashMap::new();

    // Extract headlines from the response
    if let Some(headlines_obj) = parsed.get("headlines").and_then(|v| v.as_object()) {
        for (id, headline) in headlines_obj {
            if let Some(headline_str) = headline.as_str() {
                headlines.insert(id.clone(), headline_str.to_string());
            }
        }
    }

    // Validate that we got headlines for all tweets
    for tweet in tweets {
        if !headlines.contains_key(&tweet.id) {
            eprintln!("  Warning: No headline generated for tweet {}", tweet.id);
        }
    }

    Ok(headlines)
}

/// Extract JSON from response text (handling markdown code blocks).
fn extract_json(response: &str) -> LlmResult<String> {
    let trimmed = response.trim();

    // Check for markdown code block
    if trimmed.starts_with("```") {
        let lines: Vec<&str> = trimmed.lines().collect();
        if lines.len() >= 2 {
            let start = 1;
            let end = if lines.last().map(|l| l.trim()) == Some("```") {
                lines.len() - 1
            } else {
                lines.len()
            };
            return Ok(lines[start..end].join("\n"));
        }
    }

    // Check if it starts with { directly
    if trimmed.starts_with('{') {
        return Ok(trimmed.to_string());
    }

    // Try to find JSON object in the response
    if let Some(start) = trimmed.find('{') {
        if let Some(end) = trimmed.rfind('}') {
            if end > start {
                return Ok(trimmed[start..=end].to_string());
            }
        }
    }

    Err(super::llm::LlmError::ParseError(
        "Could not find valid JSON in headline response".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bird_client::TweetAuthor;

    fn make_tweet(id: &str, text: &str, headline: Option<String>) -> TweetData {
        TweetData {
            id: id.to_string(),
            text: text.to_string(),
            author: TweetAuthor {
                username: "test".to_string(),
                name: "Test".to_string(),
            },
            author_id: None,
            created_at: None,
            reply_count: None,
            retweet_count: None,
            like_count: None,
            conversation_id: None,
            in_reply_to_status_id: None,
            in_reply_to_user_id: None,
            mentions: vec![],
            quoted_tweet: None,
            media: None,
            article: None,
            headline,
            _raw: None,
        }
    }

    #[test]
    fn test_needs_headline_short_tweet() {
        let tweet = make_tweet("1", "Short tweet", None);
        assert!(!needs_headline(&tweet));
    }

    #[test]
    fn test_needs_headline_long_tweet() {
        let long_text = "a".repeat(HEADLINE_THRESHOLD + 1);
        let tweet = make_tweet("1", &long_text, None);
        assert!(needs_headline(&tweet));
    }

    #[test]
    fn test_needs_headline_already_has_headline() {
        let long_text = "a".repeat(HEADLINE_THRESHOLD + 1);
        let tweet = make_tweet("1", &long_text, Some("Existing headline".to_string()));
        assert!(!needs_headline(&tweet));
    }

    #[test]
    fn test_filter_tweets_needing_headlines() {
        let short = make_tweet("1", "Short", None);
        let long = make_tweet("2", &"a".repeat(HEADLINE_THRESHOLD + 1), None);
        let has_headline = make_tweet(
            "3",
            &"b".repeat(HEADLINE_THRESHOLD + 1),
            Some("headline".to_string()),
        );

        let tweets = vec![short, long, has_headline];
        let needing = filter_tweets_needing_headlines(&tweets);

        assert_eq!(needing.len(), 1);
        assert_eq!(needing[0].id, "2");
    }

    #[test]
    fn test_parse_headline_response() {
        let response = r#"{"headlines": {"123": "Test headline one", "456": "Test headline two"}}"#;
        let tweet1 = make_tweet("123", &"a".repeat(250), None);
        let tweet2 = make_tweet("456", &"b".repeat(250), None);
        let tweets: Vec<&TweetData> = vec![&tweet1, &tweet2];

        let result = parse_headline_response(response, &tweets).unwrap();

        assert_eq!(result.len(), 2);
        assert_eq!(result.get("123").unwrap(), "Test headline one");
        assert_eq!(result.get("456").unwrap(), "Test headline two");
    }

    #[test]
    fn test_extract_json_markdown() {
        let response = "```json\n{\"headlines\": {}}\n```";
        let result = extract_json(response).unwrap();
        assert_eq!(result, "{\"headlines\": {}}");
    }
}
