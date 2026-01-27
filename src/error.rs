//! Error types for the bird library.

use thiserror::Error;

/// Result type alias using our Error type.
pub type Result<T> = std::result::Result<T, Error>;

/// Errors that can occur when using the bird library.
#[derive(Error, Debug)]
pub enum Error {
    /// Missing authentication credentials.
    #[error("Missing authentication credentials: both auth_token and ct0 are required")]
    MissingCredentials,

    /// Cookie extraction failed.
    #[error("Failed to extract cookies: {0}")]
    CookieExtraction(String),

    /// HTTP request failed.
    #[error("HTTP request failed: {0}")]
    HttpRequest(#[from] reqwest::Error),

    /// JSON parsing failed.
    #[error("Failed to parse JSON response: {0}")]
    JsonParse(#[from] serde_json::Error),

    /// Invalid URL provided.
    #[error("Invalid URL: {0}")]
    InvalidUrl(String),

    /// Tweet not found.
    #[error("Tweet not found: {0}")]
    TweetNotFound(String),

    /// User not found.
    #[error("User not found: {0}")]
    UserNotFound(String),

    /// API error returned by Twitter.
    #[error("Twitter API error: {0}")]
    ApiError(String),

    /// Rate limited.
    #[error("Rate limited by Twitter API")]
    RateLimited,

    /// Request timed out.
    #[error("Request timed out")]
    Timeout,

    /// Invalid tweet ID format.
    #[error("Invalid tweet ID: {0}")]
    InvalidTweetId(String),
}
