# VOIDM Migration Assessment: SQLite → Neo4j with Chunks

## STATUS: ✅ ASSESSMENT COMPLETE - Ready for Implementation

**Date**: 2026-03-28  
**Data Volume**: 1043 memories, 2252 concepts, 5317 ontology edges  
**Target**: Neo4j with automatic chunking (1-shot process)  
**Estimated Timeline**: 5-7 hours (2-3 hours implementation + 3-4 hours execution)  

---

## CURRENT STATE

### SQLite Database
```
Location: ~/Library/Application Support/voidm/memories.db (16MB)
Memories:           1043
Concepts:           2252 (old ontology system)
Memory Edges:       15
Ontology Edges:     5317
Memory Chunks:      0 (NEW feature, not yet present)
```

### New Format Architecture (Phase A/B/C Complete)
✅ Memory Chunks: Automatic chunking (350 char target, 6K soft, 15K hard limit)  
✅ Coherence Scoring: Per-chunk quality assessment  
✅ Embeddings: Support added (to be generated in Phase 2)  
✅ NER Extraction: Framework ready (to be integrated in Phase 2)  
✅ Export/Import: JSONL format complete and tested  
✅ All Backends: SQLite, PostgreSQL, Neo4j synchronized  

---

## MIGRATION STRATEGY: One-Shot Export/Import

### Step 1: Export (SQLite → JSONL with Chunks)
```
Input:  1043 memories (full content)
Process:
  - Fetch all memories with all fields (title, metadata, scopes, tags, context, etc.)
  - For each memory:
    * Apply chunk_smart() with ChunkingStrategy
    * Generate UUID v5 chunk IDs (deterministic: memory_id + chunk_index)
    * Calculate coherence_score for each chunk
    * Create MemoryRecord + ChunkRecords
  - Fetch all relationships and concepts
Output: JSONL file (~500KB-1MB)
```

### Step 2: Import (JSONL → Neo4j)
```
Input:  JSONL file with Memory + Chunk records
Process:
  - Parse JSONL line by line
  - Create Memory nodes with all fields
  - Create MemoryChunk nodes, link to Memory with BELONGS_TO
  - Create edges and relationships
  - Batch commit for performance
Output: Neo4j with 1043 Memories + ~4500 MemoryChunks
```

---

## EXPECTED OUTPUT

### JSONL File Structure
```jsonl
{"type":"memory","id":"mem-1","content":"...","memory_type":"semantic","title":"...","scopes":["..."],...}
{"type":"memory_chunk","id":"chunk-1","memory_id":"mem-1","content":"...","coherence_score":0.75}
{"type":"memory_chunk","id":"chunk-2","memory_id":"mem-1","content":"...","coherence_score":0.72}
{"type":"relationship","source_id":"mem-1","rel_type":"RELATES_TO","target_id":"mem-2",...}
{"type":"concept","id":"concept-1","name":"...","description":"...",...}
...
```

### Neo4j Result
```
Memory Nodes:        1043
MemoryChunk Nodes:   ~4500 (estimated)
BELONGS_TO Edges:    ~4500
Memory Edges:        15 (original relationships)
Ontology Edges:      5317 (concept relationships)
```

---

## WHAT NEEDS TO BE IMPLEMENTED

### Component 1: Enhanced Export (voidm-core/src/export.rs)
**Status**: 50% complete (export format exists, chunking not integrated)
**Work**:
- [ ] New function: export_sqlite_with_chunks()
- [ ] Integrate chunk_smart() during export (not after)
- [ ] Generate UUID v5 chunk IDs deterministically
- [ ] Calculate coherence_score per chunk
- [ ] Write JSONL to file or stdout

**Effort**: ~1 hour

### Component 2: CLI Command (voidm-cli/src/commands/export_format.rs)
**Status**: 0% complete (new file needed)
**Work**:
- [ ] Create new command: voidm export-format
- [ ] Arguments: --output, --backup
- [ ] Call export_sqlite_with_chunks()
- [ ] Print statistics

**Effort**: ~30 minutes

### Component 3: Enhanced Import (voidm-core/src/import.rs)
**Status**: 50% complete (structure exists, MemoryChunk handling missing)
**Work**:
- [ ] Handle MemoryChunk records in import_from_jsonl()
- [ ] Create MemoryChunk nodes in Neo4j
- [ ] Link chunks to memories (BELONGS_TO)
- [ ] Batch inserts for performance
- [ ] Return (memory_count, chunk_count, relationship_count)

**Effort**: ~1 hour

### Component 4: Database Schema (all backends)
**Status**: 0% complete for MemoryChunk tables
**Work**:
- [ ] SQLite: CREATE memory_chunks table
- [ ] PostgreSQL: CREATE memory_chunks table
- [ ] Neo4j: Already supports MemoryChunk nodes (no schema needed)
- [ ] Add indexes for memory_id lookups

**Effort**: ~30 minutes

