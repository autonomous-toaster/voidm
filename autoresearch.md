# Autoresearch: Optimize Search Recall

## Objective

Diagnose and optimize search recall for voidm's hybrid search system. Recent RRF + metadata ranking changes appear to have degraded recall quality. Goal: identify which factors (RRF parameters, signal weights, reranking, metadata ranking) are causing lower recall and optimize them back to previous levels or better.

**Workload**: Synthetic benchmark simulating hybrid search with 3 signals (vector, BM25, fuzzy). Metrics: recall@100, precision@10, NDCG@100, contextual relevance, weighted quality score.

## Metrics

- **Primary**: `recall_at_100` (%) — percentage of true relevant results found in top 100. Higher is better.
- **Secondary**: `precision_at_10`, `ndcg_at_100`, `contextual_relevance`, `weighted_score` — prevent overfitting to single metric.

## How to Run

```bash
./autoresearch.sh
```

Output format:
- `METRIC recall_at_100=<number>` (primary, in %)
- Additional metrics for comprehensive quality assessment

## Files in Scope

- `crates/voidm-core/src/rrf_fusion.rs` — RRF parameters (k constant, top-rank bonuses)
- `crates/voidm-core/src/search.rs` — signal weights, fetch limits, score scaling, metadata ranking behavior
- `crates/voidm-core/src/config.rs` — search config defaults
- `autoresearch.sh` — comprehensive benchmark script

## Off Limits

- Core memory storage (database schema)
- Embedding model (fastembed)
- BM25/fuzzy algorithms themselves
- Do NOT cheat: no hardcoding test results

## Constraints

- Recall must not degrade below 80%
- All tests must pass (when code compiles)
- No new dependencies
- Balanced optimization across recall, precision, NDCG, and contextual relevance

## What's Been Tried

### Run 1: Baseline (k=60, r1_bonus=0.05, r23_bonus=0.02, score_scale=3.5)
- Result: 85% recall@100

### Run 2: Increased RRF bonuses (k=60, r1_bonus=0.12, r23_bonus=0.06)
- Result: 100% recall@100, 78.5% precision@10, 0.82 NDCG, 80% contextual
- Status: IMPROVED ✓ (+17.6% recall)
- Decision: KEEP - Major improvement in consensus detection

### Run 3: Reduced k from 60→45 + score_scaling 3.5→2.5
- Result: 100% recall@100 (no change)
- Status: Same performance
- Decision: DISCARD - No marginal benefit, keep simpler version

### Remaining Ideas
- Test metadata ranking impact (disable/tune source/author bonuses)
- Adjust fetch_limit multiplier (currently 3x for baseline)
- Test reranking enable/disable impact on real queries
- Query expansion impact isolation
- Graph-based neighbor expansion impact

## FINDINGS & RECOMMENDATIONS

### Key Discovery: RRF Bonus Tuning is Critical
Increasing RRF top-rank bonuses from (0.05, 0.02) → (0.12, 0.06) provided **+17.6% recall improvement** in synthetic benchmark and balanced metrics across all quality dimensions.

### Improvement Breakdown
| Parameter | Before | After | Impact |
|-----------|--------|-------|--------|
| RRF k | 60 | 60 | (unchanged) |
| rank_1_bonus | 0.05 | 0.12 | +140% |
| rank_2_3_bonus | 0.02 | 0.06 | +200% |
| score_scaling | 3.5 | 3.5 | (unchanged) |
| **Recall@100** | 85% | 100% | **+17.6%** ✓ |
| Precision@10 | - | 78.5% | - |
| NDCG@100 | - | 0.82 | - |
| Contextual Relevance | - | 80% | - |

### Root Cause Analysis
Previous version underweighted consensus across signals. Memories that ranked high in just one signal (e.g., top vector result but not in BM25) were prioritized equally with memories appearing in multiple signals. The bonuses fix this by rewarding consensus.

### Implementation Location
**File**: `crates/voidm-core/src/rrf_fusion.rs`
**Function**: `impl Default for RRFConfig`
**Lines**: 32-38

### Next Optimization Frontier
1. **Metadata ranking weights** - Currently add 0.38 total to base score (may over-suppress RRF signal)
2. **Query expansion impact** - Test disabling to measure its effect on recall
3. **Reranking behavior** - Verify cross-encoder doesn't prematurely filter relevant results
4. **Fetch limit tuning** - Try increasing from 3x to 4x for more consensus opportunities

### Real-World Validation Note
Synthetic benchmark achieves 100% recall (ceiling effect). Real-world testing against actual queries needed to:
- Measure practical recall (not synthetic ceiling)
- Identify edge cases where bonuses hurt precision
- Validate contextual relevance improvements

