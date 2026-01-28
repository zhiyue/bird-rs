//! Core data types for the Twitter client.

use serde::{Deserialize, Serialize};

/// Media attachment on a tweet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TweetMedia {
    /// Type of media (photo, video, animated_gif).
    #[serde(rename = "type")]
    pub media_type: MediaType,
    /// URL to the media.
    pub url: String,
    /// Preview/thumbnail URL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview_url: Option<String>,
    /// Width in pixels.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    /// Height in pixels.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    /// For video/animated_gif: best quality video URL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_url: Option<String>,
    /// Duration in milliseconds (for video).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
}

/// Type of media attachment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaType {
    Photo,
    Video,
    AnimatedGif,
}

/// Author information for a tweet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TweetAuthor {
    /// Username (handle without @).
    pub username: String,
    /// Display name.
    pub name: String,
}

/// Article metadata for long-form posts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TweetArticle {
    /// Article title.
    pub title: String,
    /// Preview text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview_text: Option<String>,
}

/// A mentioned user in a tweet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MentionedUser {
    /// User ID.
    pub id: String,
    /// Username (handle without @).
    pub username: String,
    /// Display name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Parsed tweet data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TweetData {
    /// Tweet ID.
    pub id: String,
    /// Tweet text content.
    pub text: String,
    /// Author information.
    pub author: TweetAuthor,
    /// Author's user ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author_id: Option<String>,
    /// Creation timestamp (ISO 8601).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    /// Number of replies.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_count: Option<u64>,
    /// Number of retweets.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retweet_count: Option<u64>,
    /// Number of likes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub like_count: Option<u64>,
    /// Conversation ID (for threads).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conversation_id: Option<String>,
    /// ID of tweet being replied to.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub in_reply_to_status_id: Option<String>,
    /// User ID of the user being replied to.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub in_reply_to_user_id: Option<String>,
    /// Users mentioned in this tweet.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mentions: Vec<MentionedUser>,
    /// Quoted tweet (nested, depth controlled by quote_depth).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quoted_tweet: Option<Box<TweetData>>,
    /// Media attachments.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media: Option<Vec<TweetMedia>>,
    /// Article metadata (for long-form posts).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub article: Option<TweetArticle>,
    /// Raw GraphQL response (when include_raw is enabled).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _raw: Option<serde_json::Value>,
}

/// Twitter user profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TwitterUser {
    /// User ID.
    pub id: String,
    /// Username (handle without @).
    pub username: String,
    /// Display name.
    pub name: String,
    /// Bio/description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Number of followers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub followers_count: Option<u64>,
    /// Number of accounts followed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub following_count: Option<u64>,
    /// Whether the user has Twitter Blue verification.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_blue_verified: Option<bool>,
    /// Profile image URL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_image_url: Option<String>,
    /// Account creation timestamp.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
}

/// Current authenticated user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurrentUser {
    /// User ID.
    pub id: String,
    /// Username (handle without @).
    pub username: String,
    /// Display name.
    pub name: String,
}

/// Twitter List.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TwitterList {
    /// List ID.
    pub id: String,
    /// List name.
    pub name: String,
    /// List description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Number of members.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub member_count: Option<u64>,
    /// Number of subscribers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subscriber_count: Option<u64>,
    /// Whether the list is private.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_private: Option<bool>,
    /// Creation timestamp.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    /// List owner.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<TwitterUser>,
}

/// Result of fetching a single tweet.
#[derive(Debug, Clone)]
pub enum GetTweetResult {
    /// Successfully fetched the tweet.
    Success(Box<TweetData>),
    /// Failed to fetch the tweet.
    Error(String),
}

/// Result of a search operation.
#[derive(Debug, Clone)]
pub enum SearchResult {
    /// Successfully performed the search.
    Success {
        /// Found tweets.
        tweets: Vec<TweetData>,
        /// Cursor for pagination.
        next_cursor: Option<String>,
    },
    /// Failed to perform the search.
    Error {
        /// Error message.
        error: String,
        /// Partial results (if any).
        tweets: Vec<TweetData>,
        /// Cursor for pagination (if available).
        next_cursor: Option<String>,
    },
}

/// Result of fetching the current user.
#[derive(Debug, Clone)]
pub enum CurrentUserResult {
    /// Successfully fetched the user.
    Success(CurrentUser),
    /// Failed to fetch the user.
    Error(String),
}

/// Result of fetching following/followers list.
#[derive(Debug, Clone)]
pub enum FollowingResult {
    /// Successfully fetched the list.
    Success {
        /// Users in the list.
        users: Vec<TwitterUser>,
        /// Cursor for pagination.
        next_cursor: Option<String>,
    },
    /// Failed to fetch the list.
    Error(String),
}

/// Collection type for categorizing tweets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Collection {
    /// User's liked tweets.
    Likes,
    /// User's bookmarked tweets.
    Bookmarks,
    /// User's home timeline.
    Timeline,
    /// User's own tweets.
    UserTweets,
}

impl Collection {
    /// Get the collection name as a string.
    pub fn as_str(&self) -> &'static str {
        match self {
            Collection::Likes => "likes",
            Collection::Bookmarks => "bookmarks",
            Collection::Timeline => "timeline",
            Collection::UserTweets => "user_tweets",
        }
    }
}

impl std::fmt::Display for Collection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for Collection {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "likes" => Ok(Collection::Likes),
            "bookmarks" => Ok(Collection::Bookmarks),
            "timeline" => Ok(Collection::Timeline),
            "user_tweets" | "posts" => Ok(Collection::UserTweets),
            _ => Err(format!("Unknown collection: {}", s)),
        }
    }
}
