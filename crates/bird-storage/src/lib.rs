//! # bird-storage
//!
//! Storage backends for the bird Twitter client.
//!
//! This crate provides implementations of the storage traits defined in `bird-core`:
//! - `SurrealDbStorage`: Persistent storage using SurrealDB (local or remote)
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
pub use surrealdb::{SurrealDbAuth, SurrealDbConfig, SurrealDbStorage};

// Re-export traits from bird-core for convenience
pub use bird_core::{Storage, SyncStateStore, TweetStore};

use std::sync::Arc;

/// Storage backend configuration.
#[derive(Debug, Clone)]
pub enum StorageConfig {
    /// SurrealDB-backed storage (local or remote).
    SurrealDb(SurrealDbConfig),
    /// In-memory storage (testing only).
    Memory,
}

/// Create a storage backend from configuration.
pub async fn create_storage(config: &StorageConfig) -> bird_core::Result<Arc<dyn Storage>> {
    match config {
        StorageConfig::SurrealDb(cfg) => {
            let storage = SurrealDbStorage::new_with_config(cfg).await?;
            Ok(Arc::new(storage))
        }
        StorageConfig::Memory => Ok(Arc::new(MemoryStorage::new())),
    }
}

/// Get the default database path (~/.bird/bird.db).
pub fn default_db_path() -> std::path::PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".bird")
        .join("bird.db")
}
