# Autoresearch: Optimize Tinyllama Prompts for HyDE (Hypothetical Document Embeddings)

## Objective

Optimize tinyllama prompts to generate high-quality Hypothetical Document Embeddings (HyDE) for semantic search enhancement. The goal is to replace QMD model with optimized tinyllama HyDE generation while maintaining/improving search quality.

**What is HyDE?**
- Generate hypothetical documents for a query that would contain relevant content
- These hypothetical documents are embedded and used to retrieve similar real documents
- More effective for semantic search than traditional query expansion alone
- Output format: **hyde:** section with 3-5 conceptual document pieces

**Scope**: Tinyllama prompts only for HyDE generation. No model architecture changes, no benchmark tampering.

## Motivation

Currently, voidm uses the QMD model (tobil/qmd-query-expansion-1.7B) which outputs:
- `lex:` section (lexical terms)
- `vec:` section (vector/semantic terms)  
- `hyde:` section (hypothetical document terms)

**Goal**: Optimize tinyllama prompts to generate better HyDE sections to replace QMD.

## Metrics

- **Primary**: `hyde_quality_score` (0.0–1.0, higher is better)
  - Measures quality of hypothetical documents for semantic search
  - Dimensions:
    - **Relevance** (40%): Do generated docs directly answer the query?
    - **Diversity** (25%): Do docs cover different aspects of the topic?
    - **Coherence** (20%): Are docs grammatically sound and realistic?
    - **Embedding-friendliness** (15%): Will these embed well for search?

- **Secondary**:
  - `latency_ms`: Generation latency (target: <300ms)
  - `parse_success_rate`: Percentage of valid HyDE outputs (target: >95%)
  - `doc_count_avg`: Average hypothetical documents per query (target: 3-5)

## How to Run

```bash
./autoresearch_hyde.sh
```

Outputs `METRIC name=value` format.

## Files in Scope

### Core Implementation
- **`crates/voidm-core/src/gguf_query_expander.rs`** - HyDE parsing logic
- **`crates/voidm-core/src/query_expansion.rs`** - Tinyllama prompt templates
- New: **`autoresearch_hyde.sh`** - Quality harness for HyDE

### Test/Benchmark Files
- **`crates/voidm-core/tests/query_expansion_benchmark.rs`** - Test queries

## Off Limits

- ❌ Do NOT modify model architecture
- ❌ Do NOT add new crate dependencies
- ❌ Do NOT change public APIs
- ❌ Do NOT hardcode query-specific documents
- ❌ Do NOT use real documents (defeats HyDE purpose)
- ❌ Do NOT modify HyDE parsing logic in gguf_query_expander.rs

## Constraints

- ✅ All 144+ lib tests must pass
- ✅ HyDE output must be valid and parseable
- ✅ No new external dependencies
- ✅ Quality improvements must generalize to diverse queries
- ✅ Backward compatible (existing API unchanged)
- ✅ Latency <300ms maintained

## Success Criteria

- ✅ Primary metric (hyde_quality_score) improves from baseline
- ✅ All lib tests pass throughout
- ✅ Parse success rate >95%
- ✅ No hallucinations or nonsensical documents
- ✅ Generalization to held-out test queries
- ✅ Better quality than QMD baseline (where possible to measure)

## Optimization Strategy

### Phase 1: Baseline & Prompt Tuning
- Create specialized HyDE prompt template
- Add diverse few-shot examples (5-8 good HyDE examples)
- Focus on relevance and coherence

### Phase 2: Format & Structure
- Optimize output format (separator, structure)
- Ensure consistent parsing
- Add constraints (3-5 documents, appropriate length)

### Phase 3: Semantic Optimization
- Diversify document perspectives
- Add implicit relevance checking
- Optimize for embedding quality

### Phase 4: Integration Testing
- Validate with actual search task
- Measure end-to-end improvement
- Compare with QMD baseline

## Implementation Notes

### HyDE Document Quality
A good hypothetical document for query "machine learning" might be:
```
hyde: |A comprehensive guide to deep learning fundamentals|; 
       |How to build and train neural networks effectively|; 
       |Understanding backpropagation and gradient descent|
```

### Measuring Quality
1. Run tinyllama on test queries (20+ diverse)
2. For each query, score generated documents on:
   - Relevance (manually or via embedding similarity)
   - Diversity (do docs cover different angles?)
   - Coherence (readable, realistic?)
   - Embedding-friendliness (diversity, informativeness)
3. Average across all test queries

## Session Tracking

- **Session**: autoresearch/tinyllama-hyde-prompts-20260319
- **Date**: 2026-03-19
- **Baseline**: TBD (establish in Exp #1)
- **Target**: 0.90+ (equivalent to or better than QMD)
- **Experiments**: TBD
- **Status**: In Progress
