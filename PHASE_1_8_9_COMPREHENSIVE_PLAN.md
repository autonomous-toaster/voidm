# Phase 1.8-1.9: Complete Internal Cleanup to 100%

## Current State

**Phase 1 Progress**: 58% (53/126 violations remaining)
**Violations by Crate**:
- voidm-sqlite: 91 (expected, backend)
- voidm-core: 21 (refactorable)
- voidm-graph: 26 (requires trait work)
- voidm-cli: 19 (CLI commands)
- voidm-tagging: 8 (feature code)
- voidm-ner: 2 (feature code)
- voidm-db: 1 (foundation, acceptable)
- voidm-mcp: 1 (trait bridge, ok)

**Target**: Get to ~0 violations (or acceptable baseline)

---

## Phase 1.8: voidm-cli Refactoring (1-2 hours)

### Analysis

**Current Issue**: voidm-cli has 19 sqlx violations
- `commands/graph.rs`: 10 direct sqlx queries
- `commands/stats.rs`: 9 direct sqlx queries

**Pattern**: Both execute direct sqlx queries instead of using Database trait

**Solution**: Create trait methods for these operations

### Plan

#### Task 1: Extract graph statistics (5 violations, 30 min)

**Current** (graph.rs):
```rust
let memories: Vec<(String, String, String)> = sqlx::query_as(
    "SELECT id, type, properties FROM nodes WHERE type = 'Memory'"
).fetch_all(&pool).await?;
```

**Target**: Create `Database::get_graph_statistics()` trait method

**Steps**:
1. Add trait method to voidm-db/src/lib.rs
2. Implement in voidm-sqlite backend
3. Call from voidm-cli

#### Task 2: Extract stats operations (9 violations, 30 min)

**Current** (stats.rs):
```rust
let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM memories")
    .fetch_one(&pool).await?;
```

**Target**: Create `Database::get_statistics()` trait method

**Trait method returns**:
```rust
pub struct DatabaseStats {
    pub total_memories: i64,
    pub by_type: Vec<(String, i64)>,
    pub scope_count: i64,
    pub total_tags: i64,
    pub node_count: i64,
    pub edge_count: i64,
    pub edge_by_type: Vec<(String, i64)>,
    pub vec_count: i64,
}
```

**Steps**:
1. Define DatabaseStats struct in voidm-db
2. Add get_statistics() trait method
3. Implement in voidm-sqlite
4. Call from voidm-cli

#### Task 3: Extract graph operations (4 violations, 15 min)

**Current**: Direct query execution in graph.rs

**Target**: Move to trait methods

**Steps**:
1. Identify specific operations
2. Create trait methods
3. Implement in voidm-sqlite
4. Update voidm-cli calls

### Result
- ✅ 19 violations eliminated
- ✅ voidm-cli pure (no sqlx)
- ✅ voidm-core unaffected
- **New total**: ~34/126 (73%)

---

## Phase 1.9: voidm-graph Refactoring (2-3 hours)

### Analysis

**Current Issue**: voidm-graph has 26 sqlx violations

**Files**:
- `ops.rs`: 9 sqlx violations (graph operations)
- `traverse.rs`: 13 sqlx violations (graph traversal)
- `cypher/mod.rs`: 4 sqlx violations (cypher translation)

**Problem**: voidm-graph is NOT a backend crate, but has sqlx code

**Root Cause**: Graph operations directly execute queries instead of using trait

### Solution: Create GraphOps Trait

#### Design Pattern

**New Architecture**:
```rust
// In voidm-db (foundation)
pub trait GraphOps {
    async fn create_node(&self, ...) -> Result<i64>;
    async fn create_edge(&self, ...) -> Result<i64>;
    async fn traverse(&self, ...) -> Result<Vec<Node>>;
    async fn get_node_properties(&self, ...) -> Result<HashMap<String, Value>>;
    // ... more operations
}

// In voidm-sqlite (backend)
impl GraphOps for SqliteDatabase {
    async fn create_node(...) { /* sqlx code here */ }
    async fn create_edge(...) { /* sqlx code here */ }
    async fn traverse(...) { /* sqlx code here */ }
    // ...
}

// In voidm-graph (logic)
pub async fn traverse_graph(graph_ops: &dyn GraphOps, ...) -> Result<...> {
    // No sqlx here, use trait methods
}
```

#### Implementation Tasks

##### Task 1: Define GraphOps trait (1 hour)

**Methods to define**:
1. Node operations:
   - `create_node(id, label, properties)`
   - `get_node(id)`
   - `list_nodes(label)`
   - `delete_node(id)`
   - `update_node_properties(id, properties)`

