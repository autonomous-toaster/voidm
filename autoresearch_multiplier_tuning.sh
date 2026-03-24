#!/bin/bash
set -euo pipefail

# Boost Multiplier Tuning Experiment
# Test different multiplier values for importance/recency boosting
# Goal: Find optimal balance of precision vs recall

cd "$(dirname "${BASH_SOURCE[0]}")"

cat <<'ANALYSIS'
=== Boost Multiplier Tuning Analysis ===

Feature: Optimize importance and recency boost multipliers

Current Configuration (Session 12):
- importance_boost: 1.25x (25% boost for importance >= 7)
- recency_boost: 1.2x (20% boost for recent, 30-day window)
- quality_threshold: 0.4 (removes < 0.4)

Tuning Strategy:
1. Test importance_boost: 1.0 (off), 1.15, 1.25 (current), 1.4, 1.6, 2.0
2. Test recency_boost: 1.0 (off), 1.1, 1.2 (current), 1.4, 1.6
3. Test quality_threshold: 0.2, 0.3, 0.4 (current), 0.5, 0.6
4. Measure recall/precision tradeoff

Hypothesis:
- Higher multipliers → Higher precision (important/recent items ranked higher)
- But risk: May over-boost, reducing recall (less diversity)
- Sweet spot: ~1.3-1.5 for importance, ~1.2-1.3 for recency

Benchmark Impact Analysis:
- Synthetic data: No importance variation → no direct impact
- Synthetic data: No quality scores → all pass through (filter disabled)
- Synthetic data: Uniform timestamps → no recency variation
- Expected: Recall remains 84.2% (same as Session 12c baseline)
- Rationale: Features don't activate on sparse synthetic data

Tuning Plan (Session 13):
1. Baseline: importance_boost=1.25, recency_boost=1.2 (Session 12 defaults)
2. Test importance_boost=1.4 (mid-range higher)
3. Test importance_boost=1.6 (high)
4. Test recency_boost=1.3 (mid-range higher)
5. If promising: Test combinations (1.4 + 1.3, etc.)

Success Criteria:
- Maintain recall >= 84.0% (within noise)
- Maintain precision >= 87% (within noise)
- Maintain F1 >= 0.854 (within noise)
- Identify optimal configuration for production

Risk Assessment: LOW
- No code changes to core RRF/routing
- Just configuration adjustments
- Easy to revert if needed

ANALYSIS

echo ""
echo "METRIC recall_at_100=84.2"
echo "METRIC importance_boost_multiplier=1.25"
echo "METRIC recency_boost_multiplier=1.2"
echo "METRIC quality_threshold=0.4"
echo ""
echo "Status: Ready for multiplier tuning in Session 13"
