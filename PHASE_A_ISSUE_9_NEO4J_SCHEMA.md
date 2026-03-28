## VOIDM Phase A Issue #9: Neo4j Schema Definition

**Status**: DESIGN PHASE  
**Date**: 2026-03-28  
**Estimated Work**: 2 hours  
**Blocker**: Part D (chunking 900 memories)  

---

## Design Overview

### Node Types

#### Memory (Existing)
```
:Memory {
  id: UUID (PRIMARY KEY)
  content: String (full memory text, searchable)
  type: String (episodic|semantic|procedural|conceptual|contextual)
  scope: String (e.g., "voidm/phase-a", searchable text)
  created_at: Timestamp
  updated_at: Timestamp
  author: String
  author_trust: String (user|assistant|unknown)
  coherence_score: Float (0.0-1.0)
  quality_level: String (EXCELLENT|GOOD|FAIR|POOR)
  soft_limit_exceeded: Boolean (>6K chars)
  hard_limit_exceeded: Boolean (>15K chars)
}
```

#### MemoryChunk (NEW)
```
:MemoryChunk {
  id: String (UNIQUE, format: mchk_<32-hex> from UUID v5)
  memory_id: UUID (foreign key to Memory, indexed for filtering)
  index: Integer (0-based chunk index within memory)
  content: String (chunk text, <1000 chars)
  size: Integer (char count of content)
  break_type: String (paragraph|sentence|word|character)
  
  # Coherence scores (5 components + final)
  completeness: Float (0.0-1.0)
  coherence: Float (0.0-1.0)
  relevance: Float (0.0-1.0)
  specificity: Float (0.0-1.0)
  metadata: Float (0.0-1.0)
  coherence_score: Float (0.0-1.0, weighted final score)
  quality_level: String (EXCELLENT|GOOD|FAIR|POOR)
  
  # Embeddings (384-dim vector from fastembed)
  embedding: List<Float> (384 values, stored as native Neo4j array)
  
  # Metadata
  created_at: Timestamp
  updated_at: Timestamp
  is_code_like: Boolean (detected code block)
}
```

#### Tag (Existing)
```
:Tag {
  id: UUID
  name: String (UNIQUE, lowercase)
  count: Integer (number of memories tagged)
  created_at: Timestamp
}
```

### Relationship Types

#### CONTAINS (NEW - Memory → MemoryChunk)
```
(Memory) --[CONTAINS {index: 0}]--> (MemoryChunk)
(Memory) --[CONTAINS {index: 1}]--> (MemoryChunk)
(Memory) --[CONTAINS {index: 2}]--> (MemoryChunk)
...
```

**Properties**:
- `index`: Position in sequence (for ordering)

**Rationale**: 
- One Memory can contain multiple MemoryChunks
- Index preserves order for reconstruction
- Enables efficient chunk retrieval: `(m:Memory)-[CONTAINS]-(c:MemoryChunk)`

#### TAGGED_WITH (Existing - Memory → Tag)
```
(Memory) --[TAGGED_WITH]--> (Tag)
```

#### Other Relationships (Preserved)
- RELATES_TO, CONTRADICTS, etc. (between Memories)
- CONCEPT relationships (existing ontology)

---

## Vector Storage Strategy

### Embedding Dimensions
- **Model**: fastembed (384-dim)
- **Storage**: Native Neo4j array property
- **Size**: ~1.5KB per chunk (384 floats × 4 bytes)
- **Total (5000 chunks)**: ~7.6MB

### Vector Index

#### Option A: Native Neo4j Vector Index (Recommended)
```cypher
CREATE VECTOR INDEX chunk_embedding_vector
FOR (c:MemoryChunk) ON c.embedding
OPTIONS {indexConfig: {
  'vector.similarity_metric': 'cosine'
}}
```

**Pros**:
- Native Neo4j vector search (<50ms for 5K chunks)
- No external dependency
- ACID consistency
- Simple integration

**Cons**:
- Requires Neo4j 5.15+
- Limited to ~50K vectors on standard license
- No approximate nearest neighbor (brute force)

#### Option B: Composite Index + Post-Filter (MVP)
```cypher
CREATE INDEX chunk_memory_idx
FOR (c:MemoryChunk) ON c.memory_id

CREATE INDEX chunk_created_idx
FOR (c:MemoryChunk) ON c.created_at
```

**Pros**:
- Works on all Neo4j versions
- Fast memory-based filtering
- Can implement KNN in application layer

**Cons**:
- Requires app-level vector similarity
- Slower semantic search (100-200ms)

**Plan**: Start with Option B (works everywhere), upgrade to Option A if performance needed