2. Edge operations:
   - `create_edge(source_id, target_id, rel_type, properties)`
   - `get_edge(source_id, target_id, rel_type)`
   - `list_edges(source_id, rel_type)`
   - `delete_edge(edge_id)`

3. Traversal operations:
   - `traverse(start_id, rel_type, depth, direction)`
   - `get_connected_nodes(node_id, rel_type)`
   - `path_query(start_id, end_id, rel_type)`

4. Query operations:
   - `execute_cypher(cypher_query)`
   - `get_node_properties(node_id)`
   - `get_relationships(node_id)`

**File**: Create `voidm-db/src/graph_ops.rs`

**Steps**:
1. Define all trait methods
2. Document expected behavior
3. Add to voidm-db/src/lib.rs exports

##### Task 2: Implement GraphOps in voidm-sqlite (1 hour)

**File**: `voidm-sqlite/src/graph_ops_impl.rs`

**Process**:
1. Copy sqlx code from voidm-graph
2. Organize by operation type
3. Implement GraphOps trait
4. Verify all 26 queries are covered

##### Task 3: Refactor voidm-graph to use trait (30 min)

**Files affected**:
- `ops.rs`: Update to use trait methods
- `traverse.rs`: Update to use trait methods
- `cypher/mod.rs`: Update to use trait methods

**Process**:
1. Replace sqlx calls with trait method calls
2. Update function signatures to accept `&dyn GraphOps`
3. Verify no sqlx remains

##### Task 4: Update callers of voidm-graph (30 min)

**Who calls voidm-graph**:
- voidm-cli (graph commands)
- voidm-mcp (link operations)
- voidm-core (search queries)

**Process**:
1. Find all voidm-graph calls
2. Pass `&db` (trait object) instead of `&pool`
3. Verify builds and tests pass

### Result
- ✅ 26 violations eliminated from voidm-graph
- ✅ voidm-graph pure logic (no sqlx)
- ✅ Clean trait boundary established
- ✅ GraphOps can be implemented by any backend
- **New total**: ~8/126 (94%)

---

## Phase 1.9: Final Cleanup (1-2 hours)

### Remaining 8 Violations

**voidm-tagging**: 8 violations
- Feature code, optional
- Can be deferred to Phase 2

**voidm-ner**: 2 violations
- Feature code, optional
- Can be deferred to Phase 2

**Options**:
1. Extract tagging/ner to backend (complex, 1-2h)
2. Mark as optional/experimental (30 min)
3. Leave as-is (acceptable for optional features)

### Recommended: Mark as Experimental

**Rationale**:
- NER/Tagging are optional features
- Only enabled with feature flags
- Don't affect core functionality
- Can be cleaned up in Phase 2+ if needed

**Process** (30 min):
1. Add comments to voidm-tagging marking as experimental
2. Add comments to voidm-ner marking as experimental
3. Document feature flag usage
4. Note in FEATURES.md that these are optional

### Result
- Core path: ✅ 0 violations (100% clean)
- Optional features: 10 violations (acceptable)
- **Final state**: Pure, clean core + experimental features

---

## Phase 1.8-1.9 Summary

### Total Time: 4-5 hours

| Task | Effort | Violations | Result |
|------|--------|-----------|--------|
| 1.8: CLI refactor | 1-2h | 19 → 0 | ✅ |
| 1.9a: Graph trait | 2-2.5h | 26 → 0 | ✅ |
| 1.9b: Mark optional | 0.5h | 10 marked | ✅ |
| **Total** | **4-5h** | **55 → 10** | **✅** |

### Final Violation State

**Core Path** (production-critical):
- voidm-db: 1 (foundation, ok)
- voidm-core: 21 → 0 (refactored)
- voidm-sqlite: 91 (expected, backend)
- voidm-mcp: 1 (bridge, ok)
- voidm-cli: 19 → 0 (refactored)
- voidm-graph: 26 → 0 (refactored)
- **Core violations: 0** ✅

**Optional Features**:
- voidm-tagging: 8 (marked experimental)
- voidm-ner: 2 (marked experimental)
- **Optional violations: 10** (acceptable)

**Total: 10 violations** (all optional, marked experimental)

---

## Success Criteria

✅ Zero sqlx violations in production core code
✅ All optional features marked experimental
✅ All trait boundaries clean
✅ All tests passing
✅ Build completely clean
✅ Ready for Phase 2

---

## Phase 1 Final: 100% Complete

**After Phase 1.8-1.9**:
- Core path: ✅ CLEAN (0 violations)
- Architecture: ✅ PURE (trait-based)
- Foundation: ✅ SOLID (ready for anything)
- Optional features: ✅ MARKED
- Documentation: ✅ UPDATED

**Next**: Phase 2 (features) with complete confidence

