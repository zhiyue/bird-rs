# bird-storage

Storage backends for bird. Implements the storage traits from [bird-core].

## Backends

| Backend            | Description                                   |
| ------------------ | --------------------------------------------- |
| `SurrealDbStorage` | Persistent storage using embedded [SurrealDB] |
| `MemoryStorage`    | In-memory storage for testing                 |

## Usage

```rust
use bird_storage::{SurrealDbStorage, TweetStore, SyncStateStore};

// Create storage (uses ~/.bird/bird.db by default)
let storage = SurrealDbStorage::new_default().await?;

// Or with custom path
let storage = SurrealDbStorage::new("/path/to/db").await?;

// Store tweets
storage.upsert_tweet(&tweet).await?;
storage.upsert_tweets(&tweets).await?;

// Query tweets
let tweet = storage.get_tweet("123").await?;
let exists = storage.tweet_exists("123").await?;

// Collections
storage.add_to_collection("123", "likes", "user_id").await?;
let tweets = storage.get_tweets_by_collection("likes", "user_id", None, None).await?;

// Sync state
let state = storage.get_sync_state("likes", "user_id").await?;
storage.update_sync_state(&state).await?;

// Query by mentioned user
let user = storage.get_user_by_username("elonmusk").await?;
if let Some(u) = user {
    let tweets = storage.get_tweets_mentioning_user(&u.id, Some(10)).await?;
}
```

## Database Location

Default: `~/.bird/bird.db`

Override via:

- `SurrealDbStorage::new("/custom/path")`
- `BIRD_DB_PATH` environment variable
- `--db-path` CLI flag

## Testing

Use `MemoryStorage` for tests:

```rust
use bird_storage::MemoryStorage;

let storage = MemoryStorage::new();
// Same API as SurrealDbStorage
```

## Dependencies

- [bird-core] — Storage traits (`TweetStore`, `SyncStateStore`, `UserStore`)

[bird-core]: ../bird-core
[SurrealDB]: https://surrealdb.com
