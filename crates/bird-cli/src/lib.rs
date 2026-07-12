//! # bird-cli
//!
//! A fast X/Twitter CLI for reading tweets, powered by GraphQL.
//!
//! ## Commands
//!
//! - `bird-rs whoami` - Show the logged-in account
//! - `bird-rs check` - Show available credential sources
//! - `bird-rs read <tweet-id>` - Read a tweet (cache-first)
//! - `bird-rs likes` - Fetch likes with pagination
//! - `bird-rs bookmarks` - Fetch bookmarks with pagination
//! - `bird-rs list <collection>` - List synced tweets from database (use `--columns` to customize)
//! - `bird-rs sync likes` - Sync likes to local database
//! - `bird-rs sync bookmarks` - Sync bookmarks to local database
//! - `bird-rs sync posts` - Sync your own tweets to database
//! - `bird-rs sync backfill <collection>` - Continue fetching older tweets
//! - `bird-rs sync status` - Show sync state for all collections
//! - `bird-rs sync reset <collection>` - Reset sync state for a collection
//! - `bird-rs resonance refresh` - Compute resonance scores from synced data
//! - `bird-rs insights generate [period]` - Analyze tweets using LLM
//! - `bird-rs db status` - Show database status and counts
//! - `bird-rs db optimize` - Ensure schema and indexes exist
//! - `bird-rs db backfill-created-at` - Backfill timestamps for stored tweets
//! - `bird-rs db backfill-headlines` - Generate headlines for long tweets
//! - `bird-rs config init` - Create a default config file

pub mod cli;
pub mod commands;
pub mod insights;
pub mod output;
pub mod storage_monitor;
pub mod sync_engine;
