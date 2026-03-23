# Autoresearch: Optimize Search Recall

## Objective

Diagnose and optimize search recall for voidm's hybrid search system. Recent RRF + metadata ranking changes appear to have degraded recall quality. Goal: identify which factors (RRF parameters, signal weights, reranking, metadata ranking) are causing lower recall and optimize them back to previous levels or better.

**Workload**: Custom test database with 1,000 realistic memories across different types and scopes. Queries selected to exercise all three signals (vector, BM25, fuzzy). Metrics: recall@100, recall@50, NDCG (normalized discounted cumulative gain), average precision.

## Metrics

- **Primary**: `recall_at_100` (%) — percentage of true relevant results found in top 100. Higher is better.
- **Secondary**: `recall_at_50`, `ndcg_at_100`, `avg_precision` — additional quality signals to catch tradeoffs.

## How to Run

```bash
./autoresearch.sh
```

Output format:
- `METRIC recall_at_100=<number>` (primary)
- `METRIC recall_at_50=<number>` (secondary)
- Additional diagnostics for decision-making

## Files in Scope

- `crates/voidm-core/src/rrf_fusion.rs` — RRF parameters (k constant, top-rank bonuses)
- `crates/voidm-core/src/search.rs` — signal weights, fetch limits, score scaling, reranking behavior
- `crates/voidm-core/src/config.rs` — search config defaults (signal enable/disable, reranking)
- `autoresearch.sh` — benchmark script using custom test database

## Off Limits

- Core memory storage (database schema)
- Embedding model (fastembed) — only tune RRF/ranking params
- BM25/fuzzy algorithms themselves (only tune signal weights/scaling)
- Command-line interface or CLI behavior
- Do NOT cheat: no hardcoding test results, no disabling signals to artificially boost single-signal recall

## Constraints

- All tests must pass: `cargo test --test quality_verification` and `cargo test --test rrf_search_benchmark`
- Recall must be measured honestly against a consistent ground truth (queries defined in `autoresearch.sh`)
- No new dependencies
- No reduction in supported search modes or features

## What's Been Tried

(To be updated as experiments run)

### Session Start (2026-03-23)
- Baseline established: recall_at_100 measured on 1K-memory test database with 100 queries
- Initial RRF k=60, rank bonuses (0.05 rank-1, 0.02 rank-2/3)
- Three signals active: vector (fastembed), BM25 (FTS5), fuzzy (Levenshtein)
- Metadata ranking active (source, author boost)

### Initial Hypothesis
Search recall degraded after:
1. RRF + metadata ranking integration (commit 7c954b9 + c3be82d)
2. Possible: RRF score scaling too aggressive (0.2 + rrf_score*3.5 → [0.2, 0.9])
3. Possible: fetch_limit calculation too conservative
4. Possible: top-rank bonuses misaligned with true relevance signal

### Next Steps
- Measure baseline recall precisely
- Sweep RRF k parameter (30, 60, 120)
- Sweep score scaling multiplier (2.0, 3.5, 5.0)
- Sweep fetch_limit multiplier (2x, 3x, 4x)
- Test impact of metadata ranking enable/disable
- Validate reranking not over-penalizing recall
