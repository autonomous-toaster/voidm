# voidm v5 Remediation Plan - SIMPLIFIED & ORGANIZED

**Status**: Architecture misaligned. 13 critical gaps + 174 sqlx violations identified.
**Approach**: Fix core stability first (Phases -1 through 8), defer features to Phase 9+.
**Timeline**: 15-18 days to stable multi-backend core. Features added later.

---

## PHASE ORGANIZATION

| Phase | Focus | Duration | Status | Priority |
|-------|-------|----------|--------|----------|
| **-1** | Config override system | 2-3 hours | ✓ DONE | CRITICAL |
| **0** | Generic node/edge format | 3-4 days | NEXT | CRITICAL |
| **1** | Backend abstraction (fix sqlx) | 3-4 days | PLANNED | CRITICAL |
| **2** | Dead code removal | 1 day | PLANNED | HIGH |
| **3** | User-provided type/scope | 1.5 days | PLANNED | HIGH |
| **5** | Chunk/embedding guarantee | 2 days | PLANNED | MEDIUM |
| **6** | Tag system + refresh | 2 days | PLANNED | MEDIUM |
| **4+7** | Config flexibility + routing | 2 days | PLANNED | MEDIUM |
| **8** | Search + cleanup | 1-2 days | PLANNED | LOW |
| **DEFERRED** | Features for Phase 9+ | TBD | BACKLOG | — |

---

# CRITICAL PATH: Phases -1, 0, 1 (BLOCKING ALL OTHER WORK)

---

## Phase -1: Config Override System ✓ DONE

**Status**: COMPLETE in 1.5 hours (ahead of schedule)

**What Was Implemented**:
- Added `Config::load_from(explicit_path)` method
- Added `--config` CLI flag (respects VOIDM_CONFIG env var)
- Created `.voidm.dev.toml` template
- Added to .gitignore

**Usage**:
```bash
voidm --config .voidm.dev.toml add "memory" --type semantic
VOIDM_CONFIG=.voidm.dev.toml voidm search "query"
```

**Safety Guarantee**:
- Local config auto-selected when in project
- Production config untouched
- Can always access production: `--config ~/.config/voidm/config.toml`

**Reference**: TODO-3b8c2561 (completed)

---

## Phase 0: Generic Node/Edge Format (BLOCKING PHASES 1-8)

**Goal**: Implement filemind-style generic node/edge abstraction

**Why This Phase is Critical**:
- Foundation for all other phases
- Enables true backend-agnostic architecture
- Required before Phase 1 backend implementations

### 0.1 Generic SQLite Schema

```sql
CREATE TABLE nodes (
    id TEXT PRIMARY KEY,
    type TEXT NOT NULL,           -- Memory, Chunk, Tag, MemoryType, Scope, Entity, EntityType
    properties TEXT NOT NULL,     -- JSON: all fields
    created_at TIMESTAMP,
    updated_at TIMESTAMP,
    INDEX(type)
);

CREATE TABLE edges (
    id TEXT PRIMARY KEY,
    from_id TEXT NOT NULL,
    edge_type TEXT NOT NULL,      -- ChunkOf, TaggedWith, HasType, InScope, Mentions, RelatedEntity, etc.
    to_id TEXT NOT NULL,
    properties TEXT,              -- JSON: metadata
    created_at TIMESTAMP,
    UNIQUE(from_id, edge_type, to_id),
    FOREIGN KEY(from_id) REFERENCES nodes(id),
    FOREIGN KEY(to_id) REFERENCES nodes(id)
);
```

### 0.2 Chunk Node Structure (CRITICAL)

**Problem**: Current chunks have no ordering. Cannot reconstruct memory from chunks.

**Solution**: Add ordering fields to chunk nodes:
```
NodeType::Chunk {
  id: "chunk:uuid",
  sequence_num: 0,              // Position (0, 1, 2, ...)
  char_start: 0,                // Byte offset in original
  char_end: 2048,               // Byte offset in original
  content: "chunk text...",
  embedding_dim: 384,
}

Edge: Memory -[:HAS_CHUNK]-> Chunk
properties: { sequence_num: 0 }
```

**Why**: Enables proper chunk reconstruction, de-duplication, context linking.

### 0.3 Generic Properties

All nodes use JSON properties. Example:
- Memory: {title, content, type, scope, timestamps}
- Tag: {name, user_created}

### 0.4 Embedding Storage: Backend-Specific

NOT in generic nodes table. Each backend optimizes:
- **SQLite**: `sqlite-vector` (table: node_embeddings)
- **Neo4j**: APOC vector procedures
- **PostgreSQL**: pgvector

DB trait: `store_chunk_embedding()`, `search_by_embedding()`

### 0.5 Implementation Tasks

- [ ] 0.5.1: Migrate nodes table (JSON properties)
- [ ] 0.5.2: Migrate edges table (edge_type + properties)
- [ ] 0.5.3: Chunk nodes (sequence_num, char_start, char_end)
- [ ] 0.5.4: Generic CRUD in DB trait
- [ ] 0.5.5: Test in voidm-sqlite
- [ ] 0.5.6: Verify chunk reconstruction