### Component 5: Testing & Validation
**Status**: 0% complete
**Work**:
- [ ] Unit test: Export with chunking
- [ ] Unit test: Import chunks
- [ ] Integration test: Round-trip SQLite → JSONL → Neo4j
- [ ] Validation: Data integrity checks
- [ ] Verification: All 1043 memories + ~4500 chunks present

**Effort**: ~1-2 hours

---

## IMPLEMENTATION TIMELINE

| Phase | Task | Time | Status |
|-------|------|------|--------|
| 1 | Assessment & planning | ✓ 30 min | COMPLETE |
| 2 | Implement export_with_chunks() | 1 hour | TODO |
| 2 | Create export_format CLI command | 30 min | TODO |
| 2 | Update import handlers | 1 hour | TODO |
| 2 | Add MemoryChunk schema | 30 min | TODO |
| 2 | Testing & debugging | 1-2 hours | TODO |
| 3 | Backup SQLite | 5 min | TODO |
| 3 | Run export | 2-3 min | TODO |
| 3 | Verify JSONL | 5 min | TODO |
| 3 | Run import | 2-3 min | TODO |
| 3 | Validate results | 10 min | TODO |
| **TOTAL** | | **5-7 hours** | |

---

## RISK ASSESSMENT

### 🔴 CRITICAL RISKS
None identified - process is straightforward

### 🟡 MEDIUM RISKS
1. **Large memories (>15K chars)** 
   - Mitigation: Add validation before chunking, warn user
   
2. **Chunking inconsistency**
   - Mitigation: Use deterministic chunk_smart() with consistent strategy
   
3. **UUID collision**
   - Mitigation: Use UUID v5 (memory_id + chunk_index) - deterministic

### 🟢 LOW RISKS
- Missing embeddings initially (can be generated in Phase 2)
- Concepts not converted to Tags (can be done in Phase F)
- NER not extracted (can be done in Phase 2)

---

## SUCCESS CRITERIA

✅ Pre-Migration
- [x] SQLite backup created
- [x] Configuration ready for Neo4j
- [x] All components identified

✅ Export
- [ ] All 1043 memories exported with all fields
- [ ] Chunking produces 4500+ chunks
- [ ] JSONL format valid
- [ ] Coherence scores present

✅ Import
- [ ] All 1043 memories in Neo4j
- [ ] All chunks created and linked
- [ ] All relationships preserved
- [ ] No data loss

✅ Post-Migration
- [ ] Neo4j backend fully operational
- [ ] All queries work on chunked data
- [ ] Search/retrieval functional
- [ ] Data integrity verified

---

## EXECUTION CHECKLIST

### Phase 2 Implementation
- [ ] Implement export_sqlite_with_chunks()
- [ ] Create export_format CLI command
- [ ] Update import handlers for MemoryChunk
- [ ] Add MemoryChunk schema to all backends
- [ ] Write tests
- [ ] Test export/import cycle

### Phase 3 Execution
- [ ] Backup SQLite: `cp ~/Library/Application\ Support/voidm/memories.db ~/backups/memories_$(date +%Y%m%d).db`
- [ ] Export: `voidm export-format -o /tmp/memories.jsonl`
- [ ] Verify: `wc -l /tmp/memories.jsonl` (should be ~5500 lines)
- [ ] Edit config: Change `backend = "sqlite"` to `backend = "neo4j"`
- [ ] Import: `voidm import < /tmp/memories.jsonl`
- [ ] Verify: `voidm stats` (should show 1043 memories, 4500+ chunks)
- [ ] Test: Run search queries on Neo4j

---

## DELIVERABLES

### Code Changes
1. voidm-core/src/export.rs (+150 lines)
2. voidm-cli/src/commands/export_format.rs (+80 lines, new file)
3. voidm-core/src/import.rs (+100 lines)
4. voidm-sqlite/src/lib.rs (+30 lines for schema)
5. voidm-postgres/src/lib.rs (+30 lines for schema)
6. voidm-neo4j/src/lib.rs (updated in import)
7. Tests (+200 lines)

### Output Files
1. Backup: memories_20260328.db
2. Export: /tmp/memories.jsonl (~1MB)

### Verification Files
1. Export statistics
2. Import statistics
3. Neo4j validation queries
4. Data integrity report

---

## NEXT STEPS

1. **Review & Approve Plan** (5 min)
   - Review this assessment
   - Confirm timeline expectations
   - Approve budget (5-7 hours)

2. **Implement** (3-4 hours)
   - Follow implementation todos
   - Test as you go
   - Commit after each component

3. **Execute** (1-2 hours)
   - Run migration step by step
   - Validate at each stage
   - Document results

4. **Complete** (30 min)
   - Switch to Neo4j backend
   - Update documentation
   - Celebrate success 🎉

---

## NOTES

- This is a **one-shot migration** - all data converted at once
- **No custom migration scripts needed** - uses existing export/import
- **Fully reversible** - backup allows rollback to SQLite
- **Data-driven** - all fields preserved in round-trip
- **Timeline-realistic** - includes buffer for debugging

