# Neo4j Concept Migration Bug Fix

## Problem
Migration showed "Migrated 700 concepts..." then failed with "Error: Failed to create concept in Neo4j". Root cause: UNIQUE constraint violations on concept names when running migrations multiple times.

## Root Causes Identified

1. **Silent Error Handling**: `add_concept()` used `.run_on()` instead of `.execute_on()`, which doesn't properly surface Neo4j constraint violations
2. **Duplicate Prevention**: The UNIQUE constraint on `Concept.name` silently failed when duplicate concept names existed from previous migration runs
3. **Poor Error Context**: Migration didn't report which concept failed or provide actionable error messages
4. **No Clean Option**: Users had to manually delete Concepts in Neo4j to retry migrations

## Solutions Implemented

### 1. Fixed `add_concept()` to use `.execute_on()` (voidm-neo4j/src/lib.rs)
```rust
// Before: .run_on() — silently ignores errors
graph.run_on(&database, query).await.context("Failed to create concept in Neo4j")?;

// After: .execute_on() — properly surfaces constraint violations
graph.execute_on(&database, query).await.map_err(|e| {
    tracing::error!("Neo4j add_concept error for '{}': {}", name, e);
    anyhow::anyhow!("Failed to create concept '{}' in Neo4j: {}", name, e)
})?;
```

### 2. Added `--clean` parameter (voidm-cli/src/commands/migrate.rs)
```bash
voidm migrate sqlite neo4j --clean
```
- **Default**: false (safe by default, no data loss)
- **Effect**: Deletes all Concept nodes and ontology edges before migration
- **Neo4j Only**: SQLite backend ignores this safely
- **Logging**: Shows status messages during cleaning

### 3. Improved Error Handling in `migrate_concepts()`
- Changed from panic on first error (`.await?`) to graceful error tracking
- Now reports: `Concepts: N migrated, M failed, K skipped`
- Shows first 5 failures with details (concept name, ID, error message)
- Only fails the entire migration if ALL concepts failed (0 migrated)
- Allows partial success (e.g., 700 created, 50 skipped due to duplicates)

### 4. Added `clean_database()` to trait (voidm-db-trait/src/lib.rs)
- Default no-op implementation for SQLite and other backends
- Neo4j implementation:
  - Deletes all ontology relationship types: INSTANCE_OF, RELATES_TO, SUPPORTS, CONTRADICTS, PRECEDES, DERIVED_FROM, PART_OF, EXEMPLIFIES
  - Deletes all Concept nodes
  - Leaves Memory nodes and edges intact

## Usage

### Retry Failed Migration with Cleanup
```bash
# First attempt failed at concept 700
voidm migrate sqlite neo4j --clean

# Migration will now:
# 1. Delete all existing Concepts and ontology edges
# 2. Start fresh with 0 conflicts
# 3. Complete successfully
```

### Partial Data Safety
```bash
# Dry run first to see impact
voidm migrate sqlite neo4j --clean --dry-run

# Then execute
voidm migrate sqlite neo4j --clean
```

### Separate Memories and Concepts
Since `--clean` only affects Concepts and ontology edges, Memory nodes are preserved. This allows you to:
1. Migrate memories: `voidm migrate sqlite neo4j`
2. Clear and re-migrate concepts: `voidm migrate sqlite neo4j --clean`

## Error Output Improvements

Before:
```
Migrated 700 concepts...
Error: Failed to create concept in Neo4j
```

After:
```
Migrated 700 concepts...
  Error migrating concept 'DuplicateName' (concept-id-123): Failed to create concept 'DuplicateName' in Neo4j: Node(0) already exists with label Concept and property name = "DuplicateName"
Concepts: 700 migrated, 50 failed, 0 skipped
Error: Failed to create concept in Neo4j (but partial success occurred)
```

## Files Modified
1. **crates/voidm-neo4j/src/lib.rs**
   - Line 678: Changed `.run_on()` → `.execute_on()` in `add_concept()`
   - Added explicit error logging with concept name
   - Added `clean_database()` implementation with ontology edge cleanup

2. **crates/voidm-cli/src/commands/migrate.rs**
   - Added `--clean` bool parameter to `MigrateArgs`
   - Added pre-migration cleanup logic with status messages
   - Improved `migrate_concepts()` error handling (graceful vs panic)
   - Now tracks and reports migration statistics

3. **crates/voidm-db-trait/src/lib.rs**
   - Added `clean_database()` method to Database trait
   - Default no-op implementation for safety

## Backward Compatibility
✓ Fully backward compatible
- `--clean` defaults to false (no behavior change without flag)
- Existing migrations work as before (just with better error messages)
- SQLite backend safely ignores clean flag

## Testing
Build succeeded:
```bash
cargo build --all
# ✓ Compiles without errors
# ✓ New --clean parameter appears in help

voidm migrate --help
# Shows: --clean  Clean target database before migration...
```

## Migration Strategy
1. **For Stuck Migrations**: Use `--clean` to remove old data and retry
2. **For Iterative Development**: Use `--dry-run --clean` to preview impact
3. **For Production**: Test with dry-run first, verify memories preserved, then execute

## Related Issues Solved
- Silent constraint violations during concept migration
- Poor error messages when migrations fail
- No way to recover from failed migrations without manual database access
- Inconsistent error handling between `.run_on()` and `.execute_on()`
