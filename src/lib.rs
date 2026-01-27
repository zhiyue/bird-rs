//! # bird
//!
//! A fast X/Twitter CLI for reading tweets, powered by GraphQL.
//!
//! This library provides a Rust client for interacting with X/Twitter's
//! undocumented GraphQL API using cookie-based authentication.
//!
//! ## Features
//!
//! - Read tweets by ID or URL
//! - Fetch replies and threads
//! - Search tweets
//! - View bookmarks, likes, and timelines
//! - Cookie-based authentication via Safari (via sweet-cookie)
//!
//! ## Example
//!
//! ```no_run
//! use bird::{TwitterClient, TwitterClientOptions, cookies::resolve_credentials};
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
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

pub mod cli;
pub mod client;
pub mod constants;
pub mod cookies;
pub mod error;
pub mod output;
pub mod types;

pub use client::TwitterClient;
pub use error::{Error, Result};
pub use types::*;

/// Options for creating a Twitter client.
#[derive(Debug, Clone)]
pub struct TwitterClientOptions {
    /// Authentication cookies.
    pub cookies: cookies::TwitterCookies,
    /// Request timeout in milliseconds.
    pub timeout_ms: Option<u64>,
    /// Max depth for quoted tweets (0 disables). Defaults to 1.
    pub quote_depth: Option<u32>,
}
