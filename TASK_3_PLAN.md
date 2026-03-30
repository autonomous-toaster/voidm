# Task 3: Move Backend Utilities to voidm-sqlite

## Current State
These functions live in voidm-core but are backend-specific:
1. `resolve_id_sqlite()` - Resolves short ID prefixes, SQLite-specific
2. `get_scopes()` - Gets scopes for a memory, backend-specific
3. `chunk_nodes` - Chunking operations, should be in backend

Current: Re-exported via voidm-sqlite/utils.rs (temporary)
Goal: Actually move them to voidm-sqlite

## Problem
- If we just move them directly, we break other callers in voidm-core
- Need to identify who calls each function
- Then either move them or create proper backend wrappers

## Analysis

### resolve_id_sqlite()
Current usage:
- crud.rs line 43: definition
- crud.rs line 113: resolve_id_sqlite (in add_memory_prepare - now in backend) ✓
- crud.rs line 230: resolve_id_sqlite (in another function - need to check)
- lib.rs: exported

Let me check who calls it...
