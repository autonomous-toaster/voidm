# Phase 1.5 Next Steps: Wire add_memory_impl

## Current State

### ✅ Completed
- Created `add_memory_impl()` in voidm-sqlite (lines 355-495)
  - Full transaction logic extracted
  - All 16+ sqlx calls moved
  - Proper response building
  - Helper function for transaction-aware operations
- Build passing (14/14 crates)

### ⏳ Remaining Work

**Goal**: Make voidm_core call add_memory_impl instead of duplicating transaction logic

### Two Implementation Paths

#### Path A: Create Backend Function (RECOMMENDED)
```rust
// In voidm-sqlite/src/lib.rs (public)
pub async fn execute_add_memory_transaction(
    pool: &SqlitePool,
    id: &str,
    memory_type_str: &str,
    req: &AddMemoryRequest,
    embedding_result: Option<Vec<f32>>,
    quality: QualityScore,
    tags_json: &str,
    metadata_json: &str,
    resolved_link_targets: Vec<(EdgeType, Option<String>, String)>,
    now: &str,
) -> Result<AddMemoryResponse> {
    SqliteDatabase { pool: pool.clone() }
        .add_memory_impl(...)
        .await
}

// In voidm-core/src/crud.rs - call it
let db_impl = voidm_sqlite::execute_add_memory_transaction(
    pool,
    &id,
    &memory_type_str,
    &req,
    embedding_result,
    quality,
    &tags_json,
    &metadata_json,
    resolved_link_targets,
    &now,
).await?;
```

**Pros**: Clean, doesn't require trait changes, works with current MCP
**Cons**: voidm-core depends on voidm-sqlite (small issue, can be optional)

#### Path B: Create New Trait Method (FUTURE)
```rust
// In voidm-db-trait/src/lib.rs
fn add_memory_with_prepared_data(
    &self,
    ...prepared data...
) -> Pin<Box<dyn Future<Output = Result<AddMemoryResponse>> + Send + '_>>;
```

**Pros**: Generic, works with any backend
**Cons**: Requires changes to trait and all backends

### Recommended Approach for Next Session

1. **Implement Path A** (2-3 lines of code change in voidm-core)
2. **Test** that memories can still be added via MCP or CLI
3. **Verify** sqlx calls moved to backend
4. **Count** violations eliminated

### Files to Change

1. `crates/voidm-sqlite/src/lib.rs`
   - Add public `execute_add_memory_transaction()` function
   - Calls `add_memory_impl()` internally

2. `crates/voidm-core/src/crud.rs`
   - Replace transaction block (lines 147-246) with call to execute_add_memory_transaction()
   - Keep pre-tx logic (redaction, embeddings, scoring) as-is

### Expected Outcome

- ✅ All sqlx calls in add_memory moved to voidm-sqlite
- ✅ voidm-core contains only business logic
- ✅ 20+ violations eliminated
- ✅ MCP and CLI still work (using pool-based add_memory wrapper)
- ✅ Build passing
- ✅ Phase 1 reaches 34%+ completion (38+/126 violations)

### Success Criteria

- [ ] cargo build --all succeeds (0 errors)
- [ ] voidm-core/src/crud.rs has no sqlx imports in add_memory function
- [ ] voidm-sqlite has add_memory_impl being called
- [ ] Memory creation still works end-to-end

### Time Estimate

- Implement Path A: 30 min
- Testing: 20 min
- Build & verify: 15 min
- **Total: 65 min (1 hour)**

### Notes

- Don't need to update MCP yet (still works with pool-based version)
- Don't need to update trait method yet (still calls pool-based)
- This is purely internal refactoring
- Pattern can be reused for phases 1.6-1.9