**Estimated**: 3-4 days

**Outcome**: Generic format working, Neo4j design verified, foundation ready for Phase 1

---

## Phase 1: Backend Abstraction (BLOCKING PHASES 2-8)

**Goal**: Fix 174 sqlx violations. All DB operations route through trait.

**Critical Discovery**: DB trait exists but code bypasses it. 174 sqlx references scattered outside backends.

### Audit: 174 sqlx Violations

| Crate | Count | Must Fix |
|-------|-------|----------|
| voidm-core | 65 | YES (crud.rs, migrate.rs) |
| voidm-graph | 26 | YES (traverse.rs, ops.rs) |
| voidm-cli | 27 | YES (stats, commands) |
| voidm-tagging | 8 | YES |
| voidm-ner | 2 | YES |
| voidm-mcp | 1 | YES |
| voidm-sqlite | 35 | ALLOWED (backend) |
| voidm-postgres | 106 | ALLOWED (backend) |

### 1.1 Extend DB Trait (2-3 hours)

Add missing trait methods:

**Statistics**:
- `get_statistics()` → Statistics struct

**Batch Operations**:
- `batch_insert_memories(mems)`
- `batch_link_memories(links)`

**Transactions**:
- `begin_transaction()` → Transaction struct
- `commit()` / `rollback()`

**Graph Queries**:
- `get_neighbors(id, depth)` → Vec<Node>
- `query_cypher(query)` → QueryResult

**Node/Edge Queries**:
- `count_nodes(type)` → i64
- `count_edges(edge_type)` → i64
- `get_node(id)` → Option<Node>
- `get_edges(from_id, edge_type)` → Vec<Edge>

### 1.2 Fix voidm-core (1 day)

**crud.rs (51)**: Replace SqlitePool → &dyn Database
**migrate.rs (11)**: Move OUT of core, use db.ensure_schema()
**models.rs, search.rs (4)**: Remove sqlx, use traits

### 1.3 Fix voidm-graph (0.5-1 day)

traverse.rs, ops.rs, cypher/mod.rs: Replace sqlx → db trait calls

### 1.4 Fix voidm-cli (0.5-1 day)

stats.rs, graph.rs, commands: Use db trait for all operations

### 1.5 Fix voidm-tagging, voidm-ner, voidm-mcp (0.5-1 day)

Replace all sqlx with trait methods

### 1.6 Implement in Backends (0.5-1 day)

voidm-sqlite, voidm-postgres: Implement all trait methods

### 1.7 Validation (0.5-1 day)

- [ ] Zero sqlx outside voidm-sqlite/postgres
- [ ] All CRUD uses Database trait
- [ ] All tests pass
- [ ] Neo4j backend design verified

**Estimated**: 2.5-3.5 days (aggressive)

**Outcome**:
- ✓ Zero sqlx violations in non-backend code
- ✓ All operations route through DB trait
- ✓ Neo4j backend now implementable
- ✓ True backend-agnostic architecture

---

# QUALITY PHASES: Phases 2, 3, 5, 6 (PARALLEL EXECUTION)

---

## Phase 2: Dead Code Removal

**Goal**: Remove Concept system (being replaced by Tags)

### 2.1 Remove Concept Code
- Delete Concept nodes, ConceptRecord, constraints
- **Estimated**: 4 hours

### 2.2 Fix Optional Features
- Disable NER feature flag (broken, creates non-existent Concepts)
- Temporarily disable tinyllama
- **Estimated**: 1 hour

---

## Phase 3: First-Class Citizens (User-Provided Only)

**Goal**: MemoryType and Scope nodes (user-provided, never automatic)

### 3.1 MemoryType Nodes (1h)
- NodeType::MemoryType (Episodic, Semantic, etc.)
- Memory -[:HAS_TYPE]-> MemoryType edges

### 3.2 Scope Nodes (1h)
- NodeType::Scope (project/auth, etc.)
- Memory -[:IN_SCOPE]-> Scope edges

### 3.3 Trait Methods (1h)
- `get_memories_by_type()`, `get_memories_by_scope()`
- `link_memory_to_type()`, `link_memory_to_scope()`

### 3.4 Wire add_memory() (1h)
- Accept type/scope parameters, create edges

**Total**: 1.5 days

**Outcome**: Query by type/scope, graph structured
- ✓ User-controlled typing

---

## Phase 5: Chunk/Embedding Guarantee

**Goal**: Ensure all memories are chunked and embedded

### 5.1 DB Trait Contract
- [ ] Document: add_memory() MUST chunk before storage
- [ ] Document: add_memory() MUST embed before storage
- [ ] Immutability contract for chunks

**Estimated**: 1 hour

### 5.2 Implement in Backends
- [ ] voidm-sqlite: Chunking in add_memory()
- [ ] voidm-sqlite: Embedding in add_memory()
- [ ] Test: chunks created, embeddings stored

**Estimated**: 1 day

### 5.3 Update Cascade
- [ ] update_memory() invalidates existing chunks
- [ ] Re-chunk + re-embed
- [ ] Tests

