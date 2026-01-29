//! # bird-cli
//!
//! A fast X/Twitter CLI for reading tweets, powered by GraphQL.
//!
//! ## Commands
//!
//! - `bird whoami` - Show the logged-in account
//! - `bird check` - Show available credential sources
//! - `bird read <tweet-id>` - Read a tweet (cache-first)
//! - `bird likes` - Fetch likes with pagination
//! - `bird bookmarks` - Fetch bookmarks with pagination
//! - `bird list <collection>` - List synced tweets from database
//! - `bird sync likes` - Sync likes to local database
//! - `bird sync bookmarks` - Sync bookmarks to local database
//! - `bird sync posts` - Sync your own tweets to database
//! - `bird sync backfill <collection>` - Continue fetching older tweets
//! - `bird sync status` - Show sync state for all collections
//! - `bird sync reset <collection>` - Reset sync state for a collection
//! - `bird insights generate [period]` - Analyze tweets using LLM
//! - `bird db status` - Show database status and counts
//! - `bird db optimize` - Ensure schema and indexes exist
//! - `bird db backfill-created-at` - Backfill timestamps for stored tweets
//! - `bird db backfill-headlines` - Generate headlines for long tweets
//! - `bird config init` - Create a default config file

pub mod cli;
pub mod commands;
pub mod insights;
pub mod output;
pub mod storage_monitor;
pub mod sync_engine;
