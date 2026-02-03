# Following Sync Implementation Plan

This document outlines the plan to implement syncing of accounts you follow,
similar to how likes and bookmarks are synced.

## Overview

The `bird sync following` command will fetch and persist the list of Twitter
accounts you follow. This differs from likes/bookmarks in that it syncs
**users** rather than **tweets**.

## Current State

### Existing Database Schema

```
Tables:
├── tweet                 - Tweet data (id, text, author, etc.)
├── tweet_collection      - Association: tweet_id + collection + user_id
├── sync_state           - Sync progress per collection
└── twitter_user         - Mentioned users (limited fields)
```

The `twitter_user` table currently stores:

- `user_id` (primary key)
- `username`
- `username_lower` (indexed, for case-insensitive lookup)
- `name`
- `updated_at`

### Existing Types

`TwitterUser` in `bird-core/src/types.rs` already has the full user profile:

```rust
pub struct TwitterUser {
    pub id: String,
    pub username: String,
    pub name: String,
    pub description: Option<String>,
    pub followers_count: Option<u64>,
    pub following_count: Option<u64>,
    pub is_blue_verified: Option<bool>,
    pub profile_image_url: Option<String>,
    pub created_at: Option<String>,
}
```

### Existing API Support

- `Operation::Following` is defined in `bird-client/src/constants.rs`
- `FollowingResult` enum exists in `bird-core/src/types.rs`
- No `get_following()` method implemented yet

---

## Migration Strategy

### Phase 1: Extend `twitter_user` Table (Non-Destructive)

The existing `twitter_user` table will be extended with new nullable fields.
This is safe for existing databases:

**New fields to add:**

```
description: Option<String>
followers_count: Option<u64>
following_count: Option<u64>
is_blue_verified: Option<bool>
profile_image_url: Option<String>
account_created_at: Option<String>  // renamed from created_at to avoid confusion
```

**Migration approach:**

- SurrealDB is schemaless - new fields can be added without explicit migration
- Existing records will have `NONE` for new fields (acceptable)
- Update `init_schema()` to document expected fields (no schema enforcement
  needed)

### Phase 2: New `user_collection` Table

Create a new association table for user collections (following, followers,
etc.):

```
user_collection:
├── target_user_id: String     - The user being followed
├── collection: String         - "following" or "followers"
├── owner_user_id: String      - The authenticated user
├── added_at: DateTime<Utc>    - When discovered
```

**Indexes:**

```sql
DEFINE INDEX user_collection_pk ON user_collection
  FIELDS target_user_id, collection, owner_user_id UNIQUE

DEFINE INDEX user_collection_lookup ON user_collection
  FIELDS collection, owner_user_id
```

### Phase 3: Sync State Reuse

The existing `sync_state` table can be reused with `collection = "following"`:

- `newest_item_id` → most recently followed user ID
- `oldest_item_id` → oldest followed user ID
- `backfill_cursor` → pagination cursor
- `has_more_history` → whether backfill is complete
- `total_synced` → count of users synced

---

## Implementation Tasks

### 1. API Client (`bird-client`)

**File: `crates/bird-client/src/operations/following.rs`** (new file)

Implement the GraphQL API call:

- GraphQL endpoint: `Following`
- Query ID fallback: `BEkNpEt5pNETESoqMsTEGA`
- Variables: `userId`, `count`, `cursor`
- Features: `buildFollowingFeatures()` (port from TS)
- REST fallback: `/1.1/friends/list.json`

**File: `crates/bird-client/src/client.rs`**

Add methods:

```rust
pub async fn get_following(
    &self,
    user_id: &str,
    count: u32,
    cursor: Option<&str>,
) -> Result<FollowingResult>

pub async fn get_following_paginated(
    &self,
    user_id: &str,
    options: &PaginationOptions,
) -> Result<PaginatedResult<TwitterUser>>
```

### 2. Storage Traits (`bird-core`)

**File: `crates/bird-core/src/storage.rs`**

Extend `UserStore` trait:

```rust
/// Insert or update a full Twitter user profile.
async fn upsert_user(&self, user: &TwitterUser) -> Result<()>;

/// Insert or update multiple users. Returns count of new users.
async fn upsert_users(&self, users: &[TwitterUser]) -> Result<usize>;

/// Add a user to a collection (following, followers).
async fn add_user_to_collection(
    &self,
    target_user_id: &str,
    collection: &str,
    owner_user_id: &str,
) -> Result<()>;

/// Check if a user is in a collection.
async fn is_user_in_collection(
    &self,
    target_user_id: &str,
    collection: &str,
    owner_user_id: &str,
) -> Result<bool>;

/// Get users from a collection.
async fn get_users_by_collection(
    &self,
    collection: &str,
    owner_user_id: &str,
    limit: Option<u32>,
    offset: Option<u32>,
) -> Result<Vec<TwitterUser>>;

/// Get count of users in a collection.
async fn user_collection_count(
    &self,
    collection: &str,
    owner_user_id: &str
) -> Result<u64>;
```

