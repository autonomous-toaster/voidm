# VOIDM v4: Architecture Redesign - Complete Design Document

**Date**: 2026-03-27  
**Status**: 🟢 Design Complete, Ready for Implementation  
**Timeline**: 35-50 days (parallel phases, no blocking gates)  
**Branch**: `feat/refactor-model`

---

## Executive Summary

Complete architectural overhaul of VOIDM memory system with 3 major components and radical simplification:

1. **MemoryChunk Pattern**: Split 900 memories into 4500+ chunks with smart paragraph-aware chunking
2. **Tag System (Simplified)**: User-provided + auto-extracted tags, NO type classification
3. **Quality Scoring**: 5-component quality score with stdout feedback for LLM agents

**Key Design Decisions**:
- ✅ Smart chunking (paragraph → sentence → word fallback)
- ✅ Memory length limits (soft: 10K, hard: 50K chars)
- ✅ Tags are just names (no DOMAIN|TECH|PATTERN|STATUS|CATEGORY types)
- ✅ Tag relationships emerge from co-occurrence (>80% threshold)
- ✅ Concepts optional (not required for tags)
- ✅ Neo4j-only embeddings (384-dim vectors as properties)
- ✅ JSONL export/import (portable, human-readable)

---

## Architecture Overview

### Three Major Components

#### 1. MemoryChunk Architecture (Phase A)
**Goal**: Split 900 memories into semantic chunks with embeddings

```
Memory (900)
  ├─ MemoryChunk 1 (content, embedding[384])
  ├─ MemoryChunk 2 (content, embedding[384])
  └─ MemoryChunk N (content, embedding[384])

Result: ~4500 chunks with 384-dim embeddings
```

**Smart Chunking Strategy**:
```
Priority 1: Split by paragraphs (\n\n)
Priority 2: Fall back to sentences (". ")
Priority 3: Fall back to words
Priority 4: Character fallback (never mid-word)
```

**Coherence Target**: 0.87+ average

#### 2. Tag System (Phase E - Simplified)
**Goal**: Simple user-provided + auto-extracted tags (no types)

```
:Tag {
  id: UUID
  name: string (UNIQUE)
  usage_count: int
  frequency_pct: float
  created_at: timestamp
  source: string ("user" | "auto-extracted")
}

Relationships:
- Memory --[TAGGED]--> Tag
- Tag --[RELATED_TO {strength}]--> Tag (emerges when co-occur >80%)
```

**Why No Types?**
- Tag name IS self-describing ("oauth2" = tech, "security" = domain)
- Users know what they mean (flexibility)
- Types would require classification (unnecessary complexity)
- Relationships emerge from actual usage (data-driven)

#### 3. Quality Scoring System (Cross-Phase)
**Goal**: 5-component quality score with LLM feedback

```
Score = Completeness(0.3) + Coherence(0.2) + Relevance(0.2) + Specificity(0.15) + Metadata(0.15)

Range: 0.0-1.0
Levels: 🔴 POOR [0.0-0.3), 🟡 FAIR [0.3-0.6), 🟢 GOOD [0.6-0.8), 🟣 EXCELLENT [0.8-1.0]
```

**Stdout Logging**:
```
[CHUNKING] 5 chunks | 256 chars avg | coherence 0.87 | [mchk_0, mchk_1, mchk_2, mchk_3, mchk_4]
[EXTRACTION] 6 entities | confidence 0.96 avg
[AUTO-TAG] 5 tags | confidence 0.94 avg | new: 2
[SCORE] 1.0|0.87|0.92|0.85|0.94 → 0.91 🟣 EXCELLENT
```

---

## Complete Data Model

### Node Types