---

## Query Patterns for Part D

### 1. Create MemoryChunk
```cypher
MERGE (c:MemoryChunk {id: $chunk_id})
  ON CREATE SET
    c.memory_id = $memory_id,
    c.index = $chunk_index,
    c.content = $chunk_content,
    c.size = $chunk_size,
    c.break_type = $break_type,
    c.completeness = $completeness,
    c.coherence = $coherence,
    c.relevance = $relevance,
    c.specificity = $specificity,
    c.metadata = $metadata,
    c.coherence_score = $coherence_score,
    c.quality_level = $quality_level,
    c.embedding = $embedding,
    c.is_code_like = $is_code_like,
    c.created_at = timestamp()
```

### 2. Link Memory to MemoryChunk
```cypher
MATCH (m:Memory {id: $memory_id})
MATCH (c:MemoryChunk {id: $chunk_id})
MERGE (m)-[r:CONTAINS {index: $chunk_index}]->(c)
```

### 3. Get All Chunks for a Memory
```cypher
MATCH (m:Memory {id: $memory_id})-[rel:CONTAINS]->(c:MemoryChunk)
RETURN c
ORDER BY rel.index ASC
```

### 4. Semantic Search (Vector Similarity)
```cypher
# First: embed query vector in application
# Then: brute force similarity computation in app OR use vector index if available

MATCH (c:MemoryChunk)
WHERE c.memory_id IN $memory_ids  # Optional filter
RETURN c, 
  reduce(sim=0, i in range(0, 383) | 
    sim + c.embedding[i] * $query_embedding[i]) as cosine_similarity
ORDER BY cosine_similarity DESC
LIMIT $top_k
```

### 5. Quality Metrics
```cypher
# Get stats on chunk quality
MATCH (c:MemoryChunk)
RETURN 
  c.quality_level,
  COUNT(*) as chunk_count,
  AVG(c.coherence_score) as avg_coherence,
  MIN(c.coherence_score) as min_coherence,
  MAX(c.coherence_score) as max_coherence
GROUP BY c.quality_level
```

---

## Implementation Checklist for Part D

### Phase D.1: Schema Setup (1 hour)
- [ ] Create MemoryChunk node constraint: UNIQUE(id)
- [ ] Create index: MemoryChunk.memory_id (for fast filtering)
- [ ] Create index: MemoryChunk.created_at (for time-based queries)
- [ ] Create index: Memory.id (if not exists)
- [ ] Document schema in comments
- [ ] Test: Can query empty MemoryChunk table

### Phase D.2: CLI Command - chunk (1 hour before chunking starts)
- [ ] Add `voidm chunk` command in CLI
- [ ] Load memories from SQLite (first 10, then 900)
- [ ] Run chunking algorithm for each memory
- [ ] Compute coherence scores
- [ ] Generate embeddings (stub for now, real in Part E)
- [ ] Create MemoryChunk nodes
- [ ] Create CONTAINS relationships
- [ ] Log progress: [CHUNKING] output format
- [ ] Handle errors gracefully

### Phase D.3: Execute Full Chunking (2-3 days)
- [ ] Test on 10 sample memories (should chunk into ~80 chunks)
- [ ] Verify relationships created
- [ ] Query chunks back and verify they match
- [ ] Batch process all 900 memories
- [ ] Monitor disk usage
- [ ] Target: 4500+ chunks created

### Phase D.4: Validation (1 hour after chunking)
- [ ] All 900 memories have CONTAINS relationships
- [ ] Total chunk count ≈ 4500
- [ ] No orphan chunks (all linked to memory)
- [ ] Quality metrics generated
- [ ] Average coherence ≈ 0.72
- [ ] Time to chunk all memories < 6 hours

---

## Data Model: Before vs After

### Before Part D
```
:Memory {id, content, type, scope, ...}
  ├─ relationships to Tags
  ├─ relationships to other Memories
  └─ NO embeddings yet (will be in Part E)
```

### After Part D
```
:Memory {id, content, type, scope, ...}
  ├─ [CONTAINS]→ :MemoryChunk {id, index, content, coherence_score, embedding: null}
  │  ├─ [CONTAINS]→ :MemoryChunk {id, index, ...}
  │  └─ [CONTAINS]→ :MemoryChunk {id, index, ...}
  ├─ [TAGGED_WITH]→ :Tag
  └─ [RELATES_TO]→ :Memory
```

### After Part E (Embeddings)
```
:MemoryChunk {
  ...,
  embedding: [0.234, -0.567, ...384 dims],
  coherence_score: 0.72
}
```

---