### 3. Storage Implementation (`bird-storage`)

**File: `crates/bird-storage/src/surrealdb.rs`**

- Update `TwitterUserRecord` to include all fields
- Add `UserCollectionRecord` struct
- Update `init_schema()` to create `user_collection` table and indexes
- Implement new `UserStore` methods

### 4. Sync Engine

**Option A: Extend existing `SyncEngine`**

- Add `Collection::Following` variant
- Generalize sync logic to handle both tweets and users
- More complex but maintains single sync abstraction

**Option B: Create `UserSyncEngine`** (Recommended)

- Separate sync engine for user collections
- Simpler, more focused implementation
- Parallel structure to tweet sync

**File: `crates/bird-core/src/user_sync_engine.rs`** (new file)

```rust
pub struct UserSyncEngine<S: Storage> {
    client: Arc<TwitterClient>,
    storage: Arc<S>,
    user_id: String,
    options: UserSyncOptions,
}

impl<S: Storage> UserSyncEngine<S> {
    pub async fn sync_following(&self) -> Result<UserSyncResult>;
}
```

### 5. CLI Commands

**File: `crates/bird-cli/src/commands/sync.rs`**

Add new sync action:

```rust
SyncAction::Following {
    #[arg(long)]
    full: bool,
    #[arg(long, default_value = "10")]
    max_pages: Option<u32>,
    #[arg(long)]
    delay: Option<u64>,
}
```

Update `run_sync()` to handle `SyncAction::Following`.

### 6. Types Update

**File: `crates/bird-core/src/types.rs`**

Add user collection enum:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum UserCollection {
    Following,
    Followers,
}

impl UserCollection {
    pub fn as_str(&self) -> &'static str {
        match self {
            UserCollection::Following => "following",
            UserCollection::Followers => "followers",
        }
    }
}
```

---

## API Reference (from TypeScript)

### GraphQL Request

```
Endpoint: https://api.twitter.com/graphql/{queryId}/Following
Method: GET
```

**Variables:**

```json
{
  "userId": "12345",
  "count": 20,
  "includePromotedContent": false,
  "cursor": "optional-cursor-string"
}
```

**Features:** (abbreviated, see TS source for full list)

```json
{
  "rweb_video_screen_enabled": true,
  "responsive_web_graphql_timeline_navigation_enabled": true,
  "premium_content_api_read_enabled": true,
  ...
}
```

### Response Parsing

Path: `data.user.result.timeline.timeline.instructions`

Look for entries with:

- `type: "TimelineAddEntries"`
- `content.itemContent.user_results.result` → User data
- `content.cursorType: "Bottom"` → Next page cursor

### REST Fallback

```
Endpoint: https://api.twitter.com/1.1/friends/list.json
Method: GET
Params: user_id, count, cursor, skip_status=true, include_user_entities=false
```

---

## Testing Plan

1. **Unit tests** for user parsing from GraphQL response
2. **Integration tests** for storage operations
3. **Manual testing:**
   - `bird sync following` - initial sync
   - `bird sync following` - incremental sync
   - `bird sync following --full` - full re-sync
   - `bird sync status` - verify following appears in status

---

## File Changes Summary

| File                                      | Change Type | Description                                                      |
| ----------------------------------------- | ----------- | ---------------------------------------------------------------- |
| `bird-core/src/types.rs`                  | Modify      | Add `UserCollection` enum, `AuthorStats` struct                  |
| `bird-core/src/storage.rs`                | Modify      | Extend `UserStore` and `TweetStore` traits                       |
| `bird-client/src/operations/following.rs` | New         | Following API implementation                                     |
| `bird-client/src/operations/mod.rs`       | Modify      | Export following module                                          |
| `bird-client/src/client.rs`               | Modify      | Add `get_following()` methods                                    |
| `bird-client/src/features.rs`             | Modify      | Add `following_features()`                                       |
| `bird-storage/src/surrealdb.rs`           | Modify      | Extend schema (+ `author_id` index), implement new methods       |
| `bird-core/src/user_sync_engine.rs`       | New         | User sync engine                                                 |
| `bird-core/src/lib.rs`                    | Modify      | Export user sync engine                                          |
| `bird-cli/src/cli.rs`                     | Modify      | Add `SyncAction::Following`, `--author`/`--from-following` flags |
| `bird-cli/src/commands/sync.rs`           | Modify      | Implement following sync command                                 |
| `bird-cli/src/commands/insights.rs`       | Modify      | Add `authors` subcommand                                         |

---

## Risk Assessment

| Risk                                                 | Mitigation                                                   |
| ---------------------------------------------------- | ------------------------------------------------------------ |
| Existing DBs have `twitter_user` with limited fields | Schemaless DB - new fields are nullable, no migration needed |
| Rate limiting on Following API                       | Reuse existing rate limit infrastructure                     |
| Large following lists (1000s of users)               | Pagination + incremental sync                                |
| Query ID rotation                                    | Reuse existing query ID refresh mechanism                    |

---

## Rollback Plan

If issues arise:

1. The new `user_collection` table is isolated - can be dropped without
   affecting tweets
2. New fields on `twitter_user` are nullable - existing code ignores them
3. Sync state for "following" is separate from other collections

---

## Cross-Collection Queries

Once following is synced, powerful cross-collection queries become possible.
This enables answering questions like:

- "How many tweets have I bookmarked from @elonmusk?"
- "Show me liked tweets from accounts I follow"
- "Which authors do I engage with most?"

### Required Index (Migration)

**Add index on `tweet.author_id`** for efficient author-based queries:

```sql
DEFINE INDEX IF NOT EXISTS tweet_author_id ON tweet FIELDS author_id
```

This is a non-destructive addition to `init_schema()`. Existing databases will
build the index on first run (may take a few seconds for large datasets).

### Example Queries

**Bookmarks from a specific author:**

```sql
SELECT * FROM tweet
WHERE author_id = $author_id
AND tweet_id IN (
    SELECT tweet_id FROM tweet_collection
    WHERE collection = 'bookmarks' AND user_id = $user_id
)
```

**Tweets from accounts I follow (any collection):**

```sql
LET $following_ids = (
    SELECT target_user_id FROM user_collection
    WHERE collection = 'following' AND owner_user_id = $user_id
);

