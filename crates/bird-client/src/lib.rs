//! # bird-client
//!
//! Twitter/X GraphQL API client with pagination support.
//!
//! This crate provides a Rust client for interacting with X/Twitter's
//! undocumented GraphQL API using cookie-based authentication.
//!
//! ## Features
//!
//! - Read tweets by ID or URL
//! - Fetch replies and threads
//! - Paginated likes, bookmarks, and timelines
//! - Cookie-based authentication via Safari (via sweet-cookie)
//!
//! ## Example
//!
//! ```ignore
//! use bird_client::{TwitterClient, TwitterClientOptions, cookies::resolve_credentials};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let cookies = resolve_credentials(None, None, &[])?;
//!     let client = TwitterClient::new(TwitterClientOptions {
//!         cookies,
//!         timeout_ms: Some(30000),
//!         quote_depth: Some(1),
//!     });
//!
//!     let tweet = client.get_tweet("1234567890123456789").await?;
//!     println!("{:?}", tweet);
//!     Ok(())
//! }
//! ```

mod client;
pub mod constants;
pub mod cookies;
mod operations;
pub mod query_ids;

pub use bird_core::{
    Collection, CurrentUser, CurrentUserResult, Error, FollowingResult, GetTweetResult, MediaType,
    PaginatedResult, PaginationOptions, Result, SearchResult, SyncState, TweetArticle, TweetAuthor,
    TweetData, TweetMedia, TwitterList, TwitterUser,
};
pub use client::{RateLimitConfig, RateLimitInfo, TwitterClient};
pub use cookies::TwitterCookies;

/// Options for creating a Twitter client.
#[derive(Debug, Clone)]
pub struct TwitterClientOptions {
    /// Authentication cookies.
    pub cookies: TwitterCookies,
    /// Request timeout in milliseconds.
    pub timeout_ms: Option<u64>,
    /// Max depth for quoted tweets (0 disables). Defaults to 1.
    pub quote_depth: Option<u32>,
}
