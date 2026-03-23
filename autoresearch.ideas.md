# Autoresearch Ideas: Search Recall Optimization

## Completed & Verified

✅ **RRF Bonuses** - DONE (Session 1, Commit 838239b): rank_1_bonus 0.05→0.12, rank_2_3_bonus 0.02→0.06. Improved consensus detection.

✅ **Metadata Weights** - DONE (Session 2, Commit 931732b): Halved all default_weight_* functions by 50%. Reduced metadata suppression of RRF.

✅ **Fetch Limit Tuning** - DONE (Session 3, Commit 631b457): Aggressive progression from 3x→5x→8x→10x→12x→**20x**. **+10.6% recall improvement** (79.9%→90.5%). Identified as dominant lever.

✅ **Fuzzy Threshold** - TESTED (Session 3): Swept 0.4-0.7, zero effect. Not a recall bottleneck.

✅ **RRF Bonus Variations** - TESTED (Session 3): Confirmed 0.12/0.06 is already optimal. Variations (0.10/0.04, 0.15/0.08) showed zero effect.

## High Priority (Remaining)

- **Query Expansion Impact**: Test disabling HyDE expansion to isolate effect. Not integrated into main path yet.
- **Reranking Behavior**: Feature-gated; hard to test without config changes. Potential high impact if enabled.
- **Per-Use-Case Multiplier**: Consider parameterizing fetch_limit for A/B testing (8x for speed, 20x for recall-critical).

## Medium Priority  

- **Signal Disable Tests**: Disable BM25/fuzzy individually to measure importance weighting.
- **Reranking with Fetch**: Test if higher fetch improves reranking effectiveness (separate tuning path).
- **Metadata Ranking Rebalance**: Current -50% works; test edge cases (domain-specific weights).

## Low Priority (Low ROI / Already Tuned)

- **RRF k parameter**: Current 60 already optimal (tested 45, 120 in Session 1).
- **Score Scaling**: Current 3.5 already optimal (tested 2.5, 4.5 in Session 2).
- **Importance Boost**: Current 0.02 already good (tested 0.01-0.03 in Session 2).
- **Neighbor Expansion Decay**: Graph neighbors may dilute direct results - low priority.

---

## Session Progress Summary

| Session | Focus | Baseline | Result | Key Finding |
|---------|-------|----------|--------|-------------|
| 1 | RRF bonuses | 85% synthetic | 100% synthetic | Consensus rewards work |
| 2 | Realistic benchmark | 100% synthetic | 79.9% realistic | Synthetic ceiling misleading |
| 2+ | Metadata + Fetch | 79.9% | 81.1% | Metadata -50%, Fetch 5x |
| 3 | Fetch limit deep dive | 81.1% | **90.5%** | **Fetch limit is dominant lever** |

**Current Best**: 90.5% on realistic benchmark with:
- RRF bonuses: r1=0.12, r23=0.06 ✓
- Metadata: -50% weights ✓
- Fetch limit: **20x** (was 3x) ✓✓✓
- Score scaling: 3.5 (optimal) ✓
- Importance: 0.02 (optimal) ✓


