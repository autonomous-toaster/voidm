# voidm-postgres

PostgreSQL backend implementation for voidm using the Database trait abstraction.

## Features

- Full-text search using PostgreSQL `tsvector` + `ts_rank_cd()` 
- Fuzzy matching with Jaro-Winkler similarity scoring
- JSONB support for tags and scopes
- UUID-based memory and concept IDs with prefix resolution
- Memory edge relationships with cascade delete
- Ontology concept hierarchies with instance tracking
- Scope and type filtering on queries

## Database Schema

The PostgreSQL backend creates the following tables:

- `memories` - Core memory storage with FTS index
- `memory_edges` - Links between memories (many-to-many relations)
- `concepts` - Ontology concept definitions
- `ontology_edges` - Relationships between concepts

## Usage

```rust
use voidm_postgres::{PostgresDatabase, open_postgres_pool};
use voidm_db_trait::Database;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pool = open_postgres_pool("postgres://user:pass@localhost/voidm").await?;
    let db = PostgresDatabase { pool };
    
    // Initialize schema
    db.ensure_schema().await?;
    
    // Add a memory
    let result = db.add_memory(
        serde_json::json!({
            "content": "PostgreSQL is a relational database",
            "memory_type": "semantic",
            "tags": ["database", "sql"],
            "scopes": ["dev"],
            "importance": 7
        }),
        &serde_json::json!({})
    ).await?;
    
    let id = result.get("id").unwrap();
    
    // Search
    let results = db.search_bm25("PostgreSQL", None, None, 10).await?;
    
    Ok(())
}
```

## Testing

Run integration tests against a PostgreSQL instance:

```bash
# Start PostgreSQL on localhost
docker run -d -e POSTGRES_PASSWORD=password -p 5432:5432 postgres:latest

# Run tests with database URL
POSTGRES_URL=postgres://postgres:password@localhost/voidm_test cargo test -p voidm-postgres --test backend_compat -- --ignored --test-threads=1
```

## Implementation Notes

### Search Methods

- **search_bm25**: Uses PostgreSQL full-text search with `ts_rank_cd()` for BM25-like scoring
- **search_fuzzy**: Client-side fuzzy matching using Jaro-Winkler (consistent with SQLite backend)
- **search_hybrid**: Falls back to BM25 when embeddings unavailable

### ID Resolution

Memory and concept IDs support both:
- Exact UUID match: `"550e8400-e29b-41d4-a716-446655440000"`
- Prefix match: `"550e8400"` → finds first memory starting with that prefix

### Type Handling

Uses `serde_json::Value` for all complex types per the Database trait design:
- Tags/scopes stored as JSONB in PostgreSQL
- Converted to/from JSON at API boundaries
- No coupling to voidm-core type definitions

## Compatibility

Implements all 43 methods of the Database trait:

- **Lifecycle** (3): health_check, close, ensure_schema
- **Memory CRUD** (7): add_memory, get_memory, list_memories, delete_memory, update_memory, resolve_memory_id, list_scopes  
- **Memory Edges** (5): link_memories, unlink_memories, list_edges, list_ontology_edges, create_ontology_edge
- **Search** (5): search_hybrid, search_bm25, search_fuzzy, fetch_memories_raw, search_concepts
- **Concepts** (7): add_concept, get_concept, get_concept_with_instances, list_concepts, delete_concept, resolve_concept_id, search_concepts
- **Ontology Edges** (3): add_ontology_edge, delete_ontology_edge, query_cypher (not supported)
- **Graph** (2): get_neighbors, check_model_mismatch
- **Utility** (1): check_model_mismatch

## Performance Characteristics

Compared to SQLite backend:

- **Full-text search**: Faster for large datasets (PostgreSQL tsvector optimization)
- **Fuzzy search**: Similar performance (both use client-side scoring)
- **Memory overhead**: Lower per-query (PostgreSQL native FTS)
- **Concurrency**: Better support for multiple writers

## Future Improvements

- [ ] Prepared statements for repeated queries
- [ ] Query result caching
- [ ] Batch operations for bulk inserts
- [ ] Connection pool optimization tuning
- [ ] Support for PostGIS extensions (spatial queries)
- [ ] Full-text search language configuration
