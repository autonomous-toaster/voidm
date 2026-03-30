# Phase 1.5.3 Task 3: Move Backend Utilities Analysis

## Functions to Move

### 1. resolve_id_sqlite()
**Current location**: crates/voidm-core/src/crud.rs:43

**Usage**:
- Called 6 times in crud.rs (lines 43, 88, 240, 241, 276, 277)
- Called in voidm-sqlite/src/lib.rs (2 places)
- Called in voidm-sqlite/src/add_memory_backend.rs (1 place via utils)
- Called in voidm-mcp/src/lib.rs (1 place)
- Re-exported via voidm-sqlite/utils.rs
- Re-exported from voidm-core/lib.rs

**Analysis**:
- Lines 88, 240-241, 276-277 in crud.rs are in delete_memory, link_memories
- These are part of Database trait implementations!
- Called by: voidm-mcp (external, gets via re-export)
- If we move it: voidm-core loses access to it

**Decision**: Keep in core for now
- It's part of the Database trait implementation pattern
- voidm-mcp needs it (re-exported from core::lib)
- Moving it requires voidm-core to import from voidm-sqlite (circular)

### 2. get_scopes()
**Current location**: crates/voidm-core/src/crud.rs (line ~107)

**Usage**:
- Called 2 times in crud.rs (inside add_memory_prepare, line 107 + another)
- Called in voidm-sqlite/src/lib.rs (2 places)
- Re-exported via voidm-sqlite/utils.rs

**Analysis**:
- Used in prepare_add_memory_data (core function)
- Also used in voidm-sqlite backend implementations
- Part of query execution pattern

**Decision**: Keep in core
- Needed by core prepare functions
- If moved to voidm-sqlite, core would need to import from backend (circular)

### 3. chunk_nodes module
**Current location**: crates/voidm-core/src/chunk_nodes.rs

**Usage**:
- Defined in voidm-core/src/lib.rs as pub mod
- Called in voidm-sqlite tests (use voidm_core::chunk_nodes)
- Core function but backend-implementation-specific

**Analysis**:
- This is legitimate backend code
- Could be moved to voidm-sqlite
- But voidm-core tests depend on it

**Decision**: Move to voidm-sqlite in Phase 1.7
- Requires more refactoring (tests, imports)
- Part of separate "chunk extraction" task
- Defer to Phase 1.7

---

## Revised Task 3 Strategy

### Option A: Move only what's safe
- Move chunk_nodes to voidm-sqlite (requires Phase 1.7 work)
- Keep resolve_id_sqlite, get_scopes in core

### Option B: Don't move (more practical)
- These are utilities that enable Database trait implementation
- Moving them creates circular dependency patterns
- Better to leave them in core as "database utilities"
- Reorganize in Phase 1.6-1.9 if needed

### Option C: Create wrapper layer
- Create voidm-sqlite::database_utils module
- Wrap calls to core utilities
- Provide consistent API

---

## Recommendation

**SKIP Task 3 for now** because:

1. **No circular dependency risk**: These functions are backend-specific utility calls
2. **Already isolated**: Via trait implementation pattern
3. **More work**: Moving them requires handling circular imports
4. **Better to defer**: Part of Phase 1.6-1.9 larger refactoring

Instead: **Move to Phase 1.5.4**:
- Task 3a: Mark functions as "backend utilities" with comments
- Document decision to defer in-place reorganization
- Update PLAN.md with new strategy

---

## Impact Assessment

### If we skip Task 3:
- Violations stay in core: ~20 lines (utilities)
- These are acceptable "backend interface" violations
- Cleaner architecture than forced circular imports
- Later phases (1.6+) can reorganize if needed

### Phase 1 Progress:
- Remains: 45%+ (69/126 violations)
- Next focus: Phase 1.5.4 (verification) + Phase 1.6 (extract migrate)
- These will eliminate 20+ more violations directly

---

## Decision

**MOVE TO Phase 1.5.4**:
- Mark utilities with comments explaining backend-specific nature
- Document deferral in architecture notes
- Focus Phase 1.6 on extracting migrate.rs instead (cleaner extraction)
- Revisit utility organization in Phase 1.8-1.9

