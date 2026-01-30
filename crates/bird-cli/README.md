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
bird list                      # List all tweets (interleaved from all collections)
bird list --page 2             # Pagination across all collections
bird list likes                # List only liked tweets
bird list bookmarks            # List only bookmarked tweets
bird list user_tweets          # List only your posts

# Custom columns with collections, scores, and interactions
bird list --columns id,text,collections,score,liked,bookmarked
bird list likes --columns id,headline,score

# Pagination options
bird list --page-size 50       # Custom page size
bird list --json               # JSON output
```

### Insights (LLM Analysis)

```bash
bird insights generate         # Analyze tweets from last week
bird insights generate day     # Last day
bird insights generate month   # Last month
bird insights generate --collection likes
bird insights generate --max-tweets 50
bird insights generate -v      # Verbose output
```

### Database Maintenance

```bash
bird db status                 # Show database stats
bird db status --debug         # Include timestamp distribution
bird db optimize               # Ensure schema/indexes exist

# Repair: heal missing data (headlines + resonance scores)
bird db repair                 # Backfill headlines and recalculate all scores
bird db repair --min-length 300 # Only generate headlines for tweets >300 chars

# Individual maintenance commands
bird db backfill-created-at    # Backfill timestamps
bird db backfill-headlines     # Generate headlines for long tweets
bird db backfill-headlines --max-tweets 100
```

### Resonance Scoring

```bash
# Compute scores based on your interactions (likes, bookmarks, replies, quotes, retweets)
bird resonance refresh

# View scores with interactions (synergy bonus when both liked and bookmarked)
bird list --columns id,text,liked,bookmarked,score

# Score formula: base × active_multiplier × synergy_multiplier
# Base = 1.0 + like×0.25 + bookmark×1.0
# Active = 1.0 + reply×0.5 + quote×0.75 + retweet×0.8
# Synergy = 1.5 if both liked AND bookmarked, else 1.0
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