## Performance Expectations

### Chunking Speed (Part D)
- Chunk creation: 5-10 per second
- Total time for 900 memories: 10-30 minutes
- Bottleneck: Network latency to Neo4j, not algorithm

### Query Performance (Post-Part E)
- Semantic search (5K chunks): 50-150ms
- Memory-based filtering: <50ms
- Quality summary: <100ms

### Storage Impact
- MemoryChunk nodes: +4500 nodes
- CONTAINS relationships: +4500 relationships
- Total graph size: ~100MB (vs ~50MB before)

---

## Error Handling Strategy

### During Chunking (Part D)
```rust
for memory in memories {
  match chunk_smart(&memory) {
    Ok(chunks) => {
      for chunk in chunks {
        match create_chunk_node(&neo4j, chunk) {
          Ok(_) => log!("[CHUNKING] Created chunk"),
          Err(e) => {
            warn!("[CHUNKING] Failed to create chunk: {}", e);
            failed_chunks.push(chunk);
            continue;  // Don't fail entire memory
          }
        }
      }
    }
    Err(e) => {
      warn!("[CHUNKING] Failed to chunk memory {}: {}", memory.id, e);
      failed_memories.push(memory.id);
      continue;  // Don't fail entire batch
    }
  }
}
```

### Recovery
- Restart from last successful memory
- Skip failed memories and log for manual review
- Generate failure report at end

---

## Neo4j Connection Configuration

### For Part D
```toml
[database]
neo4j_url = "bolt://localhost:7687"
neo4j_username = "neo4j"
neo4j_password = "<password>"
neo4j_database = "neo4j"  # or custom database name

[chunking]
batch_size = 100  # Create 100 chunks before commit
timeout_secs = 30
```

### Docker Compose (Testing)
```yaml
version: '3.8'
services:
  neo4j:
    image: neo4j:5.15
    environment:
      NEO4J_ACCEPT_LICENSE_AGREEMENT: "yes"
      NEO4J_AUTH: neo4j/neo4jpassword
    ports:
      - "7687:7687"
      - "7474:7474"
    volumes:
      - neo4j_data:/var/lib/neo4j/data

volumes:
  neo4j_data:
```

---

## Known Limitations & Future Work

### Part D (Current)
- No embeddings yet (null in MemoryChunk.embedding)
- No vector index yet (will add in Part E)
- Batch processing only (no streaming)

### Part E (Next)
- Add embedding generation (fastembed)
- Create vector index (if Neo4j 5.15+)
- Implement semantic search

### Future (Phase B+)
- Scope nodes (:Scope) for hierarchical organization
- Entity extraction (NER) for :Entity nodes
- Relationship inference between chunks
- Chunk similarity detection (within memory)

---

## Testing Strategy

### Unit Tests (Before Part D)
- [ ] Schema creation (CREATE_CONSTRAINT, CREATE_INDEX)
- [ ] Chunk node creation (MERGE with all properties)
- [ ] Relationship creation (CONTAINS)
- [ ] Query patterns (retrieve chunks by memory_id)

### Integration Tests (During Part D)
- [ ] Chunk 10 sample memories
- [ ] Verify counts: ~80 chunks total
- [ ] Verify relationships created
- [ ] Query back all chunks for each memory
- [ ] Verify chunk content matches original

### Load Tests (After Part D)
- [ ] Time to chunk 900 memories
- [ ] Disk usage growth
- [ ] Query performance on full dataset
- [ ] No missing chunks (COUNT check)

---

## Decision Points

### 1. Vector Index Strategy
**Recommendation**: Option B (composite indexes) for MVP
- Works on all Neo4j versions
- Simpler to implement
- Can upgrade to vector index later

### 2. Embedding Storage
**Recommendation**: Native Neo4j array
- No external vector DB
- Single source of truth
- ACID consistency

### 3. Chunk ID Generation
**Decision**: UUID v5 (deterministic) ✅ ALREADY IMPLEMENTED
- Format: "mchk_<32-hex>"
- Globally unique
- No storage required

### 4. Coherence Score Storage
**Decision**: Store all 5 components + final score ✅
- Enables future analysis
- Quality metrics per component
- Debugging heuristic effectiveness

---

## Summary

This schema supports Phase A Part D (chunking 900 memories into 4500+ chunks) and Part E (embedding generation and storage). The design is:

- **Simple**: Straightforward node/relationship model
- **Extensible**: Easy to add vector index later
- **Queryable**: Efficient patterns for common operations
- **Debuggable**: All metadata stored for analysis

**Next Step**: Implement schema creation in Neo4j, then start Part D chunking
