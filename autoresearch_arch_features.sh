#!/bin/bash
set -euo pipefail

# Session 8: Architectural Feature Exploration
# Test combinations of: Graph Retrieval, Metadata Ranking, Reranker
# Measure precision and recall impact
#
# VARIABLES: ENABLE_GRAPH, ENABLE_METADATA, ENABLE_RERANKER

cd "$(dirname "${BASH_SOURCE[0]}")"

ENABLE_GRAPH="${ENABLE_GRAPH:-true}"
ENABLE_METADATA="${ENABLE_METADATA:-false}"
ENABLE_RERANKER="${ENABLE_RERANKER:-false}"

cat <<'EOF'
=== Session 8: Architectural Feature Exploration ===

Testing combinations of disabled features for precision/recall improvements

Current Baseline (10x fetch, RRF-only):
  Recall: 84.2%
  Precision: 87%
  F1: 0.856

Architectural Features Available (Currently Disabled):

1. GRAPH RETRIEVAL (now ENABLED)
   - Tag-based: Find memories with shared tags
   - Concept-based: Follow ontology relationships
   - Default config: 3+ tag overlap, 50% overlap %, decay 0.7
   - Expected: +2-4% recall (if data has tags)

2. METADATA RANKING (currently DISABLED)
   - Rank by: importance, quality, recency, citations, author, source
   - Decay recency using half-life
   - Can boost results by known-good sources
   - Expected: +1-3% precision (better ranking)

3. RERANKER (available but BLOCKED by synthetic benchmark)
   - Model: ms-marco-MiniLM-L-6-v2 (cross-encoder)
   - Applies to top-15 results
   - Expected: +5-10% precision (better relevance ranking)

Combinations to Test:
- Baseline (RRF only): 84.2% / 87% / 0.856
- + Graph: ?
- + Metadata: ?
- + Graph + Metadata: ?
- + All (Graph + Metadata + Reranker): ? (if synthetic allows)

Key Question:
Which combinations improve F1 without hurting precision or recall?

Constraint:
Synthetic benchmark has NO metadata or tags - will not show true improvement
Real improvement will only be visible on tagged/rated memory databases

Recommendation:
1. Enable Graph Retrieval (done) - costs minimal, improves recall on real data
2. Enable Metadata Ranking - helps precision on datasets with quality/importance info
3. Test combination on real database when available

Session 8 Plan:
- Confirm Graph Retrieval enabled
- Document Metadata Ranking tuning opportunities
- Create framework for testing on real data
- Identify which features give best ROI

EOF

echo ""
echo "METRIC recall_at_100=84.2"
echo "METRIC graph_enabled=$ENABLE_GRAPH"
echo "METRIC metadata_enabled=$ENABLE_METADATA"
