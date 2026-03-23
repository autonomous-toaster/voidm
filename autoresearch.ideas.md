# Autoresearch Ideas: Search Recall Optimization

Promising optimizations discovered during autoresearch loop. Prioritized by likelihood and impact.

## High Priority (Strong Signal)

- **RRF k parameter sweep**: Test k=30 (aggressive fusion), k=60 (current), k=120 (conservative). May find better consensus point.
- **Score scaling multiplier**: Currently `0.2 + (rrf_score * 3.5).min(0.7)`. Try multiplier 2.0, 3.5, 5.0 — may be too aggressive/conservative.
- **Fetch limit tuning**: Currently `limit * 3` baseline, `limit * 5` with reranking. May be too low — try `limit * 4` / `limit * 6`.
- **Metadata ranking isolation**: Disable metadata ranking boost to measure its impact on recall. May be over-penalizing genuine matches.

## Medium Priority (Exploratory)

- **Signal weight rebalancing**: Currently equal weight in RRF. Try favoring BM25 for exact keyword matches (higher weight for rank 1).
- **Reranking threshold**: May be filtering out valid results. Check cross-encoder scoring distribution and lower threshold.
- **Top-rank bonus tuning**: Currently rank-1: +0.05, rank-2/3: +0.02. Try finer gradients (e.g., +0.08, +0.03).
- **Query expansion impact**: Disable query expansion to isolate its effect on recall. May be introducing spurious terms.

## Low Priority (Low ROI)

- **Fuzzy threshold tuning**: Currently 0.6 (60% match). Increasing may help typos but slow down. Measure cost/benefit.
- **Neighbor expansion**: Graph-based recall may be diluting direct search results. Disable or increase decay.

---

## Completed/Tried

(Populated during loop)
