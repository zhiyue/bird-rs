# Twitter GraphQL Query ID Rotation

## Context

Twitter's web client uses a GraphQL API at `x.com/i/api/graphql/{queryId}/{OperationName}`. Each operation (Likes, Bookmarks, TweetDetail, etc.) requires a specific query ID that Twitter rotates periodically.

## The Problem

When Twitter rotates query IDs, requests with stale IDs fail with:
- HTTP 404
- `"Query: Unspecified"` error in the response

This breaks API clients until they're updated with fresh IDs.

**Example error:**
```
Error: Twitter API error: Query: Unspecified
```

## Solution: Dynamic Query ID Discovery

Bird implements automatic query ID discovery by scraping Twitter's JavaScript bundles at runtime. This is a self-healing system that recovers from ID rotation without manual intervention.

### How It Works

1. **Cache Check**: First checks disk cache at `~/.bird/query-ids-cache.json` (24-hour TTL)
2. **Discovery**: If cache is stale or missing, fetches X.com pages and extracts JS bundle URLs
3. **Extraction**: Downloads bundles and uses regex patterns to extract query IDs
4. **Caching**: Saves discovered IDs to disk and memory for future use
5. **Fallback**: If discovery fails, falls back to static IDs in source code

### Implementation

The discovery system is in `crates/bird-client/src/query_ids.rs`:

```rust
// Query ID manager handles discovery and caching
pub struct QueryIdManager {
    client: Client,
    cache: Arc<RwLock<QueryIdCache>>,
    fallbacks: HashMap<String, Vec<String>>,
}

impl QueryIdManager {
    /// Get all query IDs to try for an operation (cached + fallbacks)
    pub async fn get_all(&self, operation: &str) -> Vec<String>;

    /// Force refresh from Twitter's JS bundles
    pub async fn refresh(&self) -> Result<(), QueryIdError>;
}
```

### Auto-Refresh on Errors

Operations like `fetch_likes` automatically refresh query IDs when they encounter stale ID errors:

```rust
match self.fetch_likes_with_ids(user_id, options).await {
    Ok(result) => Ok(result),
    Err(e) => {
        // If query ID error, try refreshing and retrying
        let should_refresh = matches!(&e, Error::ApiError(msg)
            if msg.contains("Query: Unspecified") || msg.contains("All query IDs failed"));

        if should_refresh {
            if self.query_id_manager.refresh().await.is_ok() {
                return self.fetch_likes_with_ids(user_id, options).await;
            }
        }
        Err(e)
    }
}
```

## Static Fallbacks

As a last resort, static fallback query IDs are defined in `crates/bird-client/src/constants.rs`:

```rust
Operation::Likes => &[
    "fuBEtiFu3uQFuPDTsv4bfg", // Discovered 2026-01-30
    "ETJflBunfqNa1uE1mBPCaw",
    "JR2gceKucIKcVNB_9JkhsA",
],
// etc.
```

Operations try each ID in sequence until one works.

## How steipete/bird Solves This

