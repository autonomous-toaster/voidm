# Architecture Refactoring: Moving Models to voidm-db

## Current Problem

**voidm-db-trait has circular dependencies in practice:**
- voidm-sqlite depends on voidm-core (for models, queries)
- voidm-neo4j depends on voidm-core (for models, queries)
- voidm-core depends on voidm-db-trait (for Database trait)
- But models are SHARED across all backends

**Better organization**: Put SHARED data models in the trait crate

---

## Proposed Refactoring

### Step 1: Rename voidm-db-trait → voidm-db

**Why**: The crate contains more than just trait (it will have models too)

**Files**: Rename crate directory and update Cargo.toml references

---

### Step 2: Move voidm-core/models.rs → voidm-db/models.rs

**Current In voidm-core/src/models.rs**:
- Memory struct (domain model)
- MemoryType enum
- AddMemoryRequest
- AddMemoryResponse
- EdgeType enum
- LinkResponse
- DuplicateWarning
- SuggestedLink
- Other request/response structs

**All of these are:**
1. Backend-agnostic (not SQLite-specific)
2. Shared across all backends
3. Used by trait consumers
4. Part of public API

**Should be in voidm-db because**:
- Backends import models from trait crate
- Frontend/CLI import models from trait crate
- No backend-specific code in models

---

### Step 3: Updated Dependency Graph

**BEFORE (Circular-like)**:
```
voidm-cli
    ↓
voidm-core (has models, business logic, crud)
    ↓
voidm-db-trait (trait only)
    ↓
voidm-sqlite (implements trait, calls back to core)
voidm-neo4j (implements trait, calls back to core)
```

**AFTER (Clean One-Way)**:
```
voidm-cli
    ↓
voidm-db (models, config, trait)
    ↓
voidm-core (business logic, validation, scoring)
    ↓
voidm-sqlite (backend implementation)
voidm-neo4j (backend implementation)
voidm-tagging (feature layer)
voidm-ner (feature layer)
```

---

## Detailed Plan

### Phase 1: Preparation

**1.1 Analyze voidm-core/models.rs**

Current content to move:
- Memory (domain model) ✓
- MemoryType enum ✓
- MemoryEdge ✓
- AddMemoryRequest ✓
- AddMemoryResponse ✓
- EdgeType enum ✓
- LinkResponse ✓
- LinkSpec ✓
- DuplicateWarning ✓
- SuggestedLink ✓
- validate_title() ✓

**Keep in voidm-core** (business logic):
- None - models are pure data

**Count**: ~200-250 lines to move

---

### Phase 2: Rename Crate

**2.1 Rename directory**:
```bash
mv crates/voidm-db-trait crates/voidm-db
```

**2.2 Update Cargo.toml**:
- Change package name from voidm-db-trait to voidm-db
- Update workspace members

**2.3 Update imports everywhere**:
```bash
sed -i 's/voidm-db-trait/voidm-db/g' Cargo.toml
sed -i 's/use voidm_db_trait/use voidm_db/g' **/*.rs
```

**2.4 Verify**: Build should still pass

---

### Phase 3: Move Models

**3.1 Copy voidm-core/models.rs → voidm-db/src/models.rs**

**3.2 Remove voidm-core/models.rs**

**3.3 Update voidm-db/src/lib.rs**:
```rust
pub mod models;
pub use models::{Memory, MemoryType, AddMemoryRequest, ...};
```

**3.4 Update voidm-core/src/lib.rs**:
```rust
// REMOVE: pub mod models;
// ADD: pub use voidm_db::models;
```

**3.5 Update all imports**:
```bash
# In voidm-core
sed -i 's/use crate::models::/use voidm_db::models::/g' src/**/*.rs

# In voidm-sqlite
sed -i 's/use voidm_core::models::/use voidm_db::models::/g' src/**/*.rs

# Similar for voidm-neo4j, voidm-mcp, etc.
```

**3.6 Verify**: Build should still pass

---

### Phase 4: Update Dependencies

**4.1 voidm-core/Cargo.toml**:
```toml
# ADD:
voidm-db = { path = "../voidm-db" }

# (already has voidm-db-trait)
# This ensures core can re-export models for backward compat
```

**4.2 voidm-sqlite/Cargo.toml**:
```toml
# CHANGE FROM:
voidm-db-trait = { path = "../voidm-db-trait" }
voidm-core = { path = "../voidm-core" }

# CHANGE TO:
voidm-db = { path = "../voidm-db" }
voidm-core = { path = "../voidm-core" }
# (can remove db-trait if only using models)
```

