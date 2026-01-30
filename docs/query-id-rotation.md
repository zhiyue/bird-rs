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

**Current impact:** Our likes backfill is blocked because both of our hardcoded query IDs for the Likes operation are now stale.

## Current Implementation

We use static fallback query IDs defined in `crates/bird-client/src/constants.rs`:

```rust
Operation::Likes => &["ETJflBunfqNa1uE1mBPCaw", "JR2gceKucIKcVNB_9JkhsA"],
Operation::Bookmarks => &["RV1g3b8n_SGOHwkqKYSCFw", "tmd4ifV8RHltzn8ymGg1aw"],
// etc.
```

Operations try each ID in sequence until one works. When all IDs are stale, the operation fails.

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

## Recommendation

**Short-term:** Implement **Option C** (CLI refresh command) as a quick fix.

**Long-term:** Implement **Option B** (dynamic discovery) for self-healing.

The hybrid approach lets us ship a fix quickly while working on the robust solution.

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
- [ ] Implement CLI refresh command (Option C)
- [ ] Implement dynamic discovery (Option B)
- [ ] Add 404 detection and auto-refresh
