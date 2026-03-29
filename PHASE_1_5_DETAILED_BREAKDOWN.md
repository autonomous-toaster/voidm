# Phase 1.5 Detailed Breakdown: add_memory Extraction

## Current Flow (in voidm-core/crud.rs)

### Pre-Transaction Logic (lines 63-145)
```rust
1. Redaction (lines 75-85)
   - redact_memory(&mut req, config, &mut redaction_warnings)
   
2. ID generation (line 87)
   - id = req.id OR new UUID
   
3. Timestamp (line 88)
   - now = Utc::now()
   
4. Metadata setup (lines 90-101)
   - Set author="user" if not present
   - Serialize tags/metadata/type to JSON
   
5. Embedding computation (lines 103-112)
   - embeddings::embed_text_chunked()
   
6. Quality score computation (lines 114-117)
   - voidm_scoring::compute_quality_score()
   
7. Vector table ensure (lines 119-127)
   - vector::ensure_vector_table() - HAS 2 sqlx CALLS (MOVE TO BACKEND)
   - db_meta INSERT queries x2 - (MOVE TO BACKEND)
   
8. Link target validation & resolution (lines 129-145)
   - resolve_id_sqlite() for each link (MOVE TO BACKEND for ID lookup)
   - Validate RELATES_TO has note
```

### Transaction Logic (lines 147-480)
```rust
Begin transaction (line 147)

1. Insert memory (lines 150-166)
   - sqlx::query("INSERT INTO memories ...")
   
2. Insert scopes (lines 168-173)
   - Loop: sqlx::query("INSERT OR IGNORE INTO memory_scopes ...")
   
3. Insert FTS (lines 175-179)
   - sqlx::query("INSERT INTO memories_fts ...")
   
4. Insert embedding (lines 181-189)
   - if embedding: sqlx::query("INSERT INTO vec_memories ...")
   
5. Graph node upsert (lines 191-201)
   - sqlx::query("INSERT OR IGNORE INTO graph_nodes ...")
   - sqlx::query_scalar("SELECT id FROM graph_nodes ...")
   - sqlx::query("INSERT OR IGNORE INTO graph_node_labels ...")
   
6. Store memory_type property (lines 203-211)
   - intern_property_key() - has 2 sqlx calls (lines 750-760)
   - sqlx::query("INSERT OR REPLACE INTO graph_node_props_text ...")
   
7. Create --link edges (lines 213-228)
   - Loop for each link:
     - sqlx::query_scalar("SELECT n.id FROM graph_nodes ...")
     - sqlx::query("INSERT OR IGNORE INTO graph_edges ...")

Commit transaction (line 230)

Post-Transaction: Extract & link concepts (lines 232-241)
- If auto_extract_concepts: extract_and_link_concepts()
```

## Extraction Plan

### Create add_memory_impl() in voidm-sqlite

#### Copy entire transaction block (lines 147-480)
- Keep all sqlx calls as-is
- Change `&mut *tx` to `&mut tx`
- Move all transaction logic here

#### Dependencies
```rust
Needs from voidm-core:
- AddMemoryRequest, AddMemoryResponse (models)
- Config
- redaction, embeddings modules (for pre-tx logic)

Calls within voidm-sqlite:
- All sqlx operations on self.pool or transaction
```

### Create voidm-core wrapper

#### New add_memory() signature
```rust
pub async fn add_memory(
    db: &dyn Database,
    mut req: AddMemoryRequest, 
    config: &Config
) -> Result<AddMemoryResponse> {
    // 1. Pre-tx: redaction, embeddings, scoring
    // ... (keep all current pre-tx logic)
    
    // 2. Call db.add_memory()
    let resp = db.add_memory(req_json, config_json).await?;
    
    // 3. Parse response
    // ... (deserialize JSON response to AddMemoryResponse)
    
    Ok(resp)
}
```

### Update trait method

In voidm-db-trait/src/lib.rs:
```rust
fn add_memory(
    &self,
    req_json: serde_json::Value,
    config_json: &serde_json::Value,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value>> + Send + '_>>;
```

In voidm-sqlite/src/lib.rs (trait impl):
```rust
fn add_memory(...) -> ... {
    Box::pin(async move {
        let req = serde_json::from_value(req_json)?;
        let config = serde_json::from_value(config_json)?;
        let resp = self.add_memory_impl(&req, &config).await?;
        Ok(serde_json::to_value(resp)?)
    })
}
```

## Tricky Parts

1. **Pre-tx calls to resolve_id_sqlite()**
   - These happen BEFORE transaction
   - Pass resolved IDs into transaction
   - Keep as-is (or make generic later)

2. **intern_property_key() inside transaction**
   - Small helper function (lines 750-760)
   - Move to voidm-sqlite or keep in voidm-core

3. **extract_and_link_concepts() post-tx**
   - Optional feature
   - Should stay in voidm-core (business logic)
   - Keep as separate async call

4. **JSON serialization round-trip**
   - Might lose type info
   - Ensure AddMemoryRequest/Response are properly serde
   - Test with real memory creation

## Timeline

- **Step 1**: Copy transaction block to voidm-sqlite (20 min)
- **Step 2**: Extract pre-tx logic references (20 min)
- **Step 3**: Create voidm-core wrapper (30 min)
- **Step 4**: Update trait impl (15 min)
- **Step 5**: Update MCP caller (10 min)
- **Step 6**: Build + test (45 min)

**Total**: 2.5-3 hours
