# Phase 1.6: Extract migrate.rs

## Overview
Move database migration code from voidm-core to voidm-sqlite

## Current State
- **File**: crates/voidm-core/src/migrate.rs (~200 lines)
- **Violations**: ~11 (all sqlx usage)
- **Called by**: 
  - voidm-sqlite/src/lib.rs (when creating database)
  - voidm-cli/src/commands/migrate.rs (CLI command)

## Plan

### Step 1: Copy migrate.rs to voidm-sqlite
- Create `crates/voidm-sqlite/src/migrate.rs`
- Copy content from voidm-core

### Step 2: Update voidm-sqlite/src/lib.rs
- Add `mod migrate;` declaration
- Update call from `voidm_core::migrate::run` to `crate::migrate::run`

### Step 3: Update voidm-cli
- Change import from `voidm_core::migrate` to `voidm_sqlite::migrate`
- Or: create a wrapper in voidm-core that delegates to backend

### Step 4: Remove from voidm-core
- Delete crates/voidm-core/src/migrate.rs
- Remove from voidm-core/src/lib.rs

### Step 5: Update exports
- voidm-core might re-export for backward compatibility
- Or: voidm-cli directly imports from voidm-sqlite

### Step 6: Verify
- Build all crates
- Run CLI migrate command
- Run integration tests

## Expected Result
- ~11 violations eliminated
- ~58/126 total (54% complete)
- Phase 1.6 COMPLETE in ~2 hours

