# PLAN Review: Identify Overengineering

## Summary
PLAN.md is 1043 lines with scope creep. Identifies what's NECESSARY vs NICE-TO-HAVE.

---

## Overengineered Elements to Remove or Defer

### 1. **Phase 0: Backend-Specific Embeddings Storage**
**Current**: Detailed plans for SQLite vector, Neo4j APOC, pgvector
**Issue**: Premature optimization. Embedding storage is internal detail.
**Action**: Simplify. Defer backend-specific optimization to Phase 1 when needed.
**Keep**: Basic embedding generation + storage trait method

### 2. **Phase 0: Title Embeddings (NEW)**
**Current**: 384-dim title embeddings, regenerate on update, title-drift detection use case
**Issue**: Scope addition not requested. Adds complexity during critical Phase 0.
**Action**: **DEFER** to Phase 6 (after Phase 0 stable). Not blocking.
**Keep**: Phase 0 focused on generic node/edge format only

### 3. **Phase 0: Edge Weights (NEW)**
**Current**: weight: f32 on all edges, multiple use cases documented
**Issue**: Not necessary for generic format. Adds storage everywhere.
**Action**: **DEFER** to Phase 4. Store weights only when used (entity mention counts, tag confidence).
**Keep**: Phase 0 schema simple: nodes(id, type, properties), edges(from, type, to)

### 4. **Phase 3.5: Entity Mention Weighting**
**Current**: Chunk -[:MENTIONS { weight: reference_count }]-> Entity
**Issue**: Depends on deferred edge weights. Overcomplicates NER phase.
**Action**: **DEFER** to Phase 4. Collect mention counts, add weights later.
**Keep**: Phase 3.5 focused on entity extraction + linking

### 5. **Phase 4: Multi-Dimensional Search Methods**
**Current**: 5 search trait methods (title, embedding, tags, entities, scopes)
**Issue**: Not needed in Phase 4. Search trait is infrastructure.
**Action**: Implement only 2 methods in Phase 4:
  - `search_by_embedding(embedding, limit)`
  - `search_by_title_embedding(embedding, limit)`
Add others in Phase 8 when actually needed for multi-dimensional search CLI.

### 6. **Phase 6: Auto-Tagging Feature**
**Current**: tinyllama + keyword extraction + classification, tag refresh on update
**Issue**: Complex for stability phase. Only tag refresh necessary.
**Action**: **DEFER** auto-tagging to Phase 7. Implement only user-provided tags in Phase 6.
**Keep**: Basic tag structure + tag refresh logic (remove auto, keep user)

### 7. **Phase 6.2: Tag Edge Metadata (auto_generated flag)**
**Current**: Distinguish auto vs user tags via edge properties
**Issue**: Premature until Phase 7. Overcomplicates Phase 6.
**Action**: **DEFER** tag distinction to Phase 7 (when auto-tagging added).
**Keep**: Phase 6 focused on user-provided tags only

### 8. **Phase 6.4: NLI Integration (Future)**
**Current**: Placeholder for relation classification
**Issue**: Not defined. Vague future feature. Adds confusion.
**Action**: **REMOVE** from PLAN. Re-add only if scope is clarified.

### 9. **Phase 7: Multiple Backend Instances**
**Current**: [backend.default], [backend.archive], [backend.readonly]
**Issue**: Not mentioned in original requirements. Scope creep.
**Action**: **SIMPLIFY**. Keep single backend config. Multi-instance is future.
**Keep**: Phase 7 config v2 with just backend type + connection details

### 10. **Phase 8.1: Multi-Dimensional Search CLI**
**Current**: --title/--content/--tags/--entities/--scope with intersection-based ranking
**Issue**: Complex for Phase 8. Scope addition.
**Action**: **SIMPLIFY** to single search improvement. Full multi-dimensional is Phase 8 only if time permits.
**Keep**: Basic search + embeddings working

---

## What's Actually Necessary

### Phase -1: Config Override (DONE)
- ✓ --config flag
- ✓ VOIDM_CONFIG env var
- ✓ Local .voidm.dev.toml

### Phase 0: Generic Node/Edge Format (ESSENTIAL)
- nodes(id, type, properties JSON)
- edges(from_id, edge_type, to_id, properties JSON)
- Chunks with sequence_num, char_start, char_end (reconstruction)
- NO: Title embeddings, edge weights, backend-specific optimization

### Phase 1: Backend Abstraction (ESSENTIAL)
- Fix 174 sqlx violations
- Extend DB trait with basic methods only
- Implement in SQLite, Neo4j, Postgres
- Test backend switching

### Phase 2: Dead Code (STRAIGHTFORWARD)
- Identify + remove unused code

### Phase 3: User-Provided Type/Scope (SIMPLE)
- Remove auto-linking
- Manual user input only

### Phase 5: Reranker Integration (FEATURE)
- Hook up existing reranker
- If/when feature enabled

### Phase 6: Basic Tag System (SIMPLE)
- User-provided tags only
- Tag refresh on update (remove old, add new user tags)
- NO: Auto-tagging, tag metadata, distinction logic

### Phase 8: Search + Cleanup (STABILITY)
- Get existing search working
- Fix compiler warnings
- Clean code

---

## Deferred to Post-Stabilization

- Title embeddings (Phase 0 → Phase 6+)
- Edge weights everywhere (Phase 0 → Phase 4)
- Entity mention weighting (Phase 3.5 → Phase 4)
- Auto-tagging (Phase 6 → Phase 7)
- Tag metadata (Phase 6.2 → Phase 7)
- Multi-dimensional search (Phase 8.1 → Phase 8+)
- NLI integration (Phase 6.4 → TBD)
- Multiple backend instances (Phase 7 → Phase 7+)

---

## Impact on Timeline

**Original**: 24.75-28.75 days
**Simplified**: 15-18 days (baseline stable)
**With deferred features** (later implementation cycles): 22-25 days total

**Gain**: 3-10 days faster to stable multi-backend core.

---

## Recommendation

1. **Simplify PLAN.md**: Remove deferred sections. Keep only Phases -1 through 8 (core path).
2. **Maintain separate backlog** for deferred features (for Phase 9+).
3. **Freeze scope**: No new features during implementation cycle.
4. **Validate often**: Test backend switching after Phase 1.

---

## Next Steps

1. Update PLAN.md to remove overengineering
2. Start Phase 0: Generic node/edge format (simplified, no weights/embeddings)
3. Focus on core stability first, features second
