# Storage & Sync Engine Options for Bird

This document explores different storage providers and sync engines for bird-rs,
focusing on local-first architectures with SQLite.

## Current Architecture

Bird currently uses **SurrealDB** with a local RocksDB backend
(`~/.bird/bird.db`). The storage trait is already abstracted, making it feasible
to add new backends.

---

## Storage Provider Comparison

### 1. Turso/libsql (Recommended for Local-First)

**What it is:** A fork of SQLite that adds native replication and sync
capabilities.

**Key Features:**

- **Embedded Replicas**: Local SQLite file that syncs with remote Turso Cloud
- **Write offline, sync later**: Full offline support with automatic
  reconciliation
- **Multi-platform**: Rust, JavaScript, Python, Go, Swift, Kotlin, Flutter, WASM
- **Browser support**: Runs in browser via WebAssembly + OPFS
- **Vector search**: Built-in similarity search (useful for future semantic
  features)

**Sync Model:**

- Primary database in Turso Cloud
- Embedded replicas on each device
- Automatic sync on demand or periodic
- Conflict resolution via last-write-wins (timestamps)

**Pros:**

- Native Rust support (`libsql` crate)
- Drop-in SQLite replacement
- Battle-tested at scale
- Free tier available
- Excellent DX with `turso` CLI

**Cons:**

- Vendor lock-in to Turso Cloud (though libsql is open source)
- No CRDT-based conflict resolution (LWW only)

**Best for:** Bird use case - personal data sync where conflicts are rare.

---

### 2. cr-sqlite (Best Conflict Resolution)

**What it is:** SQLite extension that adds CRDT-based conflict-free replication.

**Key Features:**

- **Multi-master replication**: Any node can accept writes independently
- **CRDT column types**: LWW, Counters, Peritext (collaborative text)
- **Bidirectional sync**: Any peer can sync with any other
- **History-free**: No history kept, only current state

**Sync Model:**

```sql
-- Enable CRDT on tables
SELECT crsql_as_crr('tweet');

-- Extract changes since version X
SELECT * FROM crsql_changes WHERE db_version > X;

-- Apply remote changes
INSERT INTO crsql_changes VALUES (...);
```

**Pros:**

- True peer-to-peer sync (no central server required)
- Automatic conflict resolution
- Works with any SQLite database
- Open source (MIT)

**Cons:**

- Less mature than Turso
- Requires manual sync orchestration
- Need to self-host sync server or use peer-to-peer

**Best for:** Collaborative apps, multi-device sync with potential conflicts.

---

### 3. Electric SQL

**What it is:** Postgres-to-SQLite sync layer with real-time streaming.

**Key Features:**

- Real-time sync from Postgres to local SQLite
- Partial replication (sync only what you need)
- Works with existing Postgres databases

**Sync Model:**

- Server-side Postgres as source of truth
- Client-side SQLite as read replica with write-back
- Shape-based partial sync

**Pros:**

- Great for apps with existing Postgres backend
- Partial sync reduces data transfer
- Real-time updates

**Cons:**

- Requires Postgres (not pure SQLite)
- More complex architecture
- Not ideal for offline-first

**Best for:** Apps with central Postgres that need local SQLite caching.

---

### 4. PowerSync

**What it is:** Managed sync service connecting backend databases to client
SQLite.

**Key Features:**

- Supports Postgres, MongoDB, MySQL, SQL Server as backends
- Client SDKs for Dart/Flutter, React Native, JavaScript, Swift, Kotlin
- Managed infrastructure

**Sync Model:**

- Backend database → PowerSync Service → Client SQLite
- Rules-based data sync
- Offline-first with sync on reconnect

**Pros:**

- Multi-backend support
- Managed service (no infrastructure to run)
- Great mobile SDKs

**Cons:**

- No native Rust SDK yet
- Managed service cost
- Another dependency

**Best for:** Mobile-first apps with existing backend databases.

---

### 5. Cloudflare D1

**What it is:** Serverless SQLite database on Cloudflare's edge network.

**Key Features:**

- Managed SQLite with read replicas
- HTTP API access
- Integrated with Cloudflare Workers

**Sync Model:**

- Primary instance handles writes
- Read replicas for distributed reads
- Sessions API for sequential consistency

**Limitations:**

- No client-side SQLite sync
- Server-only (via Workers or HTTP)
- 10GB database limit

**Best for:** Server-side data storage, not local-first apps.

---

### 6. Cloudflare R2 (Object Storage)

**What it is:** S3-compatible object storage.

**Use for Bird:**

- Store SQLite database file as an object
- Sync entire database file periodically
- Simple but not incremental

**Limitations:**

- Full file sync (not incremental)
- No built-in conflict resolution
- Manual sync logic required

**Best for:** Backup/archive, not real-time sync.

---

### 7. Cloudflare Durable Objects

**What it is:** Stateful serverless compute with SQLite storage.

**Key Features:**

