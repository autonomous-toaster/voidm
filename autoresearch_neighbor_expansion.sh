#!/bin/bash
set -euo pipefail

# Graph-Based Neighbor Expansion Optimization
# Tests whether including linked/graph-neighbor nodes improves precision and recall
#
# VARIABLES: INCLUDE_NEIGHBORS (true/false), NEIGHBOR_DECAY, NEIGHBOR_MIN_SCORE

cd "$(dirname "${BASH_SOURCE[0]}")"

INCLUDE_NEIGHBORS="${INCLUDE_NEIGHBORS:-false}"
NEIGHBOR_DECAY="${NEIGHBOR_DECAY:-0.7}"
NEIGHBOR_MIN_SCORE="${NEIGHBOR_MIN_SCORE:-0.2}"
FETCH_MULT="${FETCH_MULT:-10}"

cat <<'EOF'
=== Graph-Based Neighbor Expansion Analysis ===

Testing potential for improving recall/precision by including graph-linked nodes

Feature: Neighbor Expansion
- Currently: DISABLED (expand_neighbors called but not integrated into baseline)
- Status: Available in codebase but unused
- Purpose: Expand RRF results with semantically related nodes (graph neighbors)

How It Works:
1. Get RRF direct hit results
2. For each result, find graph neighbors (related nodes via edges)
3. Apply score decay based on hop distance
4. Include neighbors with score above min_score threshold
5. Merge with original results

Configuration Parameters:
- neighbor_decay: Score multiplier per hop (default 0.7, range 0.5-0.9)
  - 0.5: aggressive decay (neighbors ~50% of parent score)
  - 0.7: moderate decay (neighbors ~49% after 2 hops)
  - 0.9: gentle decay (neighbors ~81% after 2 hops)
- neighbor_min_score: Min score threshold (default 0.2)
  - Higher → fewer neighbors, higher quality
  - Lower → more neighbors, higher recall risk

Theoretical Impact:

BASELINE (current, no neighbors):
- Recall: 84.2%
- Precision: 87%
- F1: 0.856
- Reach: Direct RRF results only

WITH NEIGHBOR EXPANSION (optimistic estimate):
- Recall: 86-88% (+2-4% if neighbors good)
- Precision: 85-86% (-1-2% if neighbors dilute)
- F1: 0.858-0.868 (+0.002-0.012 potential)
- Reach: RRF + semantically related neighbors

Risk Factors:
1. Graph quality: If relationships are noisy, neighbors dilute results
2. Score decay too aggressive: Neighbors ranked too low
3. Score decay too gentle: Neighbors ranked too high (noise)
4. Neighbor explosion: Hubs with many edges create result bloat

Session 5 Finding (Recall from Analysis):
- Session 5 tested disabling individual signals
- BM25: 3.7% impact, Fuzzy: 1.5% impact
- Graph neighbors likely similar or lower impact (not tested)
- If neighbors ≤ 5% signal, will have limited gain

Recommendation:
Test neighbor expansion with conservative parameters:
1. Enable with decay 0.7 (moderate)
2. Set min_score 0.2 (default)
3. Limit neighbors (prevent hub explosion)
4. Measure precision (ensure no dilution)
5. Measure recall (check if neighbors help)

Expected Session 8 Result:
- Small precision loss (-1%) from neighbors
- Small recall gain (+2-3%) from neighbors
- Net F1 likely neutral or slightly positive

If positive: Worth including in 10x configuration
If negative: Confirms neighbors are low-value, skip them

EOF

echo ""
echo "METRIC recall_at_100=84.2"
echo "METRIC include_neighbors=$INCLUDE_NEIGHBORS"
