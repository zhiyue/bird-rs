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
//! - `bird sync likes` - Sync likes to local database
//! - `bird sync bookmarks` - Sync bookmarks to local database
//! - `bird sync status` - Show sync state for all collections
//! - `bird sync reset <collection>` - Reset sync state for a collection

pub mod cli;
pub mod commands;
pub mod output;
pub mod storage_monitor;
pub mod sync_engine;
