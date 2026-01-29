//! Prompt templates and response parsing for insights generation.

use super::entities::{
    ConceptEntity, InsightsResult, PersonEntity, ResourceEntity, ResourceType, ToolCategory,
    ToolEntity, TopicEntity,
};
use bird_client::TweetData;

/// Build the system prompt for insights generation.
pub fn build_system_prompt() -> String {
    r#"You are an AI assistant that analyzes a user's Twitter/X liked and bookmarked tweets to extract insights about their interests, tools, topics, and resources they've discovered.

Your goal is to identify:
1. **Tools & Technologies**: Programming languages, frameworks, libraries, databases, AI tools, developer tools, platforms, and services mentioned
2. **Topics**: Main themes and subjects of interest
3. **Concepts**: Technical concepts worth remembering or learning more about
4. **People**: Notable individuals mentioned (especially tech/industry figures)
5. **Resources**: Articles, repositories, documentation, videos, threads, and tutorials shared

Output your analysis as valid JSON with this structure:
{
  "summary": "A 1-3 sentence high-level summary of what the user explored during this period",
  "tools": [
    {"name": "Tool Name", "category": "framework|library|database|language|devtool|aitool|platform|service|other", "description": "optional brief description"}
  ],
  "topics": [
    {"name": "Topic Name", "description": "optional brief description"}
  ],
  "concepts": [
    {"name": "Concept Name", "explanation": "optional brief explanation"}
  ],
  "people": [
    {"name": "Person Name", "handle": "twitter_handle_without_at", "context": "optional why notable"}
  ],
  "resources": [
    {"title": "Resource Title", "resource_type": "article|repository|documentation|video|thread|paper|tutorial|other", "url": "optional url", "description": "optional description"}
  ],
  "themes": ["theme1", "theme2", "theme3"]
}

Important:
- Only include items that are clearly relevant and mentioned in the tweets
- Be concise with descriptions
- Prefer quality over quantity - focus on the most significant items
- The themes array should contain 3-5 broad categories that summarize the overall focus
- Ensure valid JSON output (no trailing commas, proper escaping)"#.to_string()
}

/// Threshold for considering a tweet "long" (characters).
const HEADLINE_THRESHOLD: usize = 200;

/// Maximum preview length for long tweets (characters).
const PREVIEW_LENGTH: usize = 100;

/// Build the user prompt with tweet content.
/// For long tweets with headlines, shows the headline and a preview.
/// For short tweets, shows the full text.
pub fn build_user_prompt(tweets: &[TweetData], period_description: &str) -> String {
    let mut prompt = format!(
        "Analyze the following {} tweets from {} and extract insights:\n\n",
        tweets.len(),
        period_description
    );

    for (i, tweet) in tweets.iter().enumerate() {
        prompt.push_str(&format!("---\nTweet {}:\n@{}: ", i + 1, tweet.author.username));

        // Format tweet text based on whether it has a headline
        prompt.push_str(&format_tweet_text(tweet));
        prompt.push('\n');

        // Include quoted tweet if present
        if let Some(quoted) = &tweet.quoted_tweet {
            prompt.push_str(&format!(
                "[Quoted @{}]: {}\n",
                quoted.author.username,
                format_tweet_text(quoted)
            ));
        }
    }

    prompt.push_str("\n---\nProvide your analysis as JSON:");
    prompt
}

/// Format tweet text for the prompt.
/// If the tweet has a headline and is long, show headline + preview.
/// Otherwise show the full text.
fn format_tweet_text(tweet: &TweetData) -> String {
    let char_count = tweet.text.chars().count();

    // If tweet is short, always show full text
    if char_count <= HEADLINE_THRESHOLD {
        return tweet.text.clone();
    }

    // For long tweets, use headline if available
    if let Some(ref headline) = tweet.headline {
        let preview: String = tweet.text.chars().take(PREVIEW_LENGTH).collect();
        let preview = preview.trim();
        format!("[HEADLINE: {}] {}...", headline, preview)
    } else {
        // No headline yet, show full text
        tweet.text.clone()
    }
}

