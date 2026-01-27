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

| Command            | Description               |
| ------------------ | ------------------------- |
| `bird <id>`        | Read a tweet by ID or URL |
| `bird whoami`      | Show logged-in account    |
| `bird likes`       | Fetch your liked tweets   |
| `bird bookmarks`   | Fetch your bookmarks      |
| `bird sync likes`  | Sync likes to local DB    |
| `bird sync status` | Show sync progress        |

## Sync

Bird stores tweets in a local [SurrealDB] database (`~/.bird/bird.db`) for
offline access and incremental sync.

```bash
# Initial sync (fetches 10 pages by default)
bird sync likes

# Continue fetching older tweets
bird sync backfill likes

# Check progress
bird sync status
```

The sync is **bidirectional**: it catches up on new likes and can backfill your
full history over multiple sessions.

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

## License

MIT

[SurrealDB]: https://surrealdb.com
[bird-cli]: crates/bird-cli
[bird-client]: crates/bird-client
[bird-storage]: crates/bird-storage
[bird-core]: crates/bird-core
