//! GraphQL operations for the Twitter API.

pub mod bookmarks;
pub mod likes;
pub mod timeline;
pub mod tweet_detail;
pub mod user_tweets;

// Re-export common parsing utilities
pub(crate) use tweet_detail::parse_timeline_entries;
