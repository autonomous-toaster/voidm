# Phase 2: Dead Code Removal & Feature Foundation

## Status Quo

**Phase 1 Status**: 58% complete (10 hours invested)
- ✅ Critical blocker fixed
- ✅ Architecture clean
- ✅ 342 lines extracted to backend
- ✅ Integration tests passing

**Remaining Phase 1**: 
- Phase 1.8: voidm-cli refactoring (1-2h, optional)
- Phase 1.9: voidm-graph refactoring (2-3h, optional)

**Decision**: MOVE TO PHASE 2 (foundation is solid)

---

## Phase 2 Overview

Phase 2 focuses on:
1. Dead code removal (ontology already done in Session 6)
2. Feature foundation work
3. Schema cleanup

---

## Phase 2.0: Feature Flags & Optional Features

### Current Optional Features
- ner: NER (Named Entity Recognition) - currently unused
- nli: NLI (Natural Language Inference) - optional
- reranker: Query reranking - optional
- query-expansion: Query expansion - optional

### Work
- Audit which features are actually used
- Disable broken features (mark as experimental)
- Clean up unused feature-gated code

**Estimated Time**: 1 hour

---

## Phase 2.1: Clean Schema & Core Cleanup

### Current Database Schema
- ✅ Generic nodes/edges (clean)
- ✅ Memories table (clean)
- ✅ Memory scopes (clean)
- ✅ FTS index (clean)
- ✓ Graph tables (legacy, works)

### Work
- Verify all tables are used
- Check for redundant indexes
- Document schema rationale
- Create schema visualization

**Estimated Time**: 1-2 hours

---

## Phase 2.2: Dependency Audit

### Current Dependencies
- ✅ voidm-db: Foundation (clean)
- ✅ voidm-core: Logic (90% pure)
- ✅ voidm-sqlite: Backend (98% pure)
- ⚠️ voidm-cli: Has direct pool usage
- ⚠️ voidm-graph: Has sqlx (optional refactor)

### Work
- Verify dependency graph is clean
- Check for unnecessary re-exports
- Update Cargo.toml documentation
- Audit feature flags

**Estimated Time**: 1 hour

---

## Phase 2.3: Documentation & Architecture

### Current State
- ✅ PLAN.md: Comprehensive
- ✅ SESSION_9_FINAL_SUMMARY.md: Clear
- ⚠️ ARCHITECTURE_STATUS.md: Outdated
- ⚠️ No per-crate documentation

### Work
- Update architecture docs
- Create per-crate README files
- Document design patterns used
- Add decision records

**Estimated Time**: 1-2 hours

---

## Phase 2.4: Testing & Integration

### Current Tests
- ✅ Unit tests in voidm-sqlite
- ✅ Integration tests (add/list/get)
- ⚠️ No end-to-end tests
- ⚠️ No performance benchmarks

### Work
- Add more integration tests
- Document test coverage
- Create test data fixtures
- Verify all CLI commands work

**Estimated Time**: 1-2 hours

---

## Phase 2.5: Performance Baseline

### Current State
- ✅ Migrations run fast (~100ms)
- ✅ Add memory works (~50ms)
- ⚠️ No performance metrics
- ⚠️ No query benchmarks

### Work
- Establish performance baseline
- Document common operations timing
- Identify slow paths
- Plan optimizations for Phase 3+

**Estimated Time**: 1 hour

---

## Phase 2 Total

**Estimated Time**: 6-8 hours (1-2 days at 3-4 hours/day)

**Outcome**: 
- Clean, documented codebase
- Solid foundation for features
- Clear performance baseline
- Ready for Phase 3

---

## Phase 3 Preview: Features

### Phase 3.0: User-Provided MemoryType/Scope Nodes
**Work**: 
- MemoryType nodes (Episodic, Semantic, etc.)
- Scope nodes (user-created)
- Links from Memory nodes

**Estimated**: 1-2 hours

### Phase 3.1: Tag System Refresh
**Work**:
- Redesign tag storage
- Create Tag nodes
- Link from memories

**Estimated**: 2-3 hours

### Phase 3.2: Embedding & Chunking Guarantee
**Work**:
- Ensure embeddings on add
- Automatic chunking on add
- Persistence verification

**Estimated**: 2-3 hours

### Phase 3.3: Search Improvements
**Work**:
- BM25 tuning
- Vector search enhancements
- Reranking integration

**Estimated**: 2-3 hours

---

## Recommendation for Next Session

### Option A: Complete Phase 2 Now (6-8 hours)
- Deep cleanup and documentation
- Solid foundation before features
- Better for long-term maintenance

### Option B: Start Phase 3 Early (focus on features)
- Skip Phase 2 documentation work
- Jump to MemoryType/Scope nodes (Phase 3.0)
- Features-first approach

### RECOMMENDATION: **Option A + Option B Hybrid**
1. Do Phase 2.0-2.2 quickly (2-3 hours) - essential cleanup
2. Skip Phase 2.3-2.5 docs (can do later)
3. Start Phase 3.0 (user features) immediately
4. Docs can catch up in Phase 3+

---

## Quick-Start Phase 2 (2-3 hours)

### Phase 2.0: Feature Flags (30 min)
- Audit which features work
- Mark experimental features
- Update Cargo.toml

### Phase 2.1: Schema Cleanup (1 hour)
- Verify all tables used
- Document schema
- Check indexes

### Phase 2.2: Dependency Audit (30 min)
- Verify clean flow
- Update docs
- Check re-exports

### Then: Start Phase 3.0 (User Features)

---

## Decision Point

**At Session 10 Start**:
1. Quick Phase 2 cleanup (2-3 hours) - RECOMMENDED
2. Then Phase 3.0: User-provided MemoryType/Scope nodes
3. Result: Features + clean code

---

## Phase 2 Success Criteria

✅ Feature flags documented
✅ Schema verified and documented
✅ Dependency graph clean
✅ No compilation warnings from dead code
✅ Build time reasonable (~15s)
✅ All integration tests passing
✅ Performance baseline established