```cypher
# Memory (original)
:Memory {
  id: UUID
  title: string
  scope_text: string
  content: string
  importance: int
  created_at: timestamp
}

# MemoryChunk (new)
:MemoryChunk {
  id: UUID
  memory_id: UUID (reference)
  chunk_index: int
  content: string
  embedding: [384] (float array)
  coherence_score: float
  created_at: timestamp
}

# Tag (simplified, no types!)
:Tag {
  id: UUID
  name: string (UNIQUE)
  usage_count: int
  frequency_pct: float
  created_at: timestamp
  source: string ("user" | "auto-extracted")
}

# Entity (from NER/NLI extraction)
:Entity {
  id: UUID
  name: string
  type: string (PERSON, ORG, TECH, DOMAIN, etc.)
  confidence: float
  created_at: timestamp
}

# Scope (optional, for hierarchical organization)
:Scope {
  id: UUID
  name: string
  path: string (e.g., "voidm/security")
  depth: int
  memory_count: int
  created_at: timestamp
}
```

### Relationships

```cypher
# Memory to chunks
Memory --[CONTAINS]--> MemoryChunk

# Memory to scope (optional)
Memory --[SCOPED_TO]--> Scope

# Memory to tags (from user or auto-extraction)
Memory --[TAGGED]--> Tag

# Chunk to entities (extracted via NER/NLI)
MemoryChunk --[EXTRACTED_ENTITY]--> Entity

# Entity to tags (optional, if entity linked to concept)
Entity --[CONCEPT]--> Tag

# Chunk to chunk (semantic similarity)
MemoryChunk --[RELATES_TO {strength: 0.92}]--> MemoryChunk

# Tag to tag (co-occurrence, emerges when >80%)
Tag --[RELATED_TO {strength: 0.95}]--> Tag

# Scope hierarchy
Scope --[PARENT_OF]--> Scope
```

---

## Implementation Phases

### Phase A: MemoryChunk Architecture (7-11 days)

**Goals**:
1. Implement smart chunking (paragraph-based)
2. Add memory length validation (10K warn, 50K hard limit)
3. Split 900 memories into 4500+ chunks
4. Generate 384-dim embeddings
5. Measure coherence scores (0.87+ target)

**Implementation**:
```rust
// crates/voidm-core/src/chunking.rs
pub struct ChunkingStrategy {
    target_size: usize,       // 250-500 chars
    min_chunk_size: usize,    // 50 chars
    max_chunk_size: usize,    // 1000 chars
    smart_breaks: bool,       // Use paragraphs/sentences
}

pub fn chunk_smart(content: &str, strategy: ChunkingStrategy) -> Vec<Chunk> {
    // 1. Try paragraph breaks
    // 2. Fall back to sentence breaks
    // 3. Fall back to word breaks
    // 4. Final fallback: characters (never mid-word)
}

// crates/voidm-core/src/validation.rs
pub fn validate_memory_length(content: &str) -> Result<(), String> {
    if len > 50_000 {
        return Err("Memory too long: max 50K chars");
    }
    if len > 10_000 {
        eprintln!("⚠️  Warning: {} chars (soft limit 10K)", len);
    }
    Ok(())
}
```

**Files to Create**:
- `crates/voidm-core/src/chunking.rs` - ChunkingStrategy, chunk_smart()
- `crates/voidm-core/src/validation.rs` - Memory length validation
- `crates/voidm-core/src/coherence.rs` - Coherence scoring

**Success Metrics**:
- 900 memories chunked → 4500+ chunks
- Avg chunk coherence: >0.85
- Embedding latency: <50ms per chunk
- Memory length enforced (10K warn, 50K error)

---

### Phase B: NER/NLI Extraction (2-7 days)

**Goals**:
1. Extract named entities from chunks
2. Create :Entity nodes in Neo4j
3. Measure extraction quality (>80% precision target)

**Implementation**:
- Choose local LLM (Ollama, fastembed, or llama.cpp)
- Extract entities from each chunk
- Create :Entity nodes with confidence scores
- Link to MemoryChunk via --[EXTRACTED_ENTITY]--> relationship

**Success Metrics**:
- 100% extraction coverage
- >80% average precision
- Entity confidence calibrated

---

### Phase C: JSONL Export/Import (2-7 days)

