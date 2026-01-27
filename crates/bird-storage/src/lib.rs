//! # bird-storage
//!
//! Storage backends for the bird Twitter client.
//!
//! This crate provides implementations of the storage traits defined in `bird-core`:
//! - `SurrealDbStorage`: Persistent storage using embedded SurrealDB
//! - `MemoryStorage`: In-memory storage for testing
//!
//! ## Example
//!
//! ```ignore
//! use bird_storage::SurrealDbStorage;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let storage = SurrealDbStorage::new_default().await?;
//!     // Use storage...
//!     Ok(())
//! }
//! ```

pub mod memory;
pub mod surrealdb;

pub use memory::MemoryStorage;
pub use surrealdb::SurrealDbStorage;

// Re-export traits from bird-core for convenience
pub use bird_core::{Storage, SyncStateStore, TweetStore};

/// Get the default database path (~/.bird/bird.db).
pub fn default_db_path() -> std::path::PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".bird")
        .join("bird.db")
}
