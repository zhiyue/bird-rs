# bird

A fast X/Twitter CLI for reading and syncing tweets. Written in Rust.

## Install

```bash
cargo install --path crates/bird-cli
```

## Quick Start

```bash
# Authenticate (auto-extracts Safari cookies on macOS)
bird whoami

# Read a tweet
bird 1234567890123456789
bird https://x.com/user/status/1234567890123456789

# Fetch your likes
bird likes --max-pages 3

# Sync likes to local database
bird sync likes
```

## Commands

| Command               | Description                   |
| --------------------- | ----------------------------- |
| `bird <id>`           | Read a tweet by ID or URL     |
| `bird whoami`         | Show logged-in account        |
| `bird likes`          | Fetch your liked tweets       |
| `bird bookmarks`      | Fetch your bookmarks          |
| `bird list likes`     | List synced likes from DB     |
| `bird list bookmarks` | List synced bookmarks from DB |
| `bird list user_tweets` | List synced posts from DB   |
| `bird sync likes`     | Sync likes to local DB        |
| `bird sync bookmarks` | Sync bookmarks to local DB    |
| `bird sync posts`     | Sync your own tweets to DB    |
| `bird sync status`    | Show sync progress            |
| `bird resonance refresh` | Compute resonance scores   |
| `bird insights generate` | Analyze tweets with LLM    |
| `bird db status`      | Show database status and counts |
| `bird db optimize`    | Ensure schema and indexes exist |
| `bird db backfill-created-at` | Backfill timestamps for stored tweets |
| `bird db backfill-headlines` | Generate headlines for long tweets |
| `bird config init`    | Write a default config file   |

## Sync

Bird stores tweets in a local [SurrealDB] database (`~/.bird/bird.db`) for
offline access and incremental sync.

```bash
# Sync your likes, bookmarks, or own posts
bird sync likes
bird sync bookmarks
bird sync posts

# Continue fetching older tweets (explicit backfill)
bird sync backfill likes

# Check progress
bird sync status
```

The sync is **forward-only by default**: it catches up on new items, and you can
explicitly backfill older history over multiple sessions.

## Insights

Bird can analyze your synced tweets using an LLM to extract tools, topics,
concepts, people, and resources you've been exploring.

```bash
# Analyze tweets from the last week (default)
bird insights generate

# Analyze different time periods
bird insights generate day
bird insights generate week
bird insights generate month

# Filter by collection
bird insights generate --collection likes
bird insights generate --collection bookmarks

# Limit tweets analyzed
bird insights generate --max-tweets 50

# Show verbose output
bird insights generate -v
```

The insights command uses Claude Code by default (requires the `claude` CLI).
Long tweets (>200 chars) automatically get LLM-generated headlines for more
efficient analysis; these are cached in the database for reuse.

To backfill headlines for existing tweets:

```bash
bird db backfill-headlines --max-tweets 100
```

## Resonance Scores

Bird can track which tweets resonated most with you based on your interactions:

| Interaction | Weight |
| ----------- | ------ |
| Bookmark    | 1.0    |
| Quote       | 0.75   |
| Like        | 0.5    |
| Reply       | 0.25   |

First, compute resonance scores from your synced data:

```bash
bird resonance refresh
```

Then use the `--columns` flag with `bird list` to display scores:

```bash
# Show score, liked, and bookmarked columns
bird list likes --columns id,text,score,liked,bookmarked

# Available columns: id, text, time, author, liked, bookmarked, score, headline
bird list bookmarks --columns id,author,score
```

Resonance scores are cached locally and run fully offline. Re-run `bird resonance refresh`
after syncing new data to update scores.

## Storage

By default, Bird uses an embedded SurrealDB (RocksDB) database at
`~/.bird/bird.db`. To use a remote SurrealDB (including Surreal Cloud), pass a
remote endpoint and optional credentials:

```bash
bird sync likes \
  --db-url "wss://cloud.surrealdb.com" \
  --db-namespace bird \
  --db-name main \
  --db-user your_user \
  --db-pass your_pass
```

By default, Bird uses **root** authentication when credentials are provided. To
use namespace or database authentication, set `--db-auth namespace` or
`--db-auth database`.

Environment variables are also supported:

```bash
export BIRD_DB_URL="wss://cloud.surrealdb.com"
export BIRD_DB_NAMESPACE="bird"
export BIRD_DB_NAME="main"
export BIRD_DB_USER="your_user"
export BIRD_DB_PASS="your_pass"
```

For testing, you can use in-memory storage:

```bash
bird --storage memory sync likes
```

## Config File

Bird will load `~/.bird/config.toml` if present (override with `--config` or
`BIRD_CONFIG`). CLI flags and environment variables take precedence. Example:

```toml
[storage]
backend = "surrealdb"
db_url = "wss://cloud.surrealdb.com"
namespace = "bird"
database = "main"
auth = "root"
user = "your_user"
pass = "your_pass"
```

For local storage, you can set `db_path` instead of `db_url`.

Generate a starter config with:

```bash
bird config init
# Overwrite existing file
bird config init --force
```

## Migrations

If you upgraded from a version without `created_at_ts`, run:

```bash
bird db backfill-created-at
```

## Authentication

Bird auto-extracts cookies from Safari on macOS. Alternatively:

```bash
# Environment variables
export AUTH_TOKEN=your_auth_token
export CT0=your_ct0_token

# Or CLI flags
bird --auth-token TOKEN --ct0 CT0 whoami
```

## Technical Notes

### Query ID Discovery

Twitter rotates GraphQL query IDs periodically, which can break API clients.
Bird automatically discovers fresh IDs by scraping Twitter's JavaScript bundles.
This is transparent and requires no manual intervention.

- Cache: `~/.config/bird/query-ids-cache.json` (24-hour TTL)
- Auto-refreshes on stale cache or API errors
- Falls back to static IDs if discovery fails

See [docs/query-id-rotation.md](docs/query-id-rotation.md) for details.

### Rate Limiting

Bird uses human-like request pacing (2.25 seconds per tweet) to avoid rate
limits. When rate limited, it respects Twitter's `x-rate-limit-reset` header.

## Crates

| Crate          | Description             |
| -------------- | ----------------------- |
| [bird-cli]     | CLI binary              |
| [bird-client]  | Twitter GraphQL client  |
| [bird-storage] | Database backends       |
| [bird-core]    | Shared types and traits |

## Development

```bash
cargo build --workspace
cargo test --workspace
cargo clippy --all-targets -- -D warnings
```

Enable the pre-commit hook (runs fmt/clippy/test):

```bash
git config core.hooksPath .githooks
```

## License

MIT

[SurrealDB]: https://surrealdb.com
[bird-cli]: crates/bird-cli
[bird-client]: crates/bird-client
[bird-storage]: crates/bird-storage
[bird-core]: crates/bird-core
