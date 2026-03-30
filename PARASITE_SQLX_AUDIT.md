# Parasite sqlx Commands Audit

## Expected sqlx Locations (Backend & Utils)
- ✅ voidm-sqlite/src/lib.rs (72) - Backend, expected
- ✅ voidm-sqlite/src/graph_query_ops_impl.rs (27) - Backend, expected
- ✅ voidm-sqlite/src/add_memory_backend.rs (18) - Backend, expected
- ✅ voidm-sqlite/src/migrate.rs (11) - Backend utils, expected
- ✅ voidm-sqlite/src/chunk_nodes.rs (5) - Backend utils, expected

**Backend subtotal: 133 sqlx:: (CORRECT)**

## Parasite sqlx (Non-Backend, Need Fixing)
- ❌ voidm-core/src/crud.rs (20) - Should use Database trait
- ❌ voidm-graph/src/traverse.rs (13) - Should use GraphQueryOps trait
- ❌ voidm-graph/src/ops.rs (9) - Should use GraphQueryOps trait
- ❌ voidm-graph/src/cypher/mod.rs (4) - Should use GraphQueryOps trait
- ❌ voidm-tagging/src/lib.rs (8) - Should use Database trait or mark optional
- ❌ voidm-ner/src/lib.rs (2) - Should use Database trait or mark optional
- ❌ voidm-cli/src/commands/graph.rs (2) - Should pass trait objects
- ❌ voidm-cli/src/commands/stats.rs (1) - Should use trait methods
- ❌ voidm-mcp/src/lib.rs (1) - Should use trait
- ❌ voidm-db/src/models.rs (1) - Should use NewType pattern
- ❌ voidm-core/src/vector.rs (1) - Should use trait
- ❌ voidm-core/src/search.rs (1) - Should use trait

**Parasite subtotal: 62 sqlx:: (MUST ELIMINATE)**

## Action Items by Crate

### 1. voidm-core/src/crud.rs (20 sqlx::)
**Status**: Phase 1.1-1.4 supposedly fixed, but still 20 remaining
**Investigation needed**: Check what's still there
**Expected**: Should be 0 - all to Database trait

### 2. voidm-graph/* (26 sqlx::)
**Status**: Phase 1.9 ready, but NOT YET REFACTORED
**ops.rs (9)**: upsert_node, delete_node, upsert_edge, delete_edge
**traverse.rs (13)**: neighbors, shortest_path, pagerank
**cypher/mod.rs (4)**: execute_read, run_query
**Expected**: Should be 0 - all to GraphQueryOps trait

### 3. voidm-tagging/src/lib.rs (8 sqlx::)
**Status**: Optional feature
**Action**: Mark as experimental or refactor
**Expected**: Can be deferred or marked

### 4. voidm-ner/src/lib.rs (2 sqlx::)
**Status**: Optional feature
**Action**: Mark as experimental or refactor
**Expected**: Can be deferred or marked

### 5. voidm-cli/src/commands/* (3 sqlx::)
**Status**: Should be trait-based
**graph.rs (2)**: Probably trait object creation - OK
**stats.rs (1)**: Should be 0 after Phase 1.8
**Expected**: Should be 0 (only trait usage)

### 6. voidm-mcp/src/lib.rs (1 sqlx::)
**Status**: Bridge to Database trait
**Expected**: Should be 0

### 7. voidm-db/src/models.rs (1 sqlx::)
**Status**: Foundation crate
**Expected**: Should be 0 - only types here

### 8. voidm-core/src/vector.rs (1 sqlx::)
**Status**: Core business logic
**Expected**: Should be 0 - needs trait

### 9. voidm-core/src/search.rs (1 sqlx::)
**Status**: Core business logic
**Expected**: Should be 0 - needs trait

## Priority Order

**CRITICAL (Phase 1.9 continuation)**:
1. voidm-core/src/crud.rs (20) - Check actual state
2. voidm-graph/* (26) - Implement Phase 1.9 refactoring

**HIGH (Must fix for clean architecture)**:
3. voidm-core/src/search.rs (1)
4. voidm-core/src/vector.rs (1)
5. voidm-cli/src/commands/stats.rs (1)
6. voidm-cli/src/commands/graph.rs (2)
7. voidm-mcp/src/lib.rs (1)
8. voidm-db/src/models.rs (1)

**OPTIONAL (Can mark experimental)**:
9. voidm-tagging/src/lib.rs (8)
10. voidm-ner/src/lib.rs (2)

## Summary
- **Total parasite sqlx**: 62 commands
- **Critical path violations**: 48 (crud.rs + graph + core utilities)
- **Optional violations**: 10 (tagging + ner)
- **Missing bridges**: 4 (cli/mcp)