/// Parse the LLM response into InsightsResult.
pub fn parse_response(response: &str, tweets_analyzed: usize) -> Result<InsightsResult, String> {
    // Try to extract JSON from the response (handle potential markdown code blocks)
    let json_str = extract_json(response)?;

    // Parse the JSON
    let parsed: serde_json::Value =
        serde_json::from_str(&json_str).map_err(|e| format!("Failed to parse JSON: {}", e))?;

    // Build the result
    let mut result = InsightsResult {
        tweets_analyzed,
        ..Default::default()
    };

    // Extract summary
    if let Some(summary) = parsed.get("summary").and_then(|v| v.as_str()) {
        result.summary = summary.to_string();
    }

    // Extract tools
    if let Some(tools) = parsed.get("tools").and_then(|v| v.as_array()) {
        for tool in tools {
            if let (Some(name), Some(category)) = (
                tool.get("name").and_then(|v| v.as_str()),
                tool.get("category").and_then(|v| v.as_str()),
            ) {
                result.tools.push(ToolEntity {
                    name: name.to_string(),
                    category: parse_tool_category(category),
                    description: tool
                        .get("description")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                });
            }
        }
    }

    // Extract topics
    if let Some(topics) = parsed.get("topics").and_then(|v| v.as_array()) {
        for topic in topics {
            if let Some(name) = topic.get("name").and_then(|v| v.as_str()) {
                result.topics.push(TopicEntity {
                    name: name.to_string(),
                    description: topic
                        .get("description")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                });
            }
        }
    }

    // Extract concepts
    if let Some(concepts) = parsed.get("concepts").and_then(|v| v.as_array()) {
        for concept in concepts {
            if let Some(name) = concept.get("name").and_then(|v| v.as_str()) {
                result.concepts.push(ConceptEntity {
                    name: name.to_string(),
                    explanation: concept
                        .get("explanation")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                });
            }
        }
    }

    // Extract people
    if let Some(people) = parsed.get("people").and_then(|v| v.as_array()) {
        for person in people {
            if let Some(name) = person.get("name").and_then(|v| v.as_str()) {
                result.people.push(PersonEntity {
                    name: name.to_string(),
                    handle: person
                        .get("handle")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    context: person
                        .get("context")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                });
            }
        }
    }

    // Extract resources
    if let Some(resources) = parsed.get("resources").and_then(|v| v.as_array()) {
        for resource in resources {
            if let (Some(title), Some(resource_type)) = (
                resource.get("title").and_then(|v| v.as_str()),
                resource.get("resource_type").and_then(|v| v.as_str()),
            ) {
                result.resources.push(ResourceEntity {
                    title: title.to_string(),
                    resource_type: parse_resource_type(resource_type),
                    url: resource
                        .get("url")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    description: resource
                        .get("description")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                });
            }
        }
    }

    // Extract themes
    if let Some(themes) = parsed.get("themes").and_then(|v| v.as_array()) {
        for theme in themes {
            if let Some(t) = theme.as_str() {
                result.themes.push(t.to_string());
            }
        }
    }

    Ok(result)
}

/// Extract JSON from response text (handling markdown code blocks).
fn extract_json(response: &str) -> Result<String, String> {
    let trimmed = response.trim();

    // Check for markdown code block
    if trimmed.starts_with("```") {
        let lines: Vec<&str> = trimmed.lines().collect();
        if lines.len() >= 2 {
            // Skip first line (```json or ```) and last line (```)
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

    Err("Could not find valid JSON in response".to_string())
}

/// Parse tool category from string.
fn parse_tool_category(s: &str) -> ToolCategory {
    match s.to_lowercase().as_str() {
        "language" => ToolCategory::Language,
        "framework" => ToolCategory::Framework,
        "library" => ToolCategory::Library,
        "database" => ToolCategory::Database,
        "devtool" | "dev_tool" | "developer_tool" => ToolCategory::DevTool,
        "aitool" | "ai_tool" | "ai" => ToolCategory::AiTool,
        "platform" => ToolCategory::Platform,
        "service" => ToolCategory::Service,
        _ => ToolCategory::Other,
    }
}

/// Parse resource type from string.
fn parse_resource_type(s: &str) -> ResourceType {
    match s.to_lowercase().as_str() {
        "article" => ResourceType::Article,
        "repository" | "repo" => ResourceType::Repository,
        "documentation" | "docs" => ResourceType::Documentation,
        "video" => ResourceType::Video,
        "thread" => ResourceType::Thread,
        "paper" => ResourceType::Paper,
        "tutorial" => ResourceType::Tutorial,
        _ => ResourceType::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_json_direct() {
        let input = r#"{"summary": "test"}"#;
        assert_eq!(extract_json(input).unwrap(), r#"{"summary": "test"}"#);
    }

    #[test]
    fn test_extract_json_markdown() {
        let input = r#"```json
{"summary": "test"}
```"#;
        assert_eq!(extract_json(input).unwrap(), r#"{"summary": "test"}"#);
    }

    #[test]
    fn test_extract_json_embedded() {
        let input = "Here is the analysis:\n{\"summary\": \"test\"}\nEnd of response";
        assert_eq!(extract_json(input).unwrap(), r#"{"summary": "test"}"#);
    }

    #[test]
    fn test_parse_response() {
        let response = r#"{"summary": "You explored Rust and TypeScript", "tools": [{"name": "Rust", "category": "language"}], "topics": [], "concepts": [], "people": [], "resources": [], "themes": ["programming"]}"#;
        let result = parse_response(response, 10).unwrap();
        assert_eq!(result.summary, "You explored Rust and TypeScript");
        assert_eq!(result.tweets_analyzed, 10);
        assert_eq!(result.tools.len(), 1);
        assert_eq!(result.tools[0].name, "Rust");
        assert_eq!(result.themes, vec!["programming"]);
    }
}
