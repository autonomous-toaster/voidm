# voidm-sqlite

Independent SQLite backend implementation for voidm using the generic `Database` trait.

## Features

- **Backend Independence**: Implements the generic `voidm-db-trait::Database` trait
- **Type-Safe JSON Interface**: All I/O uses `serde_json::Value` for flexibility
- **Full Memory Operations**: CRUD, edges, ontology concept management
- **Schema Management**: Automatic table creation with `ensure_schema()`
- **Queryable**: QueryTranslator pattern for extensible query support

## Usage

```rust
use voidm_sqlite::{open_pool, SqliteDatabase};
use voidm_db_trait::Database;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Open connection pool
    let pool = open_pool(std::path::Path::new("memory.db")).await?;
    let db = SqliteDatabase::new(pool);
    
    // Initialize schema
    db.ensure_schema().await?;
    
    // Add a memory
    let req = serde_json::json!({
        "content": "My important thought",
        "type": "semantic",
        "importance": 5,
        "tags": ["important", "architecture"]
    });
    
    let resp = db.add_memory(req, &serde_json::json!({})).await?;
    let id = resp.get("id").unwrap();
    
    // Retrieve it
    let memory = db.get_memory(id.as_str().unwrap()).await?;
    assert!(memory.is_some());
    
    Ok(())
}
```

## Architecture

### Trait Implementation Pattern

`voidm-sqlite` implements 50+ methods from the generic `Database` trait:

- **Memory CRUD** (5 methods): add, get, list, update, delete
- **Memory Edges** (3 methods): link, unlink, list
- **Ontology Concepts** (6 methods): add, get, list, delete, search, resolve
- **Ontology Edges** (3 methods): create, list, delete
- **Search** (5 methods): BM25, fuzzy, hybrid, raw, etc. (stubs for now)
- **Graph** (2 methods): Cypher query, neighbor traversal (stubs)
- **Schema & Health** (2 methods): ensure_schema, health_check

### JSON Conversion Pattern

All incoming trait calls convert between:
1. **JSON** (trait boundary) → 2. **Rust types** (internal) → 3. **SQL** (execution) → 4. **JSON** (response)

Example - `add_memory()`:
```
JSON input: {"content": "...", "type": "...", "tags": [...]}
    ↓
Extract fields: content, memory_type, tags, importance
    ↓
Generate SQL: INSERT INTO memories (...)
    ↓
Execute & collect row
    ↓
Format response: {"id": "...", "conflicts": [], ...}
```

## Implementation Status

### ✅ Complete
- Memory CRUD (all 5 methods)
- Memory edges (all 3 methods)
- Ontology concepts (all 6 methods)
- Ontology edges (all 3 methods)
- Schema creation
- Health checks
- ID resolution
- Scope listing

### 🔄 Stub (Returns empty/default)
- Search operations (BM25, fuzzy, hybrid)
- Graph queries (Cypher, neighbors)
- Embeddings integration

### 🚀 Future Enhancements
1. Implement search operations using voidm-core search functions
2. Vector storage and ANN search via sqlite-vec
3. Full-text search via FTS5
4. Graph traversal for memory relationships

## Testing

Run tests with:
```bash
cargo test -p voidm-sqlite
```

Tests cover:
- Health check verification
- Schema creation
- Add/get memory roundtrip
- List memories with pagination
- JSON serialization/deserialization

## Dependencies

- `sqlx`: Async SQL toolkit
- `tokio`: Async runtime
- `serde_json`: JSON serialization
- `uuid`: ID generation
- `chrono`: Timestamps
- `libsqlite3-sys`: SQLite bindings
- `sqlite-vec`: Vector storage
- `voidm-db-trait`: Generic trait definition
- `voidm-core`: Core types and translations (optional, for query support)

## Notes

- Single writer, unlimited readers (SQLite constraint)
- WAL mode enabled for concurrency
- Foreign keys enforced
- sqlite-vec module auto-loaded for vector operations
