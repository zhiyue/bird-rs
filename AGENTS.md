# AGENTS.md

You are working on **bird-rs**, a fast X/Twitter CLI written in Rust for reading tweets and syncing collections (likes, bookmarks) to a local SurrealDB database.

## Project Structure

```
bird-rs/
├── crates/
│   ├── bird-core/       # Shared types, traits, error handling
│   ├── bird-client/     # Twitter GraphQL API client
│   ├── bird-storage/    # Database backends (SurrealDB, in-memory)
│   └── bird-cli/        # CLI binary and command handlers
├── Cargo.toml           # Workspace configuration
└── .github/workflows/   # CI/CD pipeline
```

**Dependency flow**: `bird-cli` → `bird-client` + `bird-storage` → `bird-core`

## Commands

```bash
# Build
cargo build --workspace

# Test (run before committing)
cargo test --workspace

# Lint (warnings are errors)
cargo clippy --all-targets -- -D warnings

# Format
cargo fmt --all

# Format check (CI uses this)
cargo fmt --all -- --check

# Install locally
cargo install --path crates/bird-cli

# Run CLI
cargo run -p bird-cli -- --help
cargo run -p bird-cli -- whoami
cargo run -p bird-cli -- read <tweet_id>
```

## Tech Stack

- **Rust 2021 Edition** with async/await (tokio runtime)
- **reqwest** for HTTP, **serde** for serialization
- **surrealdb** with RocksDB backend for persistence
- **clap** for CLI argument parsing
- **thiserror** for error types, **anyhow** for application errors

## Code Style

Follow existing patterns. Here are concrete examples:

### Error Types (bird-core)

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Missing credentials: {0}")]
    MissingCredentials(String),

    #[error("API error: {0}")]
    ApiError(String),

    #[error("Storage error: {0}")]
    Storage(String),
}

pub type Result<T> = std::result::Result<T, Error>;
```

### Domain Types

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TweetData {
    pub id: String,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<TweetAuthor>,
}
```

### Async Traits (bird-core)

```rust
use async_trait::async_trait;

#[async_trait]
pub trait TweetStore: Send + Sync {
    async fn store_tweet(&self, tweet: &TweetData) -> Result<()>;
    async fn get_tweet(&self, id: &str) -> Result<Option<TweetData>>;
}
```

### Naming Conventions

- **Types**: PascalCase (`TweetData`, `TwitterClient`)
- **Functions**: snake_case (`get_tweet`, `resolve_credentials`)
- **Constants**: SCREAMING_SNAKE_CASE
- **Modules**: snake_case (`tweet_detail`, `sync_engine`)

### Documentation

Add doc comments to public items:

```rust
/// Fetches a tweet by ID from the Twitter API.
///
/// # Arguments
/// * `id` - The tweet ID to fetch
///
/// # Returns
/// The tweet data if found, or an error
pub async fn get_tweet(&self, id: &str) -> Result<TweetData> {
    // ...
}
```

## Testing

Tests use `#[tokio::test]` for async and `tempfile` for temp databases:

```rust
#[tokio::test]
async fn test_store_and_retrieve_tweet() {
    let temp_dir = tempfile::tempdir().unwrap();
    let storage = SurrealDbStorage::new(temp_dir.path()).await.unwrap();

    let tweet = TweetData { id: "123".into(), text: "Hello".into(), .. };
    storage.store_tweet(&tweet).await.unwrap();

    let retrieved = storage.get_tweet("123").await.unwrap();
    assert_eq!(retrieved.unwrap().text, "Hello");
}
```

Run tests with: `cargo test --workspace`

## Git Workflow

- Commits should be atomic and descriptive
- Pre-commit hooks run: `cargo fmt`, `cargo clippy`, `cargo test`
- CI runs on Ubuntu, macOS, and Windows

## Boundaries

### Always Do

- Run `cargo clippy --all-targets -- -D warnings` before committing
- Run `cargo test --workspace` to verify changes
- Use `?` operator for error propagation
- Add `#[derive(Debug, Clone, Serialize, Deserialize)]` to domain types
- Use `Option<T>` for nullable fields with `#[serde(skip_serializing_if = "Option::is_none")]`
- Follow existing module organization patterns

### Ask First

- Adding new dependencies to Cargo.toml
- Creating new crates in the workspace
- Changing public trait signatures in bird-core
- Modifying database schema or table structures
- Changing CLI command structure or flags

### Never Do

- Commit code that fails `cargo clippy -- -D warnings`
- Remove or modify existing tests without discussion
- Store credentials, tokens, or secrets in code
- Use `unwrap()` in library code (use `?` or proper error handling)
- Add `println!` for debugging (use proper logging or remove before commit)
- Modify `.github/workflows/` without explicit request
- Change the workspace Cargo.toml dependency versions without reason