- SQLite storage per Durable Object
- Strict serializability
- Code and data colocated

**Use for Bird:**

- Per-user database instances
- Real-time sync coordinator
- Could coordinate cr-sqlite peers

**Limitations:**

- Workers-only access (no direct client access)
- More infrastructure to manage
- No built-in client sync

**Best for:** Server-side sync coordinator, not direct local-first.

---

## Recommended Architecture for Bird

### Option A: Turso/libsql (Simplest)

```
┌─────────────────┐         ┌─────────────────┐
│   Local Device  │         │  Turso Cloud    │
│                 │  sync   │                 │
│  libsql (local) │◄───────►│  libsql (remote)│
│  ~/.bird/bird.db│         │                 │
└─────────────────┘         └─────────────────┘
```

**Implementation:**

1. Replace SurrealDB with libsql
2. Configure embedded replica pointing to Turso
3. Sync happens automatically

**Migration Path:**

- Export current SurrealDB data
- Create libsql schema (similar to current)
- Import data into libsql
- Add Turso cloud sync

---

### Option B: cr-sqlite + Cloudflare (Most Flexible)

```
┌─────────────────┐         ┌─────────────────┐
│   Local Device  │  HTTP   │ Cloudflare DO   │
│                 │  sync   │                 │
│  SQLite +       │◄───────►│ Sync Coordinator│
│  cr-sqlite ext  │         │ + SQLite        │
└─────────────────┘         └─────────────────┘
                                    │
                                    │ backup
                                    ▼
                            ┌─────────────────┐
                            │ Cloudflare R2   │
                            │ (DB backups)    │
                            └─────────────────┘
```

**Implementation:**

1. Add cr-sqlite extension to local SQLite
2. Deploy Durable Object as sync coordinator
3. Implement sync protocol over HTTP/WebSocket
4. Periodic backup to R2

---

### Option C: Hybrid (Best of Both)

```
┌─────────────────┐         ┌─────────────────┐
│   Local Device  │         │  Turso Cloud    │
│                 │  sync   │                 │
│  libsql +       │◄───────►│  Primary DB     │
│  cr-sqlite      │         │                 │
│  (for offline)  │         │                 │
└─────────────────┘         └─────────────────┘
                                    │
                                    │ replicate
                                    ▼
                            ┌─────────────────┐
                            │ Cloudflare D1   │
                            │ (Read replica   │
                            │  for web UI)    │
                            └─────────────────┘
```

---

## Implementation Roadmap

### Phase 1: libsql Migration

1. Add `libsql` crate to dependencies
2. Create `LibSqlStorage` implementing `Storage` trait
3. Schema migration from SurrealDB
4. Test local-only operation

### Phase 2: Turso Cloud Sync

1. Set up Turso account and database
2. Configure embedded replica
3. Add sync commands to CLI
4. Handle sync conflicts (LWW)

### Phase 3: Multi-Device

1. Test sync across multiple devices
2. Add sync status UI in TUI
3. Handle offline/online transitions

### Phase 4: Advanced (Optional)

1. Add cr-sqlite for better conflict resolution
2. Deploy sync coordinator on Cloudflare
3. Build web UI with D1 read replica

---

## Quick Comparison Table

| Feature             | Turso/libsql | cr-sqlite | Electric | PowerSync | D1   |
| ------------------- | ------------ | --------- | -------- | --------- | ---- |
| Local SQLite        | ✅           | ✅        | ✅       | ✅        | ❌   |
| Offline-first       | ✅           | ✅        | ⚠️       | ✅        | ❌   |
| Conflict resolution | LWW          | CRDT      | Custom   | Custom    | N/A  |
| Rust SDK            | ✅           | ✅        | ❌       | ❌        | ❌   |
| Managed service     | ✅           | ❌        | ✅       | ✅        | ✅   |
| Self-hostable       | ✅           | ✅        | ✅       | ❌        | ❌   |
| Complexity          | Low          | Medium    | Medium   | Low       | Low  |
| Best for Bird       | ⭐⭐⭐⭐⭐   | ⭐⭐⭐⭐  | ⭐⭐     | ⭐⭐⭐    | ⭐⭐ |

---

## Recommendation

For bird-rs, I recommend **Turso/libsql** as the primary sync solution because:

1. **Native Rust support** - First-class libsql crate
2. **Simple architecture** - Embedded replicas "just work"
3. **Perfect fit** - Personal data sync with rare conflicts
4. **Free tier** - Good for personal use
5. **Future-proof** - libsql is open source, can self-host

If conflict resolution becomes critical later, cr-sqlite can be layered on top.

---

## Resources

- [libsql Rust SDK](https://github.com/tursodatabase/libsql)
- [Turso Documentation](https://docs.turso.tech/)
- [cr-sqlite](https://vlcn.io/)
- [Electric SQL](https://electric-sql.com/)
- [PowerSync](https://docs.powersync.com/)
- [Cloudflare D1](https://developers.cloudflare.com/d1/)
