# Phase 1.9 Refactoring Guide: voidm-graph to Trait-Based

## Current Status

### Foundation READY ✅
- `voidm-db/src/graph_ops.rs`: GraphQueryOps trait with 13 methods (DONE)
- `voidm-sqlite/src/graph_query_ops_impl.rs`: Full implementation with all sqlx queries (DONE)
- Build: 14/14 crates, 0 errors (VERIFIED)

### Remaining Work: Refactor voidm-graph to use trait
**Estimated Time**: 1-2 hours
**Violations to Eliminate**: 26
**Files to Modify**: 4 + 1 (caller)

## Step-by-Step Refactoring Plan

### Step 1: Update voidm-graph/src/lib.rs

**File**: `crates/voidm-graph/src/lib.rs`

Add import:
```rust
use voidm_db::graph_ops::GraphQueryOps;
```

**Current**: Functions take `&SqlitePool`
**Target**: Functions take `&dyn GraphQueryOps` (or `&impl GraphQueryOps`)

### Step 2: Refactor voidm-graph/src/ops.rs

**Functions to Update** (3 functions, 9 sqlx usages):

#### 2.1 `upsert_node`
**Current**:
```rust
pub async fn upsert_node(pool: &SqlitePool, memory_id: &str) -> Result<i64> {
    sqlx::query("INSERT OR IGNORE INTO graph_nodes (memory_id) VALUES (?)")
        .bind(memory_id)
        .execute(pool)
        .await?;
    let id: i64 = sqlx::query_scalar("SELECT id FROM graph_nodes WHERE memory_id = ?")
        .bind(memory_id)
        .fetch_one(pool)
        .await?;
    Ok(id)
}
```

**Target**:
```rust
pub async fn upsert_node(ops: &dyn GraphQueryOps, memory_id: &str) -> Result<i64> {
    ops.upsert_node(memory_id).await
}
```

**Note**: Can either be a thin wrapper (as above) or removed entirely and callers use trait directly.

#### 2.2 `delete_node`
**Similar approach**: Replace pool with trait, call `ops.delete_node()`.

#### 2.3 `upsert_edge`
**Similar approach**: Replace pool with trait, call `ops.upsert_edge()`.

#### 2.4 `delete_edge`
**Similar approach**: Replace pool with trait, call `ops.delete_edge()`.

### Step 3: Refactor voidm-graph/src/traverse.rs

**Functions to Update** (3 functions, ~13 sqlx usages):

#### 3.1 `neighbors`
**Changes Required**:
- Function signature: `pool: &SqlitePool` → `ops: &dyn GraphQueryOps`
- Replace `sqlx::query_scalar("SELECT id FROM graph_nodes WHERE memory_id = ?")` with `ops.get_node_id()`
- Replace outgoing query with `ops.get_outgoing_edges(node_id)`
- Replace incoming query with `ops.get_incoming_edges(node_id)`

**Pseudocode**:
```rust
pub async fn neighbors(
    ops: &dyn GraphQueryOps,
    memory_id: &str,
    depth: u8,
    rel_filter: Option<&str>,
) -> Result<Vec<NeighborResult>> {
    let mut visited: HashSet<String> = HashSet::new();
    visited.insert(memory_id.to_string());
    let mut results = Vec::new();
    let mut frontier: Vec<(String, u8)> = vec![(memory_id.to_string(), 0)];

    while let Some((current_id, current_depth)) = frontier.pop() {
        if current_depth >= depth { continue; }

        // CHANGE: use trait method
        let current_node: Option<i64> = ops.get_node_id(&current_id).await?;
        let node_id = match current_node {
            Some(n) => n,
            None => continue,
        };

        // CHANGE: use trait method
        let outgoing: Vec<_> = ops.get_outgoing_edges(node_id).await?;
        
        for (neighbor_id, rel_type, note) in outgoing {
            // ... rest of logic unchanged
        }

        // CHANGE: use trait method
        let incoming: Vec<_> = ops.get_incoming_edges(node_id).await?;
        
        // ... rest of logic unchanged
    }

    Ok(results)
}
```

#### 3.2 `shortest_path`
**Changes Required**:
- Similar: `pool` → `ops: &dyn GraphQueryOps`
- Replace `sqlx::query_scalar("SELECT id FROM graph_nodes WHERE memory_id = ?")` with `ops.get_node_id()`
- Replace edge query with `ops.get_all_edges(nid)`

#### 3.3 `pagerank`
**Changes Required**:
- Replace all sqlx queries with trait methods:
  - `sqlx::query_as("SELECT source_id, target_id FROM graph_edges")` → `ops.get_all_memory_edges()`
  - `sqlx::query_as("SELECT id, memory_id FROM graph_nodes")` → `ops.get_all_memory_nodes()`
  - `sqlx::query_as("SELECT id FROM ontology_concepts")` → `ops.get_all_concept_nodes()`
  - `sqlx::query_as("SELECT from_id, to_id FROM ontology_edges")` → `ops.get_all_ontology_edges()`
