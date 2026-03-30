# Phase 1.7: Extract chunk_nodes - Plan

## Overview
Move chunking node storage logic from voidm-core to voidm-sqlite backend

## Current State
- **File**: crates/voidm-core/src/chunk_nodes.rs (142 lines)
- **Violations**: ~5 sqlx usages
- **Called by**: voidm-sqlite test code only
- **Exported from**: voidm-core/src/lib.rs

## Analysis

### chunk_nodes.rs Functions
1. `compute_chunk_positions()` - Pure logic, no sqlx
2. `store_chunks_as_nodes()` - Has sqlx queries (needs to move)

### callers:
- voidm-sqlite tests: `use voidm_core::chunk_nodes;`
- Tests call: `chunk_nodes::store_chunks_as_nodes()`

### Safe to Move?
- ✅ Yes - backend-specific code
- ✅ Only used in backend tests
- ✅ No other callers

## Extraction Steps

### Step 1: Copy file
- Create `crates/voidm-sqlite/src/chunk_nodes.rs`
- Copy all 142 lines

### Step 2: Add module
- Add `pub mod chunk_nodes;` to voidm-sqlite/src/lib.rs

### Step 3: Update tests
- Change `use voidm_core::chunk_nodes;` to `use crate::chunk_nodes;`

### Step 4: Remove from core
- Delete crates/voidm-core/src/chunk_nodes.rs
- Remove `pub mod chunk_nodes;` from voidm-core/src/lib.rs

### Step 5: Verify
- Build all crates
- Run tests
- No regressions

## Expected Result
- ~5 violations eliminated
- ~53/126 total (58% complete)
- Phase 1.7 COMPLETE in ~1 hour

