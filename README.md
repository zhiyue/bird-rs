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
| `bird db backfill-created-at` | Backfill created_at_ts for stored tweets |
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