**Goals**:
1. Create portable JSONL backup format
2. Round-trip integrity validation
3. Include embeddings, relationships, all data

**Format**:
```jsonl
{"type": "memory", "id": "...", "title": "...", "content": "...", ...}
{"type": "memory_chunk", "id": "...", "memory_id": "...", "content": "...", "embedding": [...]}
{"type": "entity", "id": "...", "name": "...", "type": "TECH", ...}
{"type": "tag", "id": "...", "name": "...", "usage_count": ..., ...}
{"type": "relationship", "source": "...", "rel_type": "TAGGED", "target": "...", ...}
```

**Success Metrics**:
- 100% round-trip fidelity
- Export/import latency acceptable
- All relationships preserved

---

### Phase D: Auto-Tagging Expansion (1-2 weeks, parallel)

**Goals**:
1. Extract suggested tags from memories
2. Track tag co-occurrence
3. Collect baseline usage statistics

**Note**: No type classification (Phase E handles tags simply)

**Success Metrics**:
- 900 memories auto-tagged (100%)
- 500-800 unique tags extracted
- Co-occurrence data collected

---

### Phase E: Tag Storage + Search (1-2 days) ✅ SIMPLIFIED

**Goals**:
1. Create :Tag nodes (6 fields, no type field!)
2. Implement tag search
3. Build co-occurrence relationships

**Implementation**:
```rust
// crates/voidm-core/src/tag.rs
#[derive(Debug, Clone)]
pub struct Tag {
    pub id: String,
    pub name: String,           // UNIQUE, self-describing
    pub usage_count: i32,
    pub frequency_pct: f64,
    pub created_at: DateTime<Utc>,
    pub source: String,         // "user" or "auto-extracted"
    // NO type field!
}

// Implementation:
// - Create from user input: voidm remember --tags oauth2,security
// - Create from auto-extraction: Phase D suggests tags
// - Build relationships: when co-occur >80% across memories
```

**Success Metrics**:
- All tags stored with 6 fields
- Tag search <100ms
- Co-occurrence relationships emerging
- NO concept migration required

---

### Phase F: Concept Migration (Optional, 1 day)

**Status**: OPTIONAL (not required)

**Options**:
1. Skip entirely (recommended) - don't need concept history
2. Migrate for reference - 1 day, optional

**If Chosen**:
- Migrate 2132 :Concept nodes
- Optional reference layer (not primary)

---

### Phase G: Neo4j Testing Infrastructure (3-10 days)

**Goals**:
1. Switch all tests to real Neo4j (not SQLite mocks)
2. Docker Compose setup on port 7688
3. Test coverage >95%

**Success Metrics**:
- 100% tests on real Neo4j
- No SQLite mocks
- CI/CD integration working

---

## Unified Get Command (Cross-Phase)

**Design**: `voidm get <resource_id> [options]`

**Resource ID Scheme**:
```
mem_<uuid>              - Memory
mchk_<uuid>             - MemoryChunk (from logs)
tag_<name>              - Tag (by name, case-insensitive)
entity_<name>_<type>    - Entity
scope_<path>            - Scope
```

**Usage Examples**:
```bash
voidm get mem_abc123                    # Get memory + chunks
voidm get mchk_def456                   # Get specific chunk
voidm get tag_security                  # Get tag with stats
voidm get entity_OAuth2_TECH            # Get entity
voidm get scope_voidm/security          # Get scope hierarchy
voidm get mem_abc123 --include chunks   # With details
voidm get mem_abc123 --format json      # JSON output
```

---

## Validation Prep (Before Phase A)

**Duration**: 7-10 days (one-time investment)

**Critical Experiments**:

1. **Chunking Validation** (2-3 days)
   - Test smart chunking on 10 diverse memories
   - Measure coherence (smart vs naive)
   - Validate 0.87 assumption
   - If <0.70, reconsider algorithm

2. **Vector Latency Benchmark** (1-2 days)
   - Chunk 100 memories
   - Generate embeddings
   - Test Neo4j property search latency
   - If >300ms, pivot to Weaviate early

