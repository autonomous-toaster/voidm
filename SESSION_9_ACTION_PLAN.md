# Session 9: Phase 1.5 Completion - Architecture Fix

## Status: CRITICAL BLOCKER IDENTIFIED

Session 8 created the infrastructure, but discovered **add_memory_backend.rs is non-functional**:
- Wrapper calls back to voidm_core::add_memory
- Which STILL contains all the sqlx code
- Defeats the extraction purpose

---

## The Real Problem

### Current (Broken) Flow
```
voidm-cli calls crud_trait::add_memory()
    ↓
voidm-sqlite trait method add_memory()
    ↓
add_memory_backend::execute_add_memory_transaction_wrapper()
    ↓
voidm_core::crud::add_memory() ← STILL HAS SQLX!
    ↓
[transaction with 100+ lines of sqlx]
    ↓
Response
```

**Result**: No sqlx eliminated, no progress on Phase 1.5

### Correct Flow (Target)
```
voidm-cli calls crud_trait::add_memory()
    ↓
voidm-sqlite trait method add_memory()
    ↓
execute_add_memory_transaction_wrapper() does:
  1. Pre-tx logic (validation, embeddings, scoring)
  2. Calls backend::execute_add_memory_transaction()
  3. Post-tx logic (extract, link)
    ↓
Result: sqlx ONLY in backend, core is clean
```

---

## Session 9 Tasks

### Task 1: Fix add_memory_backend (1 hour)

**Current State**: add_memory_backend.rs exists but wrapper calls core

**Action**:
```rust
// CURRENT (WRONG):
pub async fn execute_add_memory_transaction_wrapper(...) {
    voidm_core::crud::add_memory(pool, req, config).await  // ← WRONG!
}

// TARGET (RIGHT):
pub async fn execute_add_memory_transaction_wrapper(
    pool: &SqlitePool,
    pre_tx: PreTxData,  // Prepared data from core
) -> Result<AddMemoryResponse> {
    // Do transaction only
    execute_add_memory_transaction(pool, pre_tx).await
}
```

**Steps**:
1. Define PreTxData struct (ID, tags_json, metadata_json, embedding, quality, links, etc.)
2. Move transaction block from core to backend (lines 147-246 in crud.rs)
3. Update wrapper to accept PreTxData
4. Call real execute_add_memory_transaction (not the wrapper)

**Expected Result**: ~100 lines of sqlx moved from core to backend

---

### Task 2: Split voidm-core::add_memory (1 hour)

**Current**: add_memory has pre-tx + tx + post-tx logic (250+ lines)

**Action**: Split into 2 parts:
```rust
// Part 1: Business logic (stay in core)
async fn add_memory_prepare(
    pool: &SqlitePool,
    req: AddMemoryRequest,
    config: &Config,
) -> Result<PreTxData> {
    // Validation, embeddings, scoring, ID resolution
    // Returns prepared data
}

// Part 2: Orchestration (call backend)
async fn add_memory(pool, req, config) {
    let prepared = add_memory_prepare(pool, req, config).await?;
    
    // Call backend
    let resp = voidm_sqlite::add_memory_backend::execute_add_memory_transaction_wrapper(
        pool,
        prepared,
    ).await?;
    
    // Post-tx
    if config.insert.auto_extract_concepts { ... }
}
```

**Expected Result**: 
- Pre-tx logic (50+ lines) stays in core
- Tx logic (100 lines) moves to backend
- Post-tx logic (30 lines) stays in core
- voidm-core::crud::add_memory has 0 sqlx

---

### Task 3: Move Backend-Specific Utilities (1-1.5 hours)

**Move these FROM voidm-core TO voidm-sqlite**:

1. **resolve_id_sqlite()** (from crud.rs)
   - Move to voidm-sqlite as method
   - References: 113, 230 in lib.rs

2. **get_scopes()** (from crud.rs)
   - Move to voidm-sqlite as method
   - References: 133, 196 in lib.rs

3. **chunk_nodes** (from voidm-core::chunk_nodes)
   - This whole module is backend-specific
   - Move to voidm-sqlite::chunk_nodes

**Steps for Each**:
1. Copy function to voidm-sqlite
2. Update references in voidm-sqlite to call local version
3. Delete from voidm-core (if not used elsewhere)
4. Verify build

---

### Task 4: Integration Testing (30 min)

**Test** that memories can still be created:

```bash
# CLI test
voidm remember --content "test" --type semantic

# Verify
voidm list

# Check no errors
# Verify memory stored correctly
```

**Test** MCP (if using):
```bash
# Through MCP remember tool
# Verify response includes all fields
# Verify no regressions
```

---

### Task 5: Violation Count (30 min)

