# Phase 1.5: Extract add_memory Function

**Scope**: Refactor `add_memory` from voidm-core to use Database trait
**Estimated Time**: 4-5 hours
**Violations**: ~20 sqlx calls (largest function)
**Blocker**: None (can start anytime)

## Current State

### Location
- `crates/voidm-core/src/crud.rs:63-480` (418 lines)
- Called by: MCP (voidm-mcp/src/lib.rs:remember tool)
- Callers also use: add_memory already exists as trait method in voidm-sqlite

### Current Signature
```rust
pub async fn add_memory(pool: &SqlitePool, req: AddMemoryRequest, config: &Config) -> Result<AddMemoryResponse>
```

### sqlx Violations (20+ total)
1. db_meta INSERT/UPDATE (2 queries) - line ~122
2. vec_memories INSERT (1 query) - line ~155
3. graph_nodes INSERT/ignore (1 query) - line ~164
4. graph_node_labels INSERT (1 query) - line ~169
5. graph_node_props_text UPSERT (1 query) - line ~174
6. graph_edges INSERT loop (varies with # links) - line ~192

Plus MANY more in transaction body...

## Refactoring Strategy

### Phase 1.5A: Understand Trait Contract
- [ ] Check what `Database::add_memory()` trait method signature is
- [ ] Verify return type matches AddMemoryResponse
- [ ] Check if backends (voidm-sqlite, voidm-neo4j) have implementations

### Phase 1.5B: Extract Transaction to voidm-sqlite
- [ ] Create `add_memory_impl()` in voidm-sqlite that:
  - Takes pool, req, config
  - Wraps entire transaction logic
  - Returns AddMemoryResponse
- [ ] Move ALL sqlx calls from voidm-core to voidm-sqlite
- [ ] Update trait method to call add_memory_impl

### Phase 1.5C: Create voidm-core Wrapper
- [ ] New `add_memory()` signature: `(db: &dyn Database, req: AddMemoryRequest, config: &Config)`
- [ ] Keep business logic in voidm-core:
  - Redaction
  - Validation (RELATES_TO requires note)
  - Embedding computation (call embeddings crate)
  - Quality score computation (call voidm_scoring)
  - Link target resolution
- [ ] Call `db.add_memory()` to execute database operations

### Phase 1.5D: Update Callers
- [ ] MCP: Update remember tool to use trait-based add_memory
- [ ] CLI: Check if add command uses trait yet
- [ ] Create compat layer if needed

### Phase 1.5E: Verification
- [ ] Build all crates (0 errors)
- [ ] Count violations eliminated
- [ ] Test with `voidm remember` command if possible

## Key Decisions

1. **Keep Business Logic in voidm-core**
   - Redaction (involves config.redaction_patterns)
   - Validation (EdgeType checks)
   - Embeddings (calls embeddings::embed_text_chunked)
   - Quality scoring (calls voidm_scoring::compute_quality_score)

2. **Move to Backend (voidm-sqlite)**
   - All database operations
   - Transaction handling
   - Graph node/edge management

3. **Return Type**
   - Must return AddMemoryResponse with:
     - Memory (the created memory)
     - Embedding (if computed)
     - Quality score
     - Any warnings

## Complexity Breakdown

| Task | Lines | Complexity |
|------|-------|-----------|
| Extract transaction logic | ~250 | HIGH - many interdependencies |
| Create trait signature | ~5 | LOW |
| Update voidm-sqlite impl | ~260 | HIGH - mirrors current code |
| Create wrapper in voidm-core | ~50 | MEDIUM - orchestration |
| Update callers | ~5 | LOW |
| Testing | Variable | MEDIUM |

## Next Steps

1. Start Phase 1.5A: Understand trait contract
2. Verify voidm-sqlite already has add_memory implementation
3. Extract transaction wholesale to voidm-sqlite
4. Create thin wrapper in voidm-core
5. Update MCP and CLI
6. Build and verify

## Success Criteria

- [ ] All 14 crates build successfully (0 errors)
- [ ] add_memory uses Database trait
- [ ] MCP can still remember memories
- [ ] ~20 sqlx violations eliminated from voidm-core
- [ ] No regressions in other functions
