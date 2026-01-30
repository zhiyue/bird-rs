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
    HttpRequest(String),

    /// JSON parsing failed.
    #[error("Failed to parse JSON response: {0}")]
    JsonParse(String),

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

    /// Rate limited with optional reset timestamp.
    #[error("Rate limited by Twitter API{}", .0.map(|ts| format!(" (resets at {})", ts)).unwrap_or_default())]
    RateLimited(Option<i64>),

    /// Request timed out.
    #[error("Request timed out")]
    Timeout,

    /// Invalid tweet ID format.
    #[error("Invalid tweet ID: {0}")]
    InvalidTweetId(String),

    /// Storage error.
    #[error("Storage error: {0}")]
    Storage(String),

    /// Serialization error.
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// IO error.
    #[error("IO error: {0}")]
    Io(String),
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::JsonParse(e.to_string())
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e.to_string())
    }
}
