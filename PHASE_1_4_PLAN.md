# Phase 1.4 - Extract link_memories

## Scope
Move link_memories transaction from voidm-core to voidm-sqlite backend

## Functions to Move
1. `get_or_create_node(pool, memory_id)` → `get_or_create_node_impl(pool, memory_id)` in voidm-sqlite
2. `link_memories(pool, from_id, edge_type, to_id, note)` → `link_memories_impl()` in voidm-sqlite

## Implementation Steps
1. Create `link_memories_impl()` in voidm-sqlite with all sqlx logic
2. Add `link_memories()` method to Database trait (parameters: from_id, edge_type, to_id, note, config)
3. Implement in voidm-sqlite: call link_memories_impl
4. Update voidm-core `link_memories()` to:
   - Keep business logic (validation)
   - Call `db.link_memories()` via trait
   - Return LinkResponse

## Expected Violations Eliminated
- 2-3 sqlx calls from link_memories
- 2-3 sqlx calls from get_or_create_node
- Total: 5-6 violations

## Dependencies
- Need to pass EdgeType to trait
- Need to handle LinkResponse through trait (return JSON or custom type)

## Timeline
2-3 hours for complete implementation

## Files to Modify
- crates/voidm-db-trait/src/lib.rs (add trait method)
- crates/voidm-sqlite/src/lib.rs (implement _impl and trait method)
- crates/voidm-core/src/crud.rs (refactor to use trait)
