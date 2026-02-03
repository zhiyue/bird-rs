# bird-storage

Storage backends for bird. Implements the storage traits from [bird-core].

## Backends

| Backend            | Description                                            |
| ------------------ | ------------------------------------------------------ |
| `SurrealDbStorage` | Persistent storage using [SurrealDB] (local or remote) |
| `MemoryStorage`    | In-memory storage for testing                          |

## Usage

```rust
use bird_storage::{SurrealDbStorage, SurrealDbConfig, SurrealDbAuth, TweetStore, SyncStateStore};

// Create storage (uses ~/.bird/bird.db by default)
let storage = SurrealDbStorage::new_default().await?;

// Or with custom path
let storage = SurrealDbStorage::new("/path/to/db").await?;

// Or connect to a remote SurrealDB endpoint
let config = SurrealDbConfig {
    endpoint: "wss://cloud.surrealdb.com".to_string(),
    namespace: "bird".to_string(),
    database: "main".to_string(),
    auth: Some(SurrealDbAuth::Root {
        username: "user".to_string(),
        password: "pass".to_string(),
    }),
};
let storage = SurrealDbStorage::new_with_config(&config).await?;

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
- `SurrealDbStorage::new_with_config(&SurrealDbConfig { .. })`
- `BIRD_DB_PATH` environment variable
- `BIRD_DB_URL` environment variable (remote endpoint)
- `--db-path` CLI flag
- `--db-url` CLI flag

## Maintenance

If you introduced the `created_at_ts` field in an existing database, you can
backfill via:

```rust
let result = storage.backfill_created_at_ts(200).await?;
println!("Updated {}, skipped {}", result.updated, result.skipped);
```

## Testing

Use `MemoryStorage` for tests:

```rust
use bird_storage::MemoryStorage;

let storage = MemoryStorage::new();
// Same API as SurrealDbStorage
```

## StorageConfig

For higher-level callers, you can build a config and create a backend
dynamically:

```rust
use bird_storage::{StorageConfig, SurrealDbConfig, create_storage};
use std::path::Path;

let config = StorageConfig::SurrealDb(SurrealDbConfig::local(Path::new("/path/to/db")));
let storage = create_storage(&config).await?;
```

## Dependencies

- [bird-core] — Storage traits (`TweetStore`, `SyncStateStore`, `UserStore`)

[bird-core]: ../bird-core
[SurrealDB]: https://surrealdb.com