**Estimated**: 1 day

**Estimated Total**: 2 days

**Outcome**:
- ✓ All memories properly chunked
- ✓ All chunks properly embedded
- ✓ Updates safe and consistent

---

## Phase 6: Tag System + Refresh

**Goal**: Implement user-provided tags + tag refresh on update

### 6.1 User-Provided Tags
- [ ] NodeType::Tag nodes
- [ ] Memory -[:TAGGED_WITH]-> Tag edges
- [ ] `add_memory(..., tags: Vec<String>)`
- [ ] `db.list_tags_for_memory(id)`

**Estimated**: 2 hours

### 6.2 Tag Refresh on Update
- [ ] On `update_memory()`: remove old tags
- [ ] Add new user-provided tags
- [ ] Store old tags for history (optional)

**Estimated**: 2 hours

### 6.3 Tests
- [ ] Tag creation works
- [ ] Tag linking works
- [ ] Tag refresh removes old, adds new

**Estimated**: 1 hour

**Estimated Total**: 2 days

**Outcome**:
- ✓ User-provided tags working
- ✓ Tag refresh on update working
- ✓ Tags as first-class nodes

---

# FEATURE & INTEGRATION PHASES: 4, 7, 8 (CAN PARALLEL)

---

## Phase 4: Multiple Backend Support (1 day)
- Support [backend.default], [backend.archive], etc.
- Add `--backend NAME` to all commands
- Load correct backend, pass to DB operations

## Phase 7: Configuration Flexibility (1 day)
- Config v2 with multiple instances per backend type
- CLI routing: `voidm search --backend archive`
- Default routing for unspecified backend

## Phase 8: Search & Polish (1.5 days)
- Verify search works with multiple backends
- Remove all compiler warnings
- Audit unsafe code
- `cargo build --all` passes

---

# DEFERRED TO PHASE 9+ (FEATURES, NOT CRITICAL)

| Item | Original | New Home | Rationale |
|------|----------|----------|-----------|
| Title embeddings | Phase 0 | Phase 9.1 | Post-stabilization |
| Edge weights | Phase 0 | Phase 9+ | Add as needed, not core |
| Auto-tagging | Phase 6 | Phase 9.2 | Complex, post-stabilization |
| Tag metadata | Phase 6.2 | Phase 9.3 | Only with auto-tagging |
| Multi-dim search CLI | Phase 8.1 | Phase 9.4 | Enhancement, not core |
| Entity mention weight | Phase 3.5 | Phase 9.5 | Depends on edge weights |
| NLI integration | Phase 6.4 | Backlog | Not scoped |
| Embedding optimization | Phase 0 | Phase 9.7 | Performance, not core |
| NER re-enablement | Phase 3.5 | Phase 6+ | After tag system ready |
| Entity ref weight | Phase 3.5 | Phase 9.5 | Multiple dependencies |

---

# TIMELINE COMPARISON

| Strategy | Duration | Notes |
|----------|----------|-------|
| Original | 24.75-28.75 days | Overengineered |
| **Simplified Core** | **15-18 days** | Focused, stable |
| With Phase 9+ | 22-25 days total | Features after |

**Critical Path**: Phase -1 (✓) → 0 (3-4d) → 1 (3-4d) = 6-8 days
**Parallel**: 2+3+5+6 (4-5d) + 4+7+8 (3-4d)
**Total**: 15-18 days to stable multi-backend core

---

# DEFINITION OF DONE (PHASE 8)

✓ Generic node/edge format in SQLite
✓ Chunk ordering (sequence_num, char_start, char_end)
✓ All DB ops via trait (zero raw SQL outside backends)
✓ Multiple backends configurable
✓ MemoryType, Scope, Tag nodes (user-provided)
✓ Chunking/embedding guaranteed
✓ Tag refresh on update
✓ Search working
✓ Zero warnings
✓ All tests passing

---

# KEY PRINCIPLES

1. Core stability first (Phases -1 through 8)
2. Generic format is foundation (Phase 0)
3. Trait abstraction enables backends (Phase 1)
4. User-provided, no magic inference
5. Parallel execution where possible
6. Defer features to Phase 9+

---

# RISKS & MITIGATIONS

| Risk | Impact | Mitigation |
|------|--------|-----------|
| Phase 1 sqlx violations cause breakage | Architecture broken | Automated CI: forbid sqlx in non-backends |
| Phase 0 design wrong | Blocks everything | Review filemind design, do spike test |
| Backward compat breaks | Existing migrations fail | Test on Neo4j instance first |
| Config detection too aggressive | Breaks user environments | Smart detection: only with Cargo.toml/.git |
| Production config accidentally modified | Data loss | Auto-selection prevents accidental changes |
| Phase 2 doesn't disable NER properly | NER creates Concepts | Explicit feature flag disable + tests |

---

# REFERENCES

- TODO-3b8c2561: Phase -1 implementation (DONE)
- PLAN_REVIEW.md: Detailed analysis of deferred items
- filemind-db-trait: Reference implementation for Phase 0
- voidm-db-trait: Existing trait to extend in Phase 1
