# Concept Deduplication System - Implementation Summary

## Overview
Three-phase concept deduplication system using Jaro-Winkler similarity scoring to eliminate duplicate/overlapping concepts in the ontology.

## Phase 1: Manual Merge Command ✅
Allows targeted merging of similar concepts.

```bash
voidm ontology concept merge <source_id> <target_id>
```

**Features:**
- Retargets all INSTANCE_OF edges from source → target
- Retargets IS_A relationships (hierarchy preserved)
- Handles duplicate edge prevention (DELETE before UPDATE)
- Transaction-safe (all-or-nothing)
- Reports edges retargeted

**Example:**
```bash
voidm ontology concept merge cecbb19d 30a782ee
# Output: Merged concept 'Dockerfile' into 'Docker'
# Memory edges retargeted: 4
```

## Phase 2: Auto-Detection of Duplicates ✅
Discovers similar concepts that are candidates for merging.

```bash
voidm ontology concept find-duplicates --threshold <0.0-1.0>
```

**Features:**
- Jaro-Winkler similarity scoring (0.0-1.0)
- All-pairs comparison of concept names
- Recommends merging smaller concept (fewer edges) into larger
- Sorted by similarity (highest first)
- JSON output available

**Example:**
```bash
voidm ontology concept find-duplicates --threshold 0.95

Found 9 merge candidates (similarity >= 0.95):

1. [7db72ecc] KEYPACKAGE (1 edges) → [475f9962] KeyPackages (1 edges)
   Similarity: 98.18%
   Action: voidm ontology concept merge 7db72ecc 475f9962

2. [84a2bc93] Todos (1 edges) → [e58852d8] TODO (18 edges)
   Similarity: 96.00%
   Action: voidm ontology concept merge 84a2bc93 e58852d8
```

## Phase 3: Prevention at Creation Time ✅
Proactively warns about similar concepts when adding new ones.

```bash
voidm ontology concept add "TODOs"
```

**Features:**
- Checks for similar concepts (threshold >= 0.8)
- Returns ConceptWithSimilarityWarning
- Shows similarity %, edge counts, merge commands
- User can decide whether to merge instead

**Example Output:**
```
Concept added: TODOs (88ad94b7)

⚠ Similar concepts found (consider merging):
  [e58852d8] TODO (96.0% similar, 19 edges) — voidm ontology concept merge 88ad94b7 e58852d8
  [6e098dd6] Todos→TODO (90.0% similar, 1 edges) — voidm ontology concept merge 88ad94b7 6e098dd6
  [95a36b5b] Tool (82.7% similar, 1 edges) — voidm ontology concept merge 88ad94b7 95a36b5b
```

## Implementation Details

### Core Functions
- `merge_concepts(pool, source_id, target_id)` → MergeResult
- `find_merge_candidates(pool, threshold)` → Vec<MergeCandidate>
- `find_similar_concepts(pool, name, threshold)` → Vec<SimilarConcept>
- `add_concept(pool, name, description, scope)` → ConceptWithSimilarityWarning

### Data Structures
```rust
pub struct MergeCandidate {
    pub source_id: String,
    pub source_name: String,
    pub target_id: String,
    pub target_name: String,
    pub similarity: f32,        // 0.0-1.0
    pub source_edges: i64,
    pub target_edges: i64,
}

pub struct SimilarConcept {
    pub id: String,
    pub name: String,
    pub similarity: f32,        // 0.0-1.0
    pub edge_count: i64,
}

pub struct ConceptWithSimilarityWarning {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub scope: Option<String>,
    pub created_at: String,
    pub similar_concepts: Vec<SimilarConcept>,
}
```

### Dependencies
- `strsim` crate for Jaro-Winkler similarity scoring

## Testing Results

### Automated Testing
- Phase 1: Merge with duplicate edge handling ✅
- Phase 2: find-duplicates with real ontology (186 concepts) ✅
- Phase 3: add-concept similarity warnings (tested "TODOs" → "TODO" 96% match) ✅

### Real-World Impact
- **Before:** 186 concepts with duplicates
- **After:** 181 concepts (-2.7%)
- **High-similarity candidates:** 9 → 3 (-66%)
- **Merged:** 
  - KEYPACKAGE → KeyPackages
  - Todos → TODO
  - Instance → Instance A → Instance B
  - Dockerfile/Dockerfiles → Docker
  - Alice→Bob → Alice↔Bob
  - VirtualProvider → VirtualProviderClass

## Backwards Compatibility
✅ All existing concepts preserved  
✅ Merging is user-initiated (no automatic destructive changes)  
✅ NULL quality_score handled gracefully  

## Future Phases

### Phase 4: Enrichment Integration
Auto-detect during NER enrichment:
- Before creating new concept, check similarity
- Offer merge suggestion to user
- Prevent duplication at source

### Phase 5: Batch Operations
```bash
voidm ontology concept merge-batch --threshold 0.85
# Prompts for each candidate, applies in transaction
```

### Phase 6: Concept Hierarchies
```bash
voidm ontology concept set-parent <child> <parent>
# Create IS_A relationships for categorization
# e.g., SQLite IS_A SQL, SQLAlchemy IS_A SQL
```

## Usage Guide

### Typical Workflow
1. **Discover duplicates:**
   ```bash
   voidm ontology concept find-duplicates --threshold 0.90 --json > candidates.json
   ```

2. **Review and merge:**
   ```bash
   # Review candidates.json
   voidm ontology concept merge 7db72ecc 475f9962
   voidm ontology concept merge 84a2bc93 e58852d8
   ```

3. **Prevent future duplicates:**
   - Use `add_concept` normally, heed warnings
   - System suggests merges automatically

### Best Practices
- Start with high threshold (0.95+) for high-confidence candidates
- Gradually lower to 0.85+ for exploratory deduplication
- Always review merge suggestions before executing
- Merge smaller concept into larger (fewer edge retargets)
- Monitor similarity warnings during enrichment

## References
- **Jaro-Winkler Distance:** https://en.wikipedia.org/wiki/Jaro%E2%80%93Winkler_distance
- **Concept deduplication RFC:** See voidm/docs/deduplication.md
- **Ontology design:** See voidm/docs/ontology.md

