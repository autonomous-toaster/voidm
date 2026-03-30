# Phase 1.8: Refactor voidm-graph - Analysis

## Current State

### voidm-graph Violations: 26 lines across 3 files
1. `ops.rs` - Graph operations with sqlx
2. `traverse.rs` - Graph traversal with sqlx
3. `cypher/mod.rs` - Cypher-to-SQL translation with sqlx

### What is voidm-graph?
- Standalone graph database abstraction
- Used for link/relationship operations
- Implements Cypher-like query language
- Can work with SQLite backend

## Problem

### voidm-graph is NOT a backend crate
- It's a domain logic crate (similar to voidm-core)
- Should NOT have sqlx code
- Should be backend-agnostic

### Current Architecture (WRONG)
```
voidm-core → voidm-graph (with sqlx!) ← voidm-sqlite
```

### Correct Architecture
```
voidm-core → voidm-graph (pure logic)
             voidm-sqlite (has backend for graph)
```

## Solution: Three Approaches

### Option A: Extract graph operations to backend (2-3 hours)
- Move graph query execution to voidm-sqlite
- Make voidm-graph fully backend-agnostic
- Requires significant refactoring

### Option B: Create trait for graph operations (2 hours)
- Keep voidm-graph logic in core
- Create Graph trait in voidm-db
- Implement in voidm-sqlite
- Simpler, less invasive

### Option C: Leave as-is for now (defer to Phase 2)
- Mark violations as "Phase 1.8 future work"
- Focus on more critical extractions
- Phase 2 can do complete graph refactoring

## Recommendation

**Option B: Graph Trait Approach**

This balances:
- Clean architecture (trait-based)
- Manageable scope (1.5-2 hours)
- Clear separation (operations in backend)
- Backward compatible

### Process
1. Analyze voidm-graph sqlx usage
2. Create graph operations trait in voidm-db
3. Implement in voidm-sqlite
4. Update voidm-graph to use trait
5. Build & test

### Expected Result
- ~20 violations eliminated
- ~33/126 (75% complete)
- Clear pattern for Phase 2 work