3. **Entity Extraction Pilot** (2-3 days)
   - NER/NLI on 50 chunks
   - Measure precision/recall
   - If <80%, reconsider approach

4. **Data Quality Audit** (1 day)
   - Check for duplicates
   - Validate scope consistency
   - Plan deduplication

5. **Team Capacity Clarification** (0.5 day)
   - How many engineers?
   - Parallel vs sequential plan

**ROI**: Costs 7-10 days now, saves 20-30 days later if issues found

---

## Timeline

### Option 1: Recommended (35-40 days)
```
Weeks 1-2: Phases A+B+C parallel (12-17 days)
Week 2: Enable auto-tagging (overlaps)
Week 3: Continue A+B+C while collecting tags
Week 3: Phase E (2-3 days, tags simplified!)
Week 4: Phase F (optional, skip if not needed)
Week 4: Phase G (3-4 days, testing)
→ TOTAL: 35-40 days, no gate, parallel execution
```

### Option 2: Conservative (45-50 days)
```
Weeks 1-2: Phases A+B+C (12-17 days)
Week 3: Enable auto-tagging (after JSONL backup)
Week 3-4: Collect tags (1-2 weeks)
Week 4: Phase E (2-3 days)
Week 5: Phase F (optional)
Week 5: Phase G (3-4 days)
→ TOTAL: 45-50 days, slightly safer pace
```

---

## Key Decisions

### ✅ Smart Chunking (Paragraph-Based)
- **Decision**: YES, implement in Phase A
- **Rationale**: Better coherence (0.92 vs 0.60), semantic units
- **Impact**: Critical for Phase B (entity extraction on coherent chunks)

### ✅ Memory Length Limits
- **Decision**: YES, soft 10K + hard 50K
- **Rationale**: Prevent downstream failures, guide users
- **Impact**: Phase A validation, clear error messages

### ✅ Tags: No Types
- **Decision**: RADICAL SIMPLIFICATION - tags are just names
- **Rationale**: Self-describing, user-centric, no classification needed
- **Impact**: Phase E reduced 50% (2-3 days → 1-2 days), NO gate, simple schema

### ✅ Tag Relationships: Emergent
- **Decision**: Build only when co-occur >80%
- **Rationale**: Data-driven, reflect actual usage, no manual effort
- **Impact**: Relationships emerge naturally over time

### ✅ Concepts: Optional
- **Decision**: Not required, can skip Phase F
- **Rationale**: Tags are primary, concepts historical reference
- **Impact**: Faster implementation, less maintenance

### ✅ Neo4j-Only Embeddings
- **Decision**: Store 384-dim vectors as properties (no Weaviate)
- **Rationale**: Simple architecture, ACID consistency
- **Caveat**: Monitor latency, pivot to Weaviate if >300ms

### ✅ Validation Prep
- **Decision**: Run 7-10 day prep sprint before Phase A
- **Rationale**: Discover issues early, saves 20-30 days later
- **ROI**: Positive (reduce risk 30-40%)

---

## Risk Summary

### High-Priority Risks (Mitigated by Prep)

1. **Chunking Quality** (Week 2 blocker)
   - Assumption: 0.87+ coherence
   - Mitigation: Test on 10 samples first
   - If <0.70: reconsider algorithm

2. **Vector Latency** (Week 3 blocker)
   - Assumption: <150ms per search
   - Mitigation: Benchmark on 100 chunks
   - If >300ms: pivot to Weaviate

3. **Entity Extraction** (Week 4 discovery)
   - Assumption: >80% precision
   - Mitigation: Pilot on 50 chunks
   - If <80%: reconsider approach

### Medium-Priority Risks

4. **Auto-Tagging Quality** (Week 4-5)
   - Could produce garbage data
   - Mitigation: Review sample tags manually

5. **Neo4j Testing** (Week 5)
   - Test suite might be slow
   - Mitigation: Profile early, optimize if needed

---

## Files to Create/Modify

