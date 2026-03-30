# Phase 1.9b: Remove crud.rs _sqlite Compat Functions

## Goal
Remove direct sqlx usage from voidm-core/src/crud.rs by:
1. Deleting _sqlite compat functions (which have trait versions)
2. Adding missing functions to Database trait
3. Moving SQLite-specific logic to backend

## Current Functions in crud.rs

### Already Trait-Based (Keep using trait versions):
- ✅ `resolve_id()` - uses trait
- ✅ `get_memory()` - uses trait
- ✅ `list_memories()` - uses trait
- ✅ `delete_memory()` - uses trait
- ✅ `link_memories()` - uses trait
- ✅ `unlink_memories()` - uses trait

### SQLite Compat Functions (Delete these):
- ❌ `resolve_id_sqlite()` - has trait version, delete
- ❌ `get_memory_sqlite()` - has trait version, delete
- ❌ `get_scopes()` - SQLite-specific, move to trait
- ❌ `list_scopes()` - SQLite-specific, move to trait
- ❌ `list_edges()` - SQLite-specific, move to trait
- ❌ `check_model_mismatch()` - SQLite-specific, move to backend
- ❌ `get_or_create_node()` - helper (5 lines), inline or move to trait
- ❌ `link_memories_sqlite()` - old compat, delete
- ❌ `intern_property_key()` - helper (6 lines), inline or remove
- ❌ Plus extract_entity_concepts() has 14 sqlx lines

### Total sqlx in crud.rs:
- 20 sqlx:: instances to remove

## Action Plan

### 1. Add missing methods to Database trait (voidm-db/src/lib.rs)

```rust
// Add these 3 methods to Database trait:

fn get_scopes(&self, memory_id: &str) -> Pin<Box<dyn Future<Output = Result<Vec<String>>> + Send + '_>>;
// Get all scope strings for a memory

fn list_scopes(&self) -> Pin<Box<dyn Future<Output = Result<Vec<String>>> + Send + '_>>;
// List all known scope strings

fn list_edges(&self) -> Pin<Box<dyn Future<Output = Result<Vec<MemoryEdge>>> + Send + '_>>;
// Get all edges
```

### 2. Implement in voidm-sqlite/src/lib.rs

Each method implements its sqlx query using the pool.

### 3. Implement stubs in voidm-neo4j/src/lib.rs

Each method returns error("Neo4j not yet implemented").

### 4. Delete functions from crud.rs

- `resolve_id_sqlite()`
- `get_memory_sqlite()`
- `link_memories_sqlite()`
- `check_model_mismatch()`
- `get_or_create_node()`
- `intern_property_key()`
- `extract_entity_concepts()` - refactor to use traits

### 5. Update callers

Check which callers use _sqlite versions:
- voidm-cli uses `resolve_id()` (trait) - OK
- voidm-cli uses `get_memory()` (trait) - OK
- Other code should use trait versions

### 6. Refactor extract_entity_concepts()

This function has 14 sqlx instances. It's in crud.rs but needs:
- Query for existing concepts (trait method)
- Insert new concept (trait method)
- Create INSTANCE_OF edge (trait method)

Options:
A) Extract to separate module with trait-based operations
B) Inline into callers
C) Move whole function to backend

**Decision**: Move to backend as concept creation operation

