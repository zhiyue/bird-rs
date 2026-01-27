# bird-rs

A fast X/Twitter CLI for reading tweets, powered by GraphQL. Written in Rust.

## Features

- Read tweets by ID or URL
- Cookie-based authentication via Safari (macOS)
- JSON output support
- Colored terminal output

## Installation

```bash
cargo install --path .
```

## Usage

### Check Authentication

```bash
# Show available credential sources
bird check

# Show logged-in account
bird whoami
```

### Read Tweets

```bash
# Read a tweet by ID
bird read 1234567890123456789

# Read a tweet by URL
bird read https://x.com/user/status/1234567890123456789

# Shorthand (without 'read' command)
bird 1234567890123456789

# Output as JSON
bird read 1234567890123456789 --json
```

### Authentication

The CLI automatically extracts cookies from Safari on macOS. Alternatively, you can provide credentials via:

1. **Environment variables:**

   ```bash
   export AUTH_TOKEN=your_auth_token
   export CT0=your_ct0_token
   ```

2. **CLI flags:**

   ```bash
   bird --auth-token YOUR_TOKEN --ct0 YOUR_CT0 whoami
   ```

## Development

### Prerequisites

- Rust 1.70+
- pre-commit (for git hooks)
- dprint (for markdown formatting)

### Setup

```bash
# Install pre-commit hooks
pre-commit install

# Build
cargo build

# Run tests
cargo test

# Run clippy
cargo clippy --all-targets --all-features -- -D warnings

# Format code
cargo fmt
```

## License

MIT
