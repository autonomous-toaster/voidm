# Plan Update - Session 8 Finding

## CRITICAL DISCOVERY

**neo4j_db.rs and neo4j_schema.rs in voidm-core are backend-specific dead code**

### Facts

1. **Location**: `crates/voidm-core/src/neo4j_db.rs` (200+ lines)
2. **Location**: `crates/voidm-core/src/neo4j_schema.rs` (150+ lines)
3. **Purpose**: Neo4j connection management and schema definitions
4. **Usage**: ZERO references outside voidm-core/src/lib.rs

### Why This is Wrong

- ✗ These are 100% backend-specific (Neo4j operations)
- ✗ voidm-core should be backend-agnostic
- ✗ They belong in voidm-neo4j if needed
- ✗ Currently just dead code adding 350+ lines
- ✗ Violates architecture principle: backends isolated to backend crates

### Current State

```
voidm-core/src/lib.rs:
  pub mod neo4j_schema;
  pub mod neo4j_db;
  pub use neo4j_db::Neo4jDb;
  pub use neo4j_schema::{MemoryChunkSchema, SchemaStats, CoherenceStats};
```

But nobody calls it. Not even voidm-neo4j!

### Action Plan

**Phase 1.5.1: Move Neo4j Code to Backend** (30 min)

1. ✅ Verify no references exist (DONE - zero found)
2. Copy neo4j_db.rs → voidm-neo4j/src/
3. Copy neo4j_schema.rs → voidm-neo4j/src/
4. Remove mod declarations from voidm-core/src/lib.rs
5. Remove dead code exports
6. Build and verify

### Expected Outcome

- 350+ lines of backend code removed from voidm-core
- Cleaner architecture boundary
- voidm-core 100% backend-agnostic
- Build still passing

### Time Estimate

**Duration**: 30 minutes
**Complexity**: LOW (dead code, just moving files)
**Risk**: MINIMAL (zero references)

### Impact on Phase 1 Metrics

- voidm-core violations: Unchanged (this is dead code, not sqlx usage)
- Architecture quality: +1 (cleaner separation)
- Total lines in voidm-core: -350

### Decision

This is a **MUST DO** improvement before continuing Phase 1.5. It cleans up the codebase and makes the architecture cleaner for future backends.
