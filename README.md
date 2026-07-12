# bird-rs

A fast X/Twitter CLI for reading and syncing tweets. Written in Rust.

## Install

Homebrew:

```bash
brew install zhiyue/tap/bird-rs
```

From source:

```bash
cargo install --git https://github.com/zhiyue/bird-rs --package bird-cli --locked
```

## Quick Start

```bash
# Authenticate (auto-extracts Safari cookies on macOS)
bird-rs whoami

# Read a tweet
bird-rs 1234567890123456789
bird-rs https://x.com/user/status/1234567890123456789

# Fetch your likes
bird-rs likes --max-pages 3

# Sync likes to local database
bird-rs sync likes
```

## Commands

| Command                  | Description                                                            |
| ------------------------ | ---------------------------------------------------------------------- |
| `bird-rs <id>`              | Read a tweet by ID or URL                                              |
| `bird-rs whoami`            | Show logged-in account                                                 |
| `bird-rs likes`             | Fetch your liked tweets                                                |
| `bird-rs bookmarks`         | Fetch your bookmarks                                                   |
| `bird-rs list`              | List all synced tweets (interleaved from likes, bookmarks, posts)      |
| `bird-rs list likes`        | List synced likes from DB                                              |
| `bird-rs list bookmarks`    | List synced bookmarks from DB                                          |
| `bird-rs list user_tweets`  | List synced posts from DB                                              |
| `bird-rs sync likes`        | Sync likes to local DB                                                 |
| `bird-rs sync bookmarks`    | Sync bookmarks to local DB                                             |
| `bird-rs sync posts`        | Sync your own tweets to DB                                             |
| `bird-rs sync status`       | Show sync progress                                                     |
| `bird-rs db repair`         | Heal missing data: backfill headlines and recalculate resonance scores |
| `bird-rs insights generate` | Analyze tweets with LLM                                                |
| `bird-rs db status`         | Show database status and counts                                        |
| `bird-rs db optimize`       | Ensure schema and indexes exist                                        |
| `bird-rs config init`       | Write a default config file                                            |

## Sync

Bird stores tweets in a local [SurrealDB] database (`~/.bird/bird.db`) for
offline access and incremental sync.

```bash
# Sync your likes, bookmarks, or own posts
bird-rs sync likes
bird-rs sync bookmarks
bird-rs sync posts

# Continue fetching older tweets (explicit backfill)
bird-rs sync backfill likes

# Check progress
bird-rs sync status
```

The sync catches up on new items first, then automatically backfills older
history in batches. Use `--no-backfill` for a forward-only run, or use
`bird-rs sync backfill <collection>` to continue backfilling explicitly.
Each API page is saved before the next page is requested, so interrupted runs
keep their completed work. `--max-pages` is a total safety ceiling for the run.

## Collections & Interleaved View

By default, `bird-rs list` shows an **interleaved view** of all your collections
(likes, bookmarks, posts) in a single list, deduplicated and ordered by earliest
discovery time:

```bash
# Show all collections interleaved (default)
bird-rs list

# Pagination works across all collections
bird-rs list --page 2

# Show only a specific collection (backward compatible)
bird-rs list likes
bird-rs list bookmarks
bird-rs list user_tweets
```

Use the `collections` column to see which collections contain each tweet:

```bash
# Show collection membership (❤️ for likes, 🔖 for bookmarks, 📝 for posts)
bird-rs list --columns id,text,collections

# Combine with scores and other data
bird-rs list --columns id,text,collections,liked,bookmarked,score
```

## Insights

Bird can analyze your synced tweets using an LLM to extract tools, topics,
concepts, people, and resources you've been exploring.

```bash
# Analyze tweets from the last week (default)
bird-rs insights generate

# Analyze different time periods
bird-rs insights generate day
bird-rs insights generate week
bird-rs insights generate month

# Filter by collection
bird-rs insights generate --collection likes
bird-rs insights generate --collection bookmarks

# Limit tweets analyzed
bird-rs insights generate --max-tweets 50

# Show verbose output
bird-rs insights generate -v
```

The insights command uses Claude Code by default (requires the `claude` CLI).
Long tweets (>200 chars) automatically get LLM-generated headlines for more
efficient analysis; these are cached in the database for reuse.

To heal missing data (backfill headlines and refresh resonance scores):

```bash
# All-in-one: backfill headlines and recalculate resonance scores
bird-rs db repair

# Or backfill headlines only with custom options
bird-rs db backfill-headlines --max-tweets 100 --min-length 200
```

## Resonance Scores

Bird tracks which tweets resonated most with you using a **synergistic resonance
formula** that accounts for both passive and active interactions. Interactions
compound multiplicatively, rewarding tweets with multiple signals of engagement.

### Formula

The score is calculated as: **base × active_multiplier × synergy_multiplier**

- **Base** (passive interactions): 1.0 + like×0.25 + bookmark×1.0
- **Active multiplier** (engagement): 1.0 + reply×0.5 + quote×0.75 + retweet×0.8
- **Synergy multiplier**: 1.5 if both liked AND bookmarked, else 1.0

### Examples

- Just liked: 1.25
- Just bookmarked: 2.0
- Liked + bookmarked: 3.375 (2.7× higher due to synergy)
- Liked + bookmarked + 1 reply: 5.06 (interactions compound)

### Usage

First, compute resonance scores from your synced data:

```bash
bird-rs resonance refresh
```

Then view scores across all collections:

```bash
# Show all tweets with scores and interaction counts
bird-rs list --columns id,text,liked,bookmarked,score

# View tweets from a specific collection with scores
bird-rs list likes --columns id,text,score

# Include collection membership (which collections contain each tweet)
bird-rs list --columns id,text,collections,score
```

### Available Columns

- `id` — Tweet ID
- `text` — Tweet text
- `time` — Tweet creation time
- `author` — Author name
- `liked` — Whether you liked it (Yes/No)
- `bookmarked` — Whether you bookmarked it (Yes/No)
- `score` — Resonance score (computed from interactions)
- `headline` — LLM-generated headline (for tweets >200 chars)
- `collections` — Which collections contain this tweet (❤️ for likes, 🔖 for
  bookmarks, 📝 for your posts)

### Keeping Scores Up-to-Date

Scores are cached locally. After syncing new tweets:

```bash
bird-rs resonance refresh
```

To backfill missing headlines and recalculate all scores in one command:

```bash
bird-rs db repair
```

Resonance scores run fully offline and require no external API calls.

## Storage

By default, Bird uses an embedded SurrealDB (RocksDB) database at
`~/.bird/bird.db`. To use a remote SurrealDB (including Surreal Cloud), pass a
remote endpoint and optional credentials:

```bash
bird-rs sync likes \
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
bird-rs --storage memory sync likes
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
bird-rs config init
# Overwrite existing file
bird-rs config init --force
```

## Authentication

Bird auto-extracts cookies from Safari on macOS. Alternatively:

```bash
# Environment variables
export AUTH_TOKEN=your_auth_token
export CT0=your_ct0_token

# Or CLI flags
bird-rs --auth-token TOKEN --ct0 CT0 whoami
```

## Technical Notes

### Query ID Discovery

Twitter rotates GraphQL query IDs periodically, which can break API clients.
Bird automatically discovers fresh IDs by scraping Twitter's JavaScript bundles.
This is transparent and requires no manual intervention.

- Cache: `~/.bird/query-ids-cache.json` (24-hour TTL)
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
