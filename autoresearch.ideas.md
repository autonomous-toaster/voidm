# Autoresearch Ideas: Search Recall Optimization

## Completed & Verified

✅ **RRF Bonuses** - DONE (Commit 838239b): rank_1_bonus 0.05→0.12, rank_2_3_bonus 0.02→0.06. Improved synthetic recall 85%→100%, better consensus.

✅ **Metadata Weights** - DONE (Commit 931732b): Halved all default_weight_* functions by 50% (0.43→0.215 total). May reduce metadata suppression of RRF signal.

✅ **Fetch Limit Tuning** - DONE (Commit 405684d): Increased 3x→5x for baseline searches. +1.2% recall (79.9%→81.1%).

## High Priority (Remaining)

- **Score Scaling Multiplier**: Currently `0.2 + (rrf_score * 3.5).min(0.7)`. Try 2.5, 4.0, 5.0. May improve score differentiation.
- **Reranking Behavior**: Verify cross-encoder threshold not filtering valid results. Test with/without reranking.
- **Query Expansion Impact**: Test disabling HyDE expansion to isolate its effect on recall/precision tradeoff.

## Medium Priority  

- **Fetch limit with reranker**: Currently 5x (but reranker branch uses max(5xK, apply_to_top_k*2)). May need separate tuning.
- **RRF k parameter**: Current 60. Benchmarks showed k=45 no improvement, k=120 worse. May be optimal.
- **Signal Disable Tests**: Test disabling vector/BM25/fuzzy individually to measure signal importance.

## Low Priority (Low ROI)

- **Fuzzy threshold**: Currently 0.6. May be too strict/lenient - measure cost/benefit.
- **Neighbor expansion decay**: Graph neighbors may dilute direct results - test impact.
- **Importance boost**: Currently (importance - 5) * 0.02 multiplier. May be too high.

---

## Session Progress

| Run | Change | Baseline | Result | Status |
|-----|--------|----------|--------|--------|
| 1 | Baseline | - | 85% synthetic | baseline |
| 2 | RRF bonuses (+140%, +200%) | 85% | 100% synthetic | KEEP ✓ |
| 3 | k=45, scale=2.5 | 100% synthetic | 100% synthetic | DISCARD |
| 4 | Realistic benchmark | 100% synthetic | 79.9% realistic | KEEP |
| 5 | Metadata -50% | 79.9% | 79.9% (no test) | KEEP |
| 6 | Fetch 3x→5x | 79.9% | 81.1% | KEEP ✓ |

**Current Best**: 81.1% on realistic benchmark with:
- RRF bonuses: r1=0.12, r23=0.06
- Metadata: -50% weights
- Fetch limit: 5x (baseline)