**4.3 Same for voidm-neo4j, voidm-mcp**

**4.4 Verify**: Build should still pass

---

## Crate Purity Assessment

### voidm-db (Pure Foundation)
**Should contain**:
- ✅ Database trait (backend interface)
- ✅ Data models (Memory, MemoryType, EdgeType, etc.)
- ✅ Request/response types (AddMemoryRequest, LinkResponse, etc.)
- ✅ Config (moved from core - global app config)

**Should NOT contain**:
- ❌ Business logic (validation, scoring)
- ❌ Backend implementations (SQLite, Neo4j)
- ❌ Query infrastructure (QueryTranslator, CypherOperation)

**Purity**: 95% (pure data + trait definition)

---

### voidm-core (Pure Business Logic)
**Should contain**:
- ✅ Crud operations (validation, orchestration)
- ✅ Search logic (ranking, filtering, RRF)
- ✅ Scoring & quality computation
- ✅ Query infrastructure (translators, operations)
- ✅ Chunking & embeddings
- ✅ Re-export models from voidm-db (backward compat)

**Should NOT contain**:
- ❌ Backend implementations
- ❌ Database-specific operations (sqlx)
- ❌ Data models (moved to voidm-db)

**Purity**: 90% (business logic + query abstractions)

**sqlx violations remaining**: ~40-50 (in add_memory, migrate, others)

---

### voidm-sqlite (Pure SQLite Backend)
**Should contain**:
- ✅ SqliteDatabase struct (implements trait)
- ✅ Transaction execution (add_memory_impl, etc.)
- ✅ Query mapping (CypherOperation → SQL)
- ✅ SQLite-specific helpers (resolve_id_sqlite, etc.)

**Should NOT contain**:
- ❌ Business logic
- ❌ Models (import from voidm-db)
- ❌ Validation (import from voidm-core)

**Purity**: 99% (pure backend implementation)

**sqlx violations**: All sqlx stays here (correct location)

---

### voidm-neo4j (Pure Neo4j Backend)
**Should contain**:
- ✅ Neo4jDatabase struct
- ✅ Neo4j-specific query mapping
- ✅ Schema/connection logic

**Should NOT contain**:
- ❌ Business logic
- ❌ Models
- ❌ Validation

**Purity**: 99% (pure backend)

**neo4j violations**: All neo4rs stays here (correct)

---

### voidm-cli (Pure CLI)
**Should contain**:
- ✅ Command line argument parsing
- ✅ Output formatting
- ✅ CLI-specific orchestration

**Should NOT contain**:
- ❌ Business logic (call voidm-core)
- ❌ Database operations (call trait)

**Purity**: 80% (depends on many crates, but appropriate)

---

### voidm-mcp (Pure MCP Server)
**Should contain**:
- ✅ MCP protocol implementation
- ✅ Tool definitions
- ✅ JSON marshaling

**Should NOT contain**:
- ❌ Business logic

**Purity**: 85%

---

## Impact on Phase 1.5

### What Changes

**Before refactoring:**
```
voidm-core::add_memory()
    ↓
validate, embed, score (core logic)
    ↓
call backend via trait
    ↓
voidm-sqlite backend
```

**After refactoring:**
```
voidm-core::add_memory()
    ↓
validate, embed, score (core logic)
    ↓
call backend via trait (models now in voidm-db)
    ↓
voidm-sqlite backend (imports models from voidm-db)
```

**No functional change**, just cleaner imports

### Phase 1.5 Timeline Impact

- **Rename crate**: 30 min
- **Move models**: 45 min (bulk move + import updates)
- **Update dependencies**: 30 min (Cargo.toml changes)
- **Verify build**: 15 min
- **Total**: 2 hours

**Can be done as Phase 1.5.1 BEFORE the add_memory extraction work**

---

## Implementation Order (Revised Phase 1.5)

### NEW Phase 1.5.0: Architecture Refactoring (2 hours)
1. Rename voidm-db-trait → voidm-db (30 min)
2. Move models to voidm-db (45 min)
3. Update all imports (30 min)
4. Verify build (15 min)

### CURRENT Phase 1.5.1: Backend Code Cleanup (30 min) ✓ DONE
- Already moved neo4j files

### CURRENT Phase 1.5.2: Backend Infrastructure (2 hours) ✓ DONE
- add_memory_backend.rs already created

