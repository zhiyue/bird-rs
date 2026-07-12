# bird-cli

CLI binary for bird-rs. Provides commands for reading tweets, fetching
likes/bookmarks, and syncing to local storage.

## Install

```bash
cargo install --path .
```

## Usage

```bash
bird-rs --help
```

### Reading Tweets

```bash
bird-rs 1234567890123456789              # By ID
bird-rs https://x.com/u/status/123...    # By URL
bird-rs read 123... --json               # JSON output
```

### Fetching Collections

```bash
bird-rs likes                     # First page of likes
bird-rs likes --all               # All pages (careful!)
bird-rs likes --max-pages 5       # Limit pages
bird-rs bookmarks --json          # JSON output
```

### Syncing to Database

```bash
bird-rs sync likes                # Sync liked tweets
bird-rs sync bookmarks            # Sync bookmarked tweets
bird-rs sync posts                # Sync your own tweets
bird-rs sync likes --full         # Full re-sync
bird-rs sync likes --delay 2000   # 2s per fetched tweet for request pacing
bird-rs sync backfill likes       # Continue fetching older
bird-rs sync status               # Show progress
bird-rs sync reset likes          # Clear sync state
```

Sync stores each fetched page before requesting the next one. Interrupted runs
therefore keep completed pages and can continue from their saved state.

### Listing Synced Tweets

```bash
bird-rs list                      # List all tweets (interleaved from all collections)
bird-rs list --page 2             # Pagination across all collections
bird-rs list likes                # List only liked tweets
bird-rs list bookmarks            # List only bookmarked tweets
bird-rs list user_tweets          # List only your posts

# Custom columns with collections, scores, and interactions
bird-rs list --columns id,text,collections,score,liked,bookmarked
bird-rs list likes --columns id,headline,score

# Options
bird-rs list --page-size 50       # Custom page size
bird-rs list --json               # JSON output
```

### Insights (LLM Analysis)

```bash
bird-rs insights generate         # Analyze tweets from last week
bird-rs insights generate day     # Last day
bird-rs insights generate month   # Last month
bird-rs insights generate --collection likes
bird-rs insights generate --max-tweets 50
bird-rs insights generate -v      # Verbose output
```

### Database Maintenance

```bash
bird-rs db status                 # Show database stats
bird-rs db status --debug         # Include timestamp distribution
bird-rs db optimize               # Ensure schema/indexes exist

# Repair: heal missing data (headlines + resonance scores)
bird-rs db repair                 # Backfill headlines and recalculate all scores
bird-rs db repair --min-length 300 # Only generate headlines for tweets >300 chars
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
