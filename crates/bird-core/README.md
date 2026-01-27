# bird-core

Shared types, traits, and errors for the bird workspace.

## Types

### Tweet Data

```rust
use bird_core::{TweetData, TweetAuthor, TweetMedia};

let tweet = TweetData {
    id: "123".to_string(),
    text: "Hello world".to_string(),
    author: TweetAuthor { username: "user".to_string(), name: "User".to_string() },
    // ...
};
```

### Pagination

```rust
use bird_core::{PaginationOptions, PaginatedResult, SyncState};

// Request options
let opts = PaginationOptions::new()
    .with_max_pages(10)
    .with_cursor("abc123")
    .with_stop_at_id("known_tweet_id");

// Response
let result: PaginatedResult<TweetData> = /* ... */;
println!("Fetched {} items, has_more: {}", result.items.len(), result.has_more);

// Sync state (bidirectional)
let mut state = SyncState::new("likes", "user_id");
state.update_forward(Some("newest_id".into()), 50);
state.update_backfill(Some("oldest_id".into()), cursor, has_more, 100);
```

## Traits

### TweetStore

```rust
#[async_trait]
pub trait TweetStore: Send + Sync {
    async fn upsert_tweet(&self, tweet: &TweetData) -> Result<()>;
    async fn upsert_tweets(&self, tweets: &[TweetData]) -> Result<usize>;
    async fn get_tweet(&self, id: &str) -> Result<Option<TweetData>>;
    async fn tweet_exists(&self, id: &str) -> Result<bool>;
    // ...
}
```

### SyncStateStore

```rust
#[async_trait]
pub trait SyncStateStore: Send + Sync {
    async fn get_sync_state(&self, collection: &str, user_id: &str) -> Result<Option<SyncState>>;
    async fn update_sync_state(&self, state: &SyncState) -> Result<()>;
    async fn clear_sync_state(&self, collection: &str, user_id: &str) -> Result<()>;
    // ...
}
```

## Errors

```rust
use bird_core::{Error, Result};

fn example() -> Result<()> {
    Err(Error::Api("rate limited".into()))
}
```

## Collections

```rust
use bird_core::Collection;

let c: Collection = "likes".parse()?;
assert_eq!(c.as_str(), "likes");
```

Supported: `likes`, `bookmarks`, `timeline`, `user_tweets`
