# Dependency Analysis: Why voidm-sqlite depends on voidm-core

## Summary

**voidm-sqlite depends on voidm-core BECAUSE voidm-sqlite calls utility functions back into voidm-core, not because of circular architecture.**

The dependency is ONE-WAY and LEGITIMATE, but it creates TIGHT COUPLING through utility functions.

---

## Current Dependency Flow

```
voidm-cli
    ↓
voidm-core::crud_trait (uses &dyn Database trait)
    ↓
voidm-sqlite::SqliteDatabase (implements trait)
    ↓
voidm-sqlite calls BACK to voidm-core for utilities
    ↓
voidm-core::crud, voidm-core::migrate, voidm-core::export
```

**This is NOT circular**, but it IS problematic because:
1. Backend should be independent layer
2. Utilities are scattered across voidm-core
3. Each backend must know about core utilities

---

## Detailed Dependency Breakdown

### Category 1: LEGITIMATE (type definitions, needed)

```rust
use voidm_core::models::{Memory, MemoryType, AddMemoryRequest, AddMemoryResponse};
use voidm_core::query::{QueryTranslator, SqliteTranslator, CypherOperation, QueryParams};
use voidm_core::Config;
```

**Status**: ✅ KEEP - These are abstract types, not backend-specific

**Count**: 3 modules (models, query, config)

---

### Category 2: PROBLEMATIC (backend calling core utilities)

#### 2a. ID Resolution (line 113, 230)
```rust
// voidm-sqlite/src/lib.rs:113
let full_id = voidm_core::crud::resolve_id_sqlite(&self.pool, id).await?;
```

**Problem**: 
- `resolve_id_sqlite()` is SQLite-specific but lives in voidm-core
- It's a backend implementation detail
- voidm-sqlite should implement its own ID resolution

**Solution**: Move to voidm-sqlite as `self.resolve_id()`

#### 2b. Scope Retrieval (line 133, 196)
```rust
// voidm-sqlite/src/lib.rs:133
let scopes = voidm_core::crud::get_scopes(&self.pool, &id).await?;
```

**Problem**:
- `get_scopes()` operates on pool but lives in voidm-core
- Should be part of backend's data retrieval
- Makes voidm-sqlite dependent on core internals

**Solution**: Move to voidm-sqlite as async method

#### 2c. Memory Type Conversion (line 142, 205)
```rust
// voidm-sqlite/src/lib.rs:142
let quality_mt = voidm_core::crud::convert_memory_type(&memory_type_enum);
```

**Problem**:
- Converting MemoryType to quality scores shouldn't be in crud module
- Utility logic scattered across modules
- Backend shouldn't need to call core for type conversion

**Solution**: Move to voidm_core::models module (shared utility)

#### 2d. Full add_memory (line 22 in add_memory_backend.rs)
```rust
// voidm-sqlite/src/add_memory_backend.rs:22
voidm_core::crud::add_memory(pool, req, config).await
```

**Problem**:
- Completely backwards - backend calling core which still has sqlx!
- Defeats purpose of Phase 1.5 extraction
- Creates circular-like dependency despite one-way imports

**Solution**: Extract core logic to core, transaction logic to backend

#### 2e. Migration (line 380)
```rust
// voidm-sqlite/src/lib.rs:380
voidm_core::migrate::run(&pool).await?;
```

**Problem**:
- Migration is backend-specific but lives in voidm-core
- Pool is SQLite-specific but being called from generic code

**Solution**: Move to voidm-sqlite::schema or similar

#### 2f. Similarity Computation (line 975)
```rust
// voidm-sqlite/src/lib.rs:975
if let Ok(similarity) = voidm_core::similarity::cosine_similarity(...) {
```

**Problem**:
- Search utility shouldn't depend on voidm-core::similarity
- Could be in voidm-scoring or self-contained

**Solution**: Move to shared scoring or inline in backend

#### 2g. Export Utilities (lines 1019+)
```rust
// voidm-sqlite/src/lib.rs:1019
let memory_record = voidm_core::export::MemoryRecord { ... };
if let Ok(json) = voidm_core::export::record_to_jsonl(&record) {
```

