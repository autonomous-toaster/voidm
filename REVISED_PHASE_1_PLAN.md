# REVISED PLAN: Complete Phase 1 - SQLite & Neo4j Only

## Scope Change
- ❌ **Forget PostgreSQL**: No support needed
- ✅ **SQLite only**: Primary backend
- ✅ **Neo4j optional**: Secondary backend (stubs OK)
- ✅ **Remove parasite sqlx**: All logic → traits → backend

## Current Assessment

### Phase 1 Status: 73% Claimed, But Reality Check Needed

**Expected State**:
- voidm-core: 0 sqlx (should use Database trait)
- voidm-cli: 0 sqlx (should use trait objects)
- voidm-graph: 26 sqlx (Phase 1.9 ready but NOT DONE)

**Actual State**:
- voidm-core/src/crud.rs: 20 sqlx (still has _sqlite compat functions)
- voidm-core/src/search.rs: 1 sqlx
- voidm-core/src/vector.rs: 1 sqlx
- voidm-graph/*: 26 sqlx (NOT REFACTORED YET)
- voidm-cli/*: 3 sqlx (minor)
- voidm-mcp: 1 sqlx
- voidm-db: 1 sqlx
- Optional: 10 sqlx (tagging + ner)

**Total parasites**: 62 sqlx commands outside backends

### Real Phase 1 Remaining

| Task | Violations | Status | Est. Time |
|------|-----------|--------|-----------|
| 1.9 - voidm-graph refactoring | 26 | ⏳ NOT STARTED | 1.5h |
| Remove crud.rs _sqlite functions | 8 | ⏳ NOT STARTED | 30m |
| Extract search.rs sqlx | 1 | ⏳ NOT STARTED | 15m |
| Extract vector.rs sqlx | 1 | ⏳ NOT STARTED | 15m |
| Remove cli/mcp/db parasites | 5 | ⏳ NOT STARTED | 30m |
| Mark optional (tagging/ner) | 10 | ⏳ NOT STARTED | 30m |
| **TOTAL** | **51** | - | **3.5 hours** |

## Detailed Action Plan

### Phase 1.9a: Refactor voidm-graph (1.5 hours) - CRITICAL

**Current**: 26 sqlx in ops.rs, traverse.rs, cypher/mod.rs
**Target**: 0 sqlx - use GraphQueryOps trait

**Files**:
1. `crates/voidm-graph/src/ops.rs` (9)
2. `crates/voidm-graph/src/traverse.rs` (13)
3. `crates/voidm-graph/src/cypher/mod.rs` (4)
4. `crates/voidm-cli/src/commands/graph.rs` (caller)

**Steps**:
1. Update function signatures: `pool: &SqlitePool` → `ops: &dyn GraphQueryOps`
2. Replace sqlx::query calls with ops.method() calls
3. Update CLI caller to create/pass trait object
4. Test all graph commands

**Expected**: 26 violations → 0

---

### Phase 1.9b: Remove crud.rs compat functions (30 min)

**Current**: `get_scopes()`, `list_scopes()`, `get_or_create_node()`, `link_memories_sqlite()`, etc.
**Target**: Remove _sqlite versions, keep trait-based versions only

**Functions to delete**:
- `resolve_id_sqlite()` - has trait version
- `get_memory_sqlite()` - has trait version
- `link_memories_sqlite()` - has trait version
- `get_scopes()` - add to Database trait
- `list_scopes()` - add to Database trait
- `list_edges()` - add to Database trait
- `check_model_mismatch()` - SQLite specific, move to backend

**Files to add to Database trait**:
```rust
fn get_scopes(&self, memory_id: &str) -> Pin<Box<dyn Future<Output=Result<Vec<String>>> + Send + '_>>;
fn list_scopes(&self) -> Pin<Box<dyn Future<Output=Result<Vec<String>>> + Send + '_>>;
fn list_edges(&self) -> Pin<Box<dyn Future<Output=Result<Vec<MemoryEdge>>> + Send + '_>>;
```

**Expected**: 8 violations → 0 (in crud.rs)

---

### Phase 1.9c: Extract search & vector sqlx (30 min)

**search.rs (1 sqlx)**: Likely FTS or query execution
**vector.rs (1 sqlx)**: Likely embedding storage

**Action**: Add to Database trait as methods or move to backend

**Expected**: 2 violations → 0

---

### Phase 1.9d: Clean CLI/MCP/DB parasites (30 min)

**cli/commands/stats.rs (1)**: Should be 0 - already using trait
**cli/commands/graph.rs (2)**: Only trait object creation OK
**mcp/lib.rs (1)**: Bridge should use trait
**db/models.rs (1)**: Should be NewType pattern

**Action**: Audit each, fix or document why OK

**Expected**: 4-5 violations → 0 or marked

---

### Phase 1.9e: Mark optional features (30 min)

**voidm-tagging (8 sqlx)**: Non-critical, mark experimental
**voidm-ner (2 sqlx)**: Non-critical, mark experimental

**Action**:
1. Add `#![warn(missing_docs)]` or marker comment
2. Document in Cargo.toml: `optional = true`
3. Add to main Cargo.toml: `features = ["tagging", "ner"]`
4. Make features default: `default = []`

**Expected**: 10 violations marked as experimental

---

## Revised Phase 1 Architecture

```
╔══════════════════════════════════════════════════════════════════╗
║           CLEAN TRAIT-BASED ARCHITECTURE (FINAL)                 ║
╚══════════════════════════════════════════════════════════════════╝

Foundation (voidm-db)
├─ Database trait (36 methods)
│  ├─ Memory ops (add, list, get, delete, link, unlink)
│  ├─ Stats ops (get_statistics, get_graph_stats, get_graph_export_data)
│  ├─ Scope ops (get_scopes, list_scopes)
│  ├─ Edge ops (list_edges)
│  └─ Model info (check_model_mismatch)
├─ GraphQueryOps trait (13 methods)
│  ├─ Node/edge ops
│  ├─ Traversal ops
│  ├─ PageRank data
│  └─ Cypher execution
└─ Models (0 sqlx)

Backend (voidm-sqlite)
├─ Database impl (33 methods)
├─ GraphQueryOps impl (13 methods)
└─ All sqlx (133 commands)

Logic (voidm-core, voidm-cli, voidm-graph)
├─ All trait-based (0 sqlx)
├─ No back-calling to core
└─ Backend-agnostic

Features (voidm-tagging, voidm-ner)
├─ Marked experimental
├─ Optional (feature flags)
└─ Can use traits or have own sqlx
```

## Updated Violation Summary

**Before This Session**:
- Total: 126 violations
- Core/CLI/Graph: 62 (parasite)
- Backend: 99 (expected)
- Optional: 10 (marked)

**After Phase 1.9 (Complete)**:
- Total: 99
- Core/CLI/Graph: 0 (all parasite removed)
- Backend: 89 (SQLite + Neo4j stubs)
- Optional: 10 (marked experimental)

**Achievement**: Clean 100% non-optional core

## Build Status After Phase 1.9

Expected:
- ✅ 14 crates compile
- ✅ 0 errors
- ✅ All graph commands work
- ✅ All CLI commands work
- ✅ All integration tests pass

## Neo4j Strategy

**Implementation**: Stub implementations only (for now)
```rust
impl Database for Neo4jBackend {
    async fn get_memory(&self, id: &str) -> Result<Option<Memory>> {
        Err(anyhow!("Neo4j backend not yet implemented"))
    }
    // ...
}

impl GraphQueryOps for Neo4jGraphOps {
    async fn upsert_node(&self, memory_id: &str) -> Result<i64> {
        Err(anyhow!("Neo4j graph ops not yet implemented"))
    }
    // ...
}
```

**Why**: Keep architecture clean, enable future implementation without refactoring

## Timeline

**Total remaining**: ~3.5 hours
1. Phase 1.9a (voidm-graph): 1.5 hours
2. Phase 1.9b (crud.rs): 30 min
3. Phase 1.9c (search/vector): 30 min
4. Phase 1.9d (cli/mcp/db): 30 min
5. Phase 1.9e (optional): 30 min

**Result**: Phase 1 = 100% (core clean, backends isolated)

## Non-Negotiable Rules

1. ✅ **No sqlx outside backends**: Core/CLI/logic use traits
2. ✅ **No back-calling**: Backend never imports voidm-core
3. ✅ **Neo4j optional**: Stubs OK, full impl later
4. ✅ **PostgreSQL dropped**: SQLite + Neo4j only
5. ✅ **One-way deps**: Foundation ← Backend ← Logic only

