# bird-cli

CLI binary for bird. Provides commands for reading tweets, fetching
likes/bookmarks, and syncing to local storage.

## Install

```bash
cargo install --path .
```

## Usage

```bash
bird --help
```

### Reading Tweets

```bash
bird 1234567890123456789              # By ID
bird https://x.com/u/status/123...    # By URL
bird read 123... --json               # JSON output
```

### Fetching Collections

```bash
bird likes                     # First page of likes
bird likes --all               # All pages (careful!)
bird likes --max-pages 5       # Limit pages
bird bookmarks --json          # JSON output
```

### Syncing to Database

```bash
bird sync likes                # Sync liked tweets
bird sync bookmarks            # Sync bookmarked tweets
bird sync posts                # Sync your own tweets
bird sync likes --full         # Full re-sync
bird sync likes --delay 2000   # 2s between requests
bird sync backfill likes       # Continue fetching older
bird sync status               # Show progress
bird sync reset likes          # Clear sync state
```

### Listing Synced Tweets

```bash
bird list likes                # List synced likes (default)
bird list bookmarks            # List synced bookmarks
bird list user_tweets          # List synced posts
bird list likes --page 2       # Pagination
bird list likes --page-size 50 # Custom page size
bird list likes --json         # JSON output
```

## Options

| Flag          | Description                     |
| ------------- | ------------------------------- |
| `--json`      | Output as JSON                  |
| `--plain`     | No emoji, no color              |
| `--no-cache`  | Skip local DB, hit API          |
| `--db-path`   | Custom DB location              |
| `--delay`     | Delay between API requests (ms) |
| `--max-pages` | Limit pages fetched             |

## Database

Tweets are stored in `~/.bird/bird.db` (SurrealDB). Override with `--db-path` or
`BIRD_DB_PATH`.

## Dependencies

- [bird-client] — Twitter API client
- [bird-storage] — Database backends
- [bird-core] — Shared types

[bird-client]: ../bird-client
[bird-storage]: ../bird-storage
[bird-core]: ../bird-core