- Rest of PageRank algorithm remains unchanged (pure Rust logic)

### Step 4: Refactor voidm-graph/src/cypher/mod.rs

**Function to Update** (1 function, ~4 sqlx usages):

#### 4.1 `execute_read`
**Current**: Takes `pool: &SqlitePool`, calls `run_query(pool, sql, params)`
**Target**: Takes `ops: &dyn GraphQueryOps`, calls `ops.execute_cypher(sql, params)`

**Changes**:
```rust
pub async fn execute_read(
    ops: &dyn GraphQueryOps,  // CHANGE: was pool
    query: &str,
) -> Result<Vec<HashMap<String, serde_json::Value>>> {
    // Step 1-4: Parse, validate, translate (unchanged)
    let (sql, params) = translator::translate(&ast)?;

    // Step 5: CHANGE - use trait
    let rows = ops.execute_cypher(&sql, &params).await?;
    Ok(rows)
}
```

#### 4.2 Remove `run_query` function
**Current**: Implements dynamic sqlx parameter binding
**After**: Moved to SqliteGraphQueryOps::execute_cypher in backend
**Action**: Delete the entire `run_query` function from cypher/mod.rs

### Step 5: Update voidm-cli/src/commands/graph.rs

**File**: `crates/voidm-cli/src/commands/graph.rs`

**Changes Required**:

#### 5.1 Add imports
```rust
use voidm_sqlite::graph_query_ops_impl::SqliteGraphQueryOps;
```

#### 5.2 Update `run` function
Create the trait object and pass to graph functions:

```rust
pub async fn run(cmd: GraphCommands, db: &std::sync::Arc<dyn voidm_db::Database>, pool: &sqlx::SqlitePool, json: bool) -> Result<()> {
    let graph_ops: SqliteGraphQueryOps = SqliteGraphQueryOps::new(pool.clone());
    let graph_ops_ref: &dyn GraphQueryOps = &graph_ops;
    
    match cmd {
        GraphCommands::Cypher(args) => run_cypher(args, graph_ops_ref, json).await,
        GraphCommands::Neighbors(args) => run_neighbors(args, db, graph_ops_ref, json).await,
        GraphCommands::Path(args) => run_path(args, db, graph_ops_ref, json).await,
        GraphCommands::Pagerank(args) => run_pagerank(args, graph_ops_ref, json).await,
        GraphCommands::Stats => run_stats(db, pool, json).await,
        GraphCommands::Export(args) => run_export(args, db, pool, json).await,
    }
}
```

#### 5.3 Update function signatures
- `run_cypher`: Add `ops: &dyn GraphQueryOps` parameter
- `run_neighbors`: Add `ops: &dyn GraphQueryOps` parameter
- `run_path`: Add `ops: &dyn GraphQueryOps` parameter
- `run_pagerank`: Add `ops: &dyn GraphQueryOps` parameter
- Update calls to `voidm_graph::` functions to pass `ops` instead of `pool`

**Example**:
```rust
async fn run_neighbors(args: NeighborsArgs, db: &Arc<dyn Database>, ops: &dyn GraphQueryOps, json: bool) -> Result<()> {
    // ... id resolution ...
    let results = voidm_graph::neighbors(ops, &id, args.depth, args.rel.as_deref()).await?;
    // ... formatting ...
}
```

### Step 6: Remove sqlx imports from voidm-graph

**Files to Update**:
- `crates/voidm-graph/src/ops.rs`: Remove `use sqlx::SqlitePool;`
- `crates/voidm-graph/src/traverse.rs`: Remove `use sqlx::SqlitePool;`
- `crates/voidm-graph/src/cypher/mod.rs`: Remove `use sqlx::SqlitePool;` and sqlx usage

### Step 7: Verify

**Build**:
```bash
cargo build
```
Expected: 14/14 crates, 0 errors

**Test all graph commands**:
```bash
voidm graph stats
voidm graph export --format dot | head -20
```

**Verify violations eliminated**:
```bash
rg "sqlx::" crates/voidm-graph/src/
```
Expected: 0 results

## Violations Eliminated

| File | Before | After | Method |
|------|--------|-------|--------|
| ops.rs | 9 | 0 | Replaced with trait calls |
| traverse.rs | 13 | 0 | Replaced with trait calls |
| cypher/mod.rs | 4 | 0 | Replaced with trait calls |
| **Total** | **26** | **0** | **All replaced** |

## Summary

This refactoring:
- ✅ Eliminates 26 sqlx violations from voidm-graph
- ✅ Keeps all business logic in voidm-graph (pure Rust)
- ✅ Moves all database operations to trait/backend
- ✅ Results in Phase 1 → 94% (only 8 optional features remain)
- ✅ Follows established three-layer pattern

**Total Expected Time**: 1-2 hours
**Risk Level**: Low (pattern proven, clear path)
**Build Status After**: 14/14, 0 errors (expected)