SELECT * FROM tweet
WHERE author_id IN $following_ids
AND tweet_id IN (
    SELECT tweet_id FROM tweet_collection
    WHERE user_id = $user_id
)
```

**Author engagement stats:**

```sql
SELECT
    author_id,
    author_username,
    count() as tweet_count,
    array::group(collection) as collections
FROM tweet_collection
JOIN tweet ON tweet_collection.tweet_id = tweet.tweet_id
WHERE tweet_collection.user_id = $user_id
GROUP BY author_id, author_username
ORDER BY tweet_count DESC
LIMIT 20
```

### New Storage Methods

Add to `TweetStore` trait:

```rust
/// Get tweets from a collection filtered by author.
async fn get_tweets_by_collection_and_author(
    &self,
    collection: &str,
    user_id: &str,
    author_id: &str,
    limit: Option<u32>,
) -> Result<Vec<TweetData>>;

/// Get author engagement statistics across collections.
async fn get_author_stats(
    &self,
    user_id: &str,
    limit: Option<u32>,
) -> Result<Vec<AuthorStats>>;

/// Get tweets from collections filtered to only authors you follow.
async fn get_tweets_from_following(
    &self,
    collections: &[&str],
    user_id: &str,
    limit: Option<u32>,
    offset: Option<u32>,
) -> Result<Vec<TweetWithCollections>>;
```

**New type:**

```rust
pub struct AuthorStats {
    pub author_id: String,
    pub author_username: String,
    pub author_name: String,
    pub total_tweets: u64,
    pub liked_count: u64,
    pub bookmarked_count: u64,
    pub is_following: bool,
}
```

### CLI Integration

New command options:

```bash
# Bookmarks from a specific author
bird bookmarks --author elonmusk

# Likes from accounts you follow
bird likes --from-following

# Author engagement leaderboard
bird insights authors
```

---

## Estimated Effort

| Task                      | Complexity |
| ------------------------- | ---------- |
| API client implementation | Medium     |
| Storage trait extension   | Low        |
| SurrealDB implementation  | Medium     |
| User sync engine          | Medium     |
| CLI integration           | Low        |
| Cross-collection queries  | Medium     |
| Author stats & insights   | Low        |
| Testing                   | Medium     |

**Total:** ~3-4 days of focused work

---

## Implementation Phases

**Phase 1: Core Following Sync** (MVP)

- API client, storage, sync engine, basic CLI
- Enables `bird sync following`

**Phase 2: Cross-Collection Queries**

- Add `author_id` index
- Implement `--author` and `--from-following` filters
- Add `AuthorStats` queries

**Phase 3: Insights Integration**

- `bird insights authors` command
- Integration with existing resonance scoring
