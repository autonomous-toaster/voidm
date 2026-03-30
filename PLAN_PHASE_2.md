# voidm v5 Phase 2+ Plan - UPDATED

**Status**: Phase 1 COMPLETE ✅
- Core: 0 sqlx violations
- Backend: 136 sqlx violations (correct)
- Architecture: Pure, backend-agnostic, production-ready

---

## COMPLETED PHASES

| Phase | Focus | Status | Duration |
|-------|-------|--------|----------|
| **-1** | Config override system | ✅ DONE | 1.5h |
| **0** | Generic node/edge format | ✅ DONE | 3-4h |
| **1** | Backend abstraction + sqlx isolation | ✅ DONE | ~20h |

---

## PHASE 2: Dead Code Removal & Cleanup

**Duration**: 1-2 days
**Priority**: HIGH
**Goal**: Remove unused code, simplify codebase

### Phase 2.1: Dead Code Audit
- Identify unused functions in voidm-core
- Identify unused functions in voidm-graph
- Identify unused modules
- Document removal impact

### Phase 2.2: Remove Unused Functions
**Candidates**:
- `find_similar()` - unused search function
- `build_suggested_links()` - unused suggestion function
- `extract_and_link_concepts()` - NER dead code
- `check_model_mismatch()` - moved to trait, old version unused
- `list_edges()` - moved to trait, old version unused

### Phase 2.3: Simplify Optional Features
- Mark voidm-tagging as optional (feature flag)
- Mark voidm-ner as optional (feature flag)
- Move optional code behind feature gates

### Phase 2.4: Test & Verify
- Build with all features enabled
- Build with no features
- Verify all CLI commands still work

---

## PHASE 3: User-Provided Type/Scope

**Duration**: 1.5 days
**Priority**: HIGH
**Goal**: Allow users to define custom memory types and scopes

### Phase 3.1: Custom Memory Types
- Add config section for custom types
- Validate against predefined + custom types
- Update CLI to accept custom types
- Update add_memory to handle custom types

### Phase 3.2: Dynamic Scopes
- Allow arbitrary scope strings (not predefined)
- Update scope validation
- Update search to filter by scope
- Update list to show scopes

### Phase 3.3: Backward Compatibility
- Ensure existing memories work
- Ensure existing scopes work
- Ensure existing types work

---

## PHASE 4: Config Flexibility & Routing

**Duration**: 1-2 days
**Priority**: MEDIUM
**Goal**: Advanced config options for backend selection and routing

### Phase 4.1: Multiple Backend Instances
- Support multiple SQLite databases
- Support multiple Neo4j instances
- Config routing rules (which backend for which operation)

### Phase 4.2: Read/Write Splitting
- Route writes to primary backend
- Route reads to replica backends
- Fallback logic

### Phase 4.3: Migration Support
- Migrate data between backends
- Verify data consistency
- Rollback capability

---

## PHASE 5: Chunk/Embedding Guarantee

**Duration**: 2 days
**Priority**: MEDIUM
**Goal**: Ensure every memory has chunks and embeddings

### Phase 5.1: Chunking System
- Implement chunking on add_memory
- Store chunks with sequence numbers
- Verify chunk ordering

### Phase 5.2: Embedding System
- Generate embeddings for chunks
- Store embeddings efficiently
- Support multiple embedding models

### Phase 5.3: Verification
- Audit existing memories
- Generate missing chunks
- Generate missing embeddings

---

## PHASE 6: Tag System & Refresh

**Duration**: 2 days
**Priority**: MEDIUM
**Goal**: Robust tag system with refresh capabilities

### Phase 6.1: Tag Management
- Create/delete tags
- Assign tags to memories
- Query by tags
- Tag statistics

### Phase 6.2: Refresh System
- Refresh memory embeddings
- Refresh memory quality scores
- Refresh memory scopes
- Batch refresh operations

### Phase 6.3: Performance
- Index tags for fast lookup
- Optimize refresh queries
- Batch operations

---

## PHASE 7: Search Optimization

**Duration**: 1-2 days
**Priority**: LOW
**Goal**: Improve search performance and relevance

### Phase 7.1: Hybrid Search
- Vector search (ANN)
- BM25 full-text search
- Fuzzy matching
- Combine results with RRF

### Phase 7.2: Search Ranking
- Relevance scoring
- Recency boosting
- Importance boosting
- Quality filtering

### Phase 7.3: Performance
- Query optimization
- Index tuning
- Caching strategy

---

## PHASE 8: Final Cleanup & Documentation

**Duration**: 1-2 days
**Priority**: LOW
**Goal**: Polish, documentation, examples

### Phase 8.1: Code Cleanup
- Remove commented code
- Fix warnings
- Optimize imports
- Consistent formatting

### Phase 8.2: Documentation
- API documentation
- Architecture guide
- Backend extension guide
- Configuration reference

### Phase 8.3: Examples
- Example backends
- Example CLI usage
- Example MCP integration

---

## RECOMMENDED NEXT STEPS

### Immediate (Next Session)
1. **Phase 2.1-2.2**: Dead code removal (1-2 hours)
   - Audit and remove unused functions
   - Verify no regressions
   - Clean build

2. **Phase 2.3**: Feature flags (1 hour)
   - Mark optional features
   - Test with/without features

### Short Term (1-2 days)
3. **Phase 3**: User-provided types/scopes (1.5 days)
   - Extend config system
   - Update validation
   - Test custom types

4. **Phase 4**: Config routing (1-2 days)
   - Multiple backends
   - Read/write splitting
   - Migration support

### Medium Term (1 week)
5. **Phase 5-6**: Chunks, embeddings, tags (4 days)
   - Complete chunking system
   - Embedding management
   - Tag system

6. **Phase 7-8**: Search & polish (2-3 days)
   - Search optimization
   - Documentation
   - Examples

---

## ARCHITECTURE READY FOR PHASE 2

✅ Pure core (0 sqlx)
✅ Backend-agnostic (works with any backend)
✅ Trait-based abstraction (Database, GraphQueryOps)
✅ MCP server (works with any backend)
✅ Graph operations (works with any backend)
✅ All CLI commands working
✅ Zero regressions
✅ Production-ready

**Ready to build features without architectural debt.**

---

## ESTIMATED TIMELINE

| Phase | Duration | Cumulative |
|-------|----------|-----------|
| Phase 2 | 1-2 days | 1-2 days |
| Phase 3 | 1.5 days | 2.5-3.5 days |
| Phase 4 | 1-2 days | 3.5-5.5 days |
| Phase 5-6 | 4 days | 7.5-9.5 days |
| Phase 7-8 | 2-3 days | 9.5-12.5 days |
| **TOTAL** | **~2 weeks** | - |

---

## SUCCESS CRITERIA

✅ Phase 1: Pure architecture (ACHIEVED)
✅ Phase 2: Clean codebase
✅ Phase 3: Flexible configuration
✅ Phase 4: Advanced routing
✅ Phase 5-6: Complete feature set
✅ Phase 7-8: Optimized & documented

---

**Status**: Ready for Phase 2. Architecture foundation is solid and proven.