**Count** how many sqlx violations are eliminated:

```bash
# Before (should be ~51 in crud.rs add_memory section)
grep "sqlx::" crates/voidm-core/src/crud.rs | wc -l

# After (should be ~0 in add_memory, but some in other functions)
# Extract add_memory function and count
```

**Expected**:
- 20-30 violations eliminated from voidm-core
- All moved to voidm-sqlite backend
- Build passing (14/14 crates)

---

## Detailed Implementation: Task 1

### Create PreTxData struct

```rust
// In add_memory_backend.rs or separate module
pub struct PreTxData {
    pub id: String,
    pub memory_type_str: String,
    pub content: String,
    pub importance: i64,
    pub tags_json: String,
    pub metadata_json: String,
    pub quality: QualityScore,
    pub embedding_result: Option<Vec<f32>>,
    pub context: Option<String>,
    pub title: Option<String>,
    pub scopes: Vec<String>,
    pub resolved_link_targets: Vec<(EdgeType, Option<String>, String)>,
    pub now: String,
}
```

### Extract Transaction Block

```rust
// Move from voidm-core lines 147-246 to voidm-sqlite
pub async fn execute_add_memory_transaction(
    pool: &SqlitePool,
    data: &PreTxData,
) -> Result<AddMemoryResponse> {
    let mut tx = pool.begin().await?;
    
    // All the sqlx code from lines 147-246
    
    tx.commit().await.context("Transaction commit failed")?;
    
    Ok(AddMemoryResponse { ... })
}
```

### Fix Wrapper

```rust
pub async fn execute_add_memory_transaction_wrapper(
    pool: &SqlitePool,
    data: PreTxData,
) -> Result<AddMemoryResponse> {
    execute_add_memory_transaction(pool, &data).await
}
```

---

## Detailed Implementation: Task 2

### In voidm-core/src/crud.rs

```rust
// Extract preparation logic
async fn prepare_memory_data(
    pool: &SqlitePool,
    mut req: AddMemoryRequest,
    config: &Config,
) -> Result<PreTxData> {
    // Lines 64-145 from current add_memory
    // Return PreTxData struct
}

// Keep existing add_memory but simplify
pub async fn add_memory(
    pool: &SqlitePool,
    req: AddMemoryRequest,
    config: &Config,
) -> Result<AddMemoryResponse> {
    // Get prepared data
    let prepared = prepare_memory_data(pool, req, config).await?;
    
    // Call backend (will need to import voidm-sqlite)
    // OR use feature flag to avoid direct dependency
    
    #[cfg(feature = "sqlite-backend")]
    let resp = voidm_sqlite::add_memory_backend::execute_add_memory_transaction_wrapper(
        pool,
        prepared,
    ).await?;
    
    #[cfg(not(feature = "sqlite-backend"))]
    let resp = default_add_memory(pool, prepared).await?;
    
    // Post-tx logic (lines 247+)
    if config.insert.auto_extract_concepts {
        if let Err(e) = extract_and_link_concepts(... ) { ... }
    }
    
    Ok(resp)
}
```

---

## Success Criteria

- [ ] add_memory_backend.rs properly implements transaction logic
- [ ] voidm-core::add_memory has 0 sqlx violations in add_memory function
- [ ] All 100+ lines of transaction code in voidm-sqlite backend
- [ ] voidm-core calls backend via wrapper/feature flag
- [ ] Build passing (14/14 crates, 0 errors)
- [ ] CLI memory creation works end-to-end
- [ ] 20+ sqlx violations eliminated
- [ ] Tests passing

---

## Time Breakdown

| Task | Duration | Buffer |
|------|----------|--------|
| Fix add_memory_backend | 1h | 15min |
| Split voidm-core::add_memory | 1h | 15min |
| Move utilities | 1-1.5h | 30min |
| Integration testing | 30min | 15min |
| Violation count | 30min | - |
| **TOTAL** | **4-4.5h** | **1.25h** |

**Realistic Time**: 4-5.5 hours (may need 2 sessions)

---

## If Time Runs Out

### Minimum Viable: 2 hours
1. Fix add_memory_backend wrapper (1h)
2. Move resolve_id_sqlite & get_scopes (1h)
3. Verify build (15min)

**Result**: Partial Phase 1.5 completion, pattern established

### Full: 4-5 hours
All tasks above, Phase 1.5 complete

---

## Next Steps After Session 9

If Phase 1.5 completes:
- Phase 1.6: Extract migrate.rs (2 hours)
- Phase 1.7: Extract chunk_nodes.rs (1-2 hours)
- Phase 1.8: Refactor voidm-graph (3 hours)
- Phase 1.9: Cleanup (2-3 hours)

