//! # bird-core
//!
//! Core types, traits, and error handling for the bird Twitter client.
//!
//! This crate provides:
//! - Data types for tweets, users, and lists
//! - Error types and Result alias
//! - Pagination types for cursor-based API responses
//! - Storage traits for pluggable backends

pub mod error;
pub mod pagination;
pub mod storage;
pub mod types;

pub use error::{Error, Result};
pub use pagination::{PaginatedResult, PaginationOptions, SyncState};
pub use storage::{ResonanceScore, ResonanceStore, Storage, SyncStateStore, TweetStore, UserStore};
pub use types::*;
