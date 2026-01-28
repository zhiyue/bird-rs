# bird-client

Twitter/X GraphQL API client with pagination and rate limiting.

## Features

- Read tweets by ID
- Fetch likes, bookmarks, user tweets, timeline
- Cursor-based pagination
- Rate limiting with exponential backoff
- Cookie-based authentication

## Usage

```rust
use bird_client::{TwitterClient, TwitterClientOptions, RateLimitConfig};
use bird_client::cookies::resolve_credentials;

let cookies = resolve_credentials(None, None, &[])?;
let client = TwitterClient::new(TwitterClientOptions {
    cookies,
    timeout_ms: Some(30000),
    quote_depth: Some(1),
});

// Fetch a tweet
let tweet = client.get_tweet("1234567890").await?;

// Fetch likes with rate limiting
let config = RateLimitConfig::with_delay(1000); // 1s between pages
let likes = client
    .get_all_likes_with_rate_limit(&user_id, Some(10), &config)
    .await?;
```

## Rate Limiting

The `RateLimitConfig` controls request pacing:

```rust
RateLimitConfig {
    delay_ms: 1000,           // Delay between pages
    max_retries: 4,           // Retries on 429
    initial_backoff_ms: 1000, // Starting backoff
    max_backoff_ms: 16000,    // Backoff cap
}
```

## Authentication

Cookies are extracted via [sweet-cookie] (Safari on macOS) or provided manually:

```rust
let cookies = resolve_credentials(
    Some("auth_token_value"),
    Some("ct0_value"),
    &[], // browser hints
)?;
```

## Dependencies

- [bird-core] — Shared types (`TweetData`, `PaginationOptions`, etc.)

[bird-core]: ../bird-core
[sweet-cookie]: https://github.com/nicholasio/sweet-cookie-rs