**Problem**:
- Export is generic but lives in voidm-core
- Backend needs to call core for serialization

**Solution**: Move to voidm-db-trait or keep in voidm-core (it's generic)

#### 2h. Chunk Nodes (line 1759)
```rust
// voidm-sqlite/src/lib.rs:1759
use voidm_core::chunk_nodes;
```

**Problem**:
- Chunk operations shouldn't be in voidm-core
- They're backend-specific node operations

**Solution**: Move chunk_nodes to voidm-sqlite

---

## Count of Problematic Dependencies

| Function | Location | Lines | Fix |
|----------|----------|-------|-----|
| resolve_id_sqlite | crud.rs | 113, 230 | Move to voidm-sqlite |
| get_scopes | crud.rs | 133, 196 | Move to voidm-sqlite |
| convert_memory_type | crud.rs | 142, 205 | Move to models |
| add_memory | crud.rs | add_memory_backend.rs:22 | **CRITICAL** |
| migrate::run | migrate.rs | 380 | Move to voidm-sqlite |
| similarity::cosine_similarity | similarity.rs | 975 | Move to voidm-scoring |
| export::* | export.rs | 1019+ | Keep in core (generic) |
| chunk_nodes | chunk_nodes.rs | 1759 | Move to voidm-sqlite |

**Total Problematic**: 7 functions/modules
**Total Lines in voidm-sqlite that call back to voidm-core**: ~15-20 lines

---

## The Real Problem: add_memory

The WORST offender is line 22 in add_memory_backend.rs:

```rust
pub async fn execute_add_memory_transaction_wrapper(
    pool: &SqlitePool,
    req: AddMemoryRequest,
    config: &voidm_core::Config,
) -> Result<AddMemoryResponse> {
    // THIS IS THE PROBLEM!
    voidm_core::crud::add_memory(pool, req, config).await
}
```

**Why this is wrong**:
1. We created backend module to EXTRACT transaction logic
2. But wrapper still calls voidm-core::add_memory
3. Which STILL has all the sqlx code!
4. Defeats the entire Phase 1.5 purpose

**Why this happened**:
- Circular dependency issue prevented calling voidm_sqlite from voidm_core
- So we "cheated" by calling back to core
- But core's add_memory still has sqlx

**Correct approach**:
```rust
// voidm-core::add_memory should:
// 1. Do pre-tx logic (validation, embeddings, scoring)
// 2. Call backend via TRAIT to execute transaction
// 3. Do post-tx logic (extract, link)

// voidm-sqlite backend should:
// 1. Receive pre-computed data
// 2. Execute transaction only
// 3. Return response

// voidm-sqlite trait method should:
// 1. Deserialize JSON
// 2. Call backend::execute_add_memory_transaction()
// 3. Return JSON
```

---

## Resolution Strategy

### Short Term (Phase 1.5 fix - 2-3 hours)
1. Extract pure transaction logic to add_memory_backend (DONE ✓)
2. Keep core::add_memory as-is for backward compat
3. Move resolve_id_sqlite to voidm-sqlite (30 min)
4. Move get_scopes to voidm-sqlite (30 min)
5. Move convert_memory_type to models (15 min)

### Medium Term (Phase 1.6+ - 4-6 hours)
1. Extract all migration logic to voidm-sqlite::schema
2. Move similarity to voidm-scoring
3. Move chunk_nodes logic to voidm-sqlite
4. Clean export utilities (already generic, keep in core)

### Long Term (Phase 2 - future)
1. Make all backends implement common schema trait
2. Support multiple backends without calling core utilities
3. Full isolation: backends only call trait, never back to core

---

## Recommendation

**voidm-sqlite's dependency on voidm-core is NOT a problem with the dependency direction, it's a problem with HOW utilities are organized.**

Fix by:
1. Moving SQLite-specific utilities FROM voidm-core TO voidm-sqlite
2. Moving generic utilities TO voidm-db-trait or voidm-scoring
3. Only keeping domain logic (models, config, business rules) in voidm-core

This will:
- Keep one-way dependency (voidm-sqlite → voidm-core models/config only)
- Reduce coupling
- Enable backend independence
- Support future backends easily