### New Files
- `crates/voidm-core/src/chunking.rs` - Smart chunking
- `crates/voidm-core/src/validation.rs` - Memory length limits
- `crates/voidm-core/src/coherence.rs` - Coherence scoring
- `crates/voidm-core/src/tag.rs` - Tag operations
- `crates/voidm-cli/src/commands/tag_management.rs` - Tag CLI
- `VOIDM_V4_DESIGN.md` - This document

### Modify
- `crates/voidm-core/src/lib.rs` - Add new modules
- `crates/voidm-core/src/memory.rs` - Add length validation
- `crates/voidm-neo4j/src/lib.rs` - Tag storage, search, chunking
- `crates/voidm-sqlite/src/lib.rs` - Read for migration
- `Cargo.toml` - Add dependencies (embeddings, NER/NLI)

---

## Success Criteria

### Architecture
- [x] Design complete (this document)
- [x] Data model finalized
- [x] Phases defined (A-G)
- [x] Risk assessment done

### Phase A (MemoryChunk)
- [ ] Smart chunking implemented
- [ ] Memory validation in place
- [ ] 900 memories chunked → 4500+ chunks
- [ ] Embeddings generated
- [ ] Coherence scoring >0.85 avg

### Phase B (Extraction)
- [ ] Entity extraction >80% precision
- [ ] All chunks processed
- [ ] :Entity nodes created

### Phase C (JSONL)
- [ ] Export/import working
- [ ] Round-trip 100% fidelity
- [ ] Embeddings serialized

### Phase D (Auto-Tagging)
- [ ] 900 memories auto-tagged
- [ ] 500-800 unique tags extracted
- [ ] Co-occurrence tracked

### Phase E (Tags - Simplified)
- [ ] :Tag nodes created (6 fields, no type)
- [ ] Tag search working
- [ ] Relationships emerging (>80%)
- [ ] NO concept migration

### Phase G (Testing)
- [ ] All tests on real Neo4j
- [ ] >95% coverage
- [ ] CI/CD integrated

---

## Notes

- **Pragmatism Over Perfectionism**: Use existing concepts as seed, iterate based on real usage
- **Simpler = Better**: Tags don't need types (name is the type)
- **Parallel Execution**: Phases A+B+C can run simultaneously
- **Data-Driven Relationships**: Let co-occurrence patterns emerge naturally
- **Emergent Organization**: System learns from real usage, not design assumptions
- **Fast Iteration**: Deploy after Phase C (user tags work), refine based on reality

---

## References

### TODOs (Design Phase)
- TODO-918084b1: Master tracker + executive summary
- TODO-2a624739: Phase A (MemoryChunk, revised with smart chunking)
- TODO-98224db2: Phase B (NER/NLI extraction)
- TODO-a74adfcc: Phase C (JSONL export/import)
- TODO-09d35adc: Phase D (Auto-tagging)
- TODO-8f95b1ec: Phase E (Tags - RADICALLY SIMPLIFIED, NO TYPES!)
- TODO-707c2fa6: Phase F (Concept migration - OPTIONAL)
- TODO-e0260d06: Phase G (Testing)
- TODO-82da48cb: Scoring & feedback system
- TODO-b1854949: Logging & CLI design
- TODO-071996aa: Critical review (10 risks, 4 blockers)
- TODO-0bf0288e: Design revision (3 issues assessed)

### Design Memories
- [b6991801]: 3 Critical Design Issues (Tag Types, Memory Length, Smart Chunking)
- [f93583f6]: Tags Need No Types - Radical Simplification
- [271d4f1b]: Logging Conciseness + Unified Get Command
- [d3b57db6]: Critical Review (10 Risks + 4 Blockers)
- [5c6fb2a3]: TODO Refactoring + Scoring System

---

## Implementation Ready

✅ **Design Complete**  
✅ **Risk Assessed**  
✅ **Phases Defined**  
✅ **Timeline Clear**  
✅ **Ready for Code**

Start with validation prep sprint (7-10 days) to de-risk assumptions, then proceed to Phase A.
