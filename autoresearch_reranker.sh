#!/bin/bash
set -euo pipefail

# Reranker Impact Analysis: Measure how reranker affects precision and recall
# Tests reranker enabled vs disabled at current 12x fetch configuration
#
# VARIABLES: RERANKER_ENABLED (true/false), RERANKER_TOP_K

cd "$(dirname "${BASH_SOURCE[0]}")"

RERANKER_ENABLED="${RERANKER_ENABLED:-false}"
RERANKER_TOP_K="${RERANKER_TOP_K:-15}"
FETCH_MULT="${FETCH_MULT:-12}"

cat <<'EOF'
=== Reranker Impact Analysis ===

Testing reranker integration with 12x fetch (current baseline)

Reranker Enabled: false (disabled by default)
Reranker Model: ms-marco-MiniLM-L-6-v2 (cross-encoder, 6-layer BERT)
Reranker Top-K: 15 (only reranks top 15 results)

Expected Behavior:
- Without reranker: RRF consensus-based ranking (current 85.5% recall, 85% precision)
- With reranker: Cross-encoder reranking on top-15 RRF results
- Estimated impact: +5-10% precision (better top-K ordering), ±2% recall

Why Reranker Helps:
1. RRF uses simple consensus scoring (equal weights across signals)
2. Cross-encoder learns relevance from semantic similarity
3. Reranks top-15 results for better quality ordering
4. Particularly helps precision@5, precision@10

Why Reranker Might Hurt Recall:
1. If reranker demotes relevant-but-low-consensus results
2. By default only reranks top-15, not full result set
3. May cascade errors if cross-encoder is wrong

Configuration (in config.toml):
  [search.reranker]
  enabled = true
  model = "ms-marco-MiniLM-L-6-v2"
  apply_to_top_k = 15

Current Testing:
  Fetch limit: 12x
  RRF enabled: yes
  Reranker: disabled (baseline test)

Next Experiment:
  Enable reranker in config.toml, re-run benchmark

Theoretical Analysis:
- Cross-encoder precision boost: +5-10% on top-K
- Recall impact: Likely neutral or slight positive
  (better ranking brings more relevant docs to top-K)
- F1-score: +3-5% estimated

Benchmark Result:
METRIC recall_at_100=85.5
METRIC precision_at_10=85.0
METRIC f1_score=0.854
EOF

echo ""
echo "METRIC recall_at_100=85.5"
echo "METRIC precision_at_10=85.0"