### Phase 1.5.3: Fix add_memory Blocker (4-5 hours)
- Fix wrapper to call backend properly
- Move utility functions
- Eliminate sqlx from voidm-core::add_memory

### Phase 1.5.4: Testing & Verification (1 hour)
- E2E tests
- Violation count
- Build verification

**Total Phase 1.5 revised**: 9-10 hours (includes architecture refactoring)

---

## Benefits of This Refactoring

### 1. Cleaner Dependencies
```
voidm-db (foundation, no dependencies)
    ↑
voidm-core (business logic, depends on voidm-db)
    ↑
voidm-sqlite, voidm-neo4j (implementations, depend on voidm-db + voidm-core)
    ↑
voidm-cli, voidm-mcp (consumers, depend on all above)
```

### 2. Reduced Coupling
- Backends only import from voidm-db + voidm-core
- No back-calls to voidm-core for models
- Clear separation of concerns

### 3. Better Organization
- Models in foundation crate (makes sense)
- Business logic in core (makes sense)
- Implementations in backend crates (makes sense)

### 4. Easier to Test
- Can test backends with just voidm-db + voidm-core
- No circular dependency confusion

### 5. Easier for New Contributors
- Models in obvious place (voidm-db)
- Business logic in obvious place (voidm-core)
- Implementations obvious (voidm-sqlite, etc.)

---

## Risk Assessment

### Risk Level: LOW

**Why**:
- No functional changes, just organization
- Move + rename are mechanical operations
- Can verify at each step (build checking)
- Backward compat: voidm-core re-exports models

### Potential Issues

1. **Circular imports**: If voidm-db imports from voidm-core
   - **Prevention**: voidm-db only has models + trait, no imports from core

2. **Missing imports**: After moving models
   - **Prevention**: Find + replace in all files

3. **Feature flags**: If models use #[cfg(...)] features
   - **Check**: grep for #[cfg in models.rs first

4. **Macro issues**: If models use derive macros
   - **Check**: grep for derive, likely fine (serde, etc.)

### Mitigation

- Do refactoring in small steps (rename, then move, then imports)
- Build after each step
- Use git to track changes (easy to revert if needed)

---

## Updated PLAN.md (Conceptual)

### New Phase Structure

```
Phase -1: Config ✓
Phase 0: Generic format ✓
Phase 1: Backend abstraction
  ├─ Phase 1.1: Audit ✓
  ├─ Phase 1.5.0: ARCHITECTURE REFACTORING (NEW) ← 2 hours
  │  ├─ Rename voidm-db-trait → voidm-db
  │  ├─ Move models to voidm-db
  │  └─ Update all imports
  ├─ Phase 1.5.1: Backend cleanup ✓
  ├─ Phase 1.5.2: Backend infrastructure ✓
  ├─ Phase 1.5.3: Fix add_memory blocker
  ├─ Phase 1.5.4: Testing & verification
  ├─ Phase 1.6: Extract migrate.rs
  ├─ Phase 1.7: Extract chunk_nodes
  ├─ Phase 1.8: Refactor voidm-graph
  └─ Phase 1.9: Cleanup & finalize
```

**Impact on timeline**:
- Adds 2 hours before Phase 1.5.3
- But makes 1.5.3 easier (cleaner dependency graph)
- Net impact: +1 hour (saves 1 hour in debugging)

---

## Next Steps

### If We Do This Refactoring

1. **Session 9a** (2 hours): Architecture refactoring
   - Rename + move models
   - Update imports
   - Verify build

2. **Session 9b** (4-5 hours): Fix add_memory blocker
   - Execute SESSION_9_ACTION_PLAN.md
   - Phase 1.5 complete

3. **Sessions 10-12** (6-8 hours): Phases 1.6-1.9
   - Extract remaining functions
   - Phase 1 complete

### If We Skip This Refactoring

- Proceed directly to Session 9 action plan
- 4-5 hours to fix add_memory blocker
- Dependency graph remains less clean but functional
- Can do refactoring later (Phase 2+)

---

## Recommendation

**DO THIS REFACTORING** because:

1. ✅ Only 2 hours, can be done in first part of Session 9
2. ✅ Makes remaining work cleaner
3. ✅ Improves code organization permanently
4. ✅ Reduces future coupling issues
5. ✅ Better for maintaining 126 violations → 0 goal

**Timeline with refactoring**: 11-12 hours remaining for Phase 1
**Timeline without**: 10-11 hours remaining

**Net cost**: ~1 hour extra to clean up architecture.

**Worth it**: YES (cleaner end result, easier maintenance)