The TypeScript [bird](https://github.com/steipete/bird) project implements a 3-layer system:

### Layer 1: Static Fallbacks
Hard-coded IDs in source code as last resort.

### Layer 2: Runtime Cache
- Disk cache: `~/.config/bird/query-ids-cache.json`
- Memory cache for fast access
- 24-hour TTL

### Layer 3: Dynamic Discovery
When a 404 is detected:
1. Fetch X.com pages (homepage, /explore, /notifications, /settings/profile)
2. Extract `<script src="...">` URLs pointing to `abs.twimg.com/responsive-web/client-web/*.js`
3. Download JS bundles (6 in parallel)
4. Extract query IDs using regex patterns:
   ```regex
   e\.exports=\{queryId:"([^"]+)",operationName:"([^"]+)"
   ```
5. Cache results to disk + memory
6. Retry the failed request with fresh IDs

## Proposed Solutions

### Option A: Manual Updates (Current)
**Approach:** Periodically update hardcoded IDs manually.

**Pros:**
- Simple implementation
- No runtime network overhead
- No scraping complexity

**Cons:**
- Breaks when IDs rotate
- Requires manual intervention
- Users experience failures until we release updates

**Effort:** Minimal (we already do this)

---

### Option B: Dynamic Discovery (Like steipete/bird)
**Approach:** Scrape Twitter's JS bundles at runtime to extract fresh IDs.

**Pros:**
- Self-healing: automatically recovers from ID rotation
- No manual updates needed
- Works indefinitely (as long as bundle structure is stable)

**Cons:**
- Complex implementation (HTML parsing, JS bundle fetching, regex extraction)
- Runtime network overhead (mitigated by caching)
- Fragile: Twitter could change bundle structure/obfuscation
- May trigger rate limiting if done too frequently

**Effort:** Medium-high (2-4 hours)

**Implementation outline:**
```rust
// crates/bird-client/src/query_ids.rs

pub struct QueryIdCache {
    ids: HashMap<String, String>,
    fetched_at: DateTime<Utc>,
    ttl: Duration,
}

impl QueryIdCache {
    /// Load from disk or fetch fresh
    pub async fn load() -> Result<Self>;

    /// Get ID for operation, refreshing if stale
    pub async fn get(&mut self, operation: &str) -> Result<String>;

    /// Force refresh from X.com bundles
    pub async fn refresh(&mut self) -> Result<()>;
}

// Discovery flow
async fn discover_query_ids() -> Result<HashMap<String, String>> {
    let pages = ["https://x.com/?lang=en", "https://x.com/explore", ...];
    let bundle_urls = extract_bundle_urls(&pages).await?;
    let bundles = fetch_bundles(&bundle_urls).await?;
    extract_ids_from_bundles(&bundles)
}
```

---

### Option C: Hybrid with CLI Refresh Command
**Approach:** Add a `bird query-ids refresh` command that users can run manually.

**Pros:**
- Simpler than full auto-discovery
- User-controlled (no surprise network requests)
- Can be run in CI/CD to keep IDs fresh

**Cons:**
- Still requires user intervention
- Not self-healing

**Effort:** Low-medium (1-2 hours)

---

### Option D: External Query ID Service
**Approach:** Host a service that scrapes Twitter and provides fresh IDs via API.

**Pros:**
- Centralizes scraping logic
- Clients stay simple
- Can monitor for ID changes

**Cons:**
- Requires hosting infrastructure
- Single point of failure
- Privacy concerns (clients phone home)

**Effort:** High (service development + hosting)

---

## Implementation Notes

**Option B (dynamic discovery) is now implemented.** The system is self-healing and automatically recovers from query ID rotation.

Key files:
- `crates/bird-client/src/query_ids.rs` - Discovery and caching logic
- `crates/bird-client/src/constants.rs` - Static fallback IDs
- `crates/bird-client/src/operations/likes.rs` - Auto-refresh on errors

## Technical Details

### Bundle URL Pattern
```regex
https://abs\.twimg\.com/responsive-web/client-web(?:-legacy)?/[A-Za-z0-9.-]+\.js
```

### Query ID Extraction Patterns

Twitter's minified JS contains exports like:
```javascript
e.exports={queryId:"ETJflBunfqNa1uE1mBPCaw",operationName:"Likes",...}
```

Regex patterns to extract (need multiple due to property order variations):
```regex
// Pattern 1: queryId first
e\.exports=\{queryId\s*:\s*["']([^"']+)["']\s*,\s*operationName\s*:\s*["']([^"']+)["']

// Pattern 2: operationName first
e\.exports=\{operationName\s*:\s*["']([^"']+)["']\s*,\s*queryId\s*:\s*["']([^"']+)["']

// Pattern 3: Loose matching with bounded lookback
queryId\s*[:=]\s*["']([^"']+)["'](.{0,4000}?)operationName\s*[:=]\s*["']([^"']+)["']
```

### Cache File Structure
```json
{
  "fetched_at": "2025-01-30T12:34:56Z",
  "ttl_seconds": 86400,
  "ids": {
    "Likes": "ETJflBunfqNa1uE1mBPCaw",
    "Bookmarks": "RV1g3b8n_SGOHwkqKYSCFw",
    "TweetDetail": "_NvJCnIjOW__EP5-RF197A"
  }
}
```

### Target Operations
```
CreateTweet, TweetDetail, Likes, Bookmarks, BookmarkFolderTimeline,
Following, Followers, UserTweets, SearchTimeline, HomeTimeline
```

## References

- [steipete/bird runtime-query-ids.ts](https://github.com/steipete/bird/blob/main/src/lib/runtime-query-ids.ts)
- [steipete/bird twitter-client-base.ts](https://github.com/steipete/bird/blob/main/src/lib/twitter-client-base.ts)
- [Twitter GraphQL API reverse engineering](https://github.com/fa0311/TwitterInternalAPIDocument)

## Status

- [x] Document the problem
- [x] Implement dynamic discovery (Option B)
- [x] Add 404 detection and auto-refresh
- [x] Disk caching with 24-hour TTL
- [x] Update fallback IDs with discovered values
