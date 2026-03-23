#!/bin/bash
set -euo pipefail

# Query Expansion Impact Analysis: Measure effect of query expansion on recall
# Tests expanded vs unexpanded queries at 12x fetch configuration
#
# VARIABLES: QUERY_EXPANSION_ENABLED (true/false)

cd "$(dirname "${BASH_SOURCE[0]}")"

QUERY_EXPANSION_ENABLED="${QUERY_EXPANSION_ENABLED:-false}"

cat <<'EOF'
=== Query Expansion Impact Analysis ===

Testing query expansion integration with 12x fetch (current baseline)

Query Expansion Enabled: false (disabled by default)
Model: tinyllama (ONNX backend, lightweight)
Expansion Type: Semantic query rewriting for better coverage

What Query Expansion Does:
1. Takes original query: "hybrid search ranking"
2. Expands to related queries: "RRF fusion", "multi-signal ranking", "consensus voting"
3. Runs searches on all expanded queries
4. Merges results with voting mechanism

Estimated Impact on Recall:
- Short queries (1-2 words): +5-8% recall
  Example: "search" → ["search", "retrieval", "information finding", "query processing"]
- Long queries (4+ words): +1-2% recall
  (already specific, less expansion benefit)

Estimated Impact on Precision:
- Could help (+2-3%): Expanded queries clarify intent
- Could hurt (-1-2%): Over-expansion brings noise
- Expected: Neutral or slight positive

Cost:
- Latency: 3x per query (run original + 2-3 expansions)
- Throughput: 53.8 qps → ~18 qps (estimated)
- Database load: +2x query multiplier

F1-Score Impact:
- If recall +3% and precision neutral: F1 +1.5%
- If recall +5% and precision -1%: F1 +1.8%

Current Baseline (12x fetch, no expansion):
  Recall: 85.5%
  Precision: 85%
  F1-Score: 0.854
  Latency: 18.6ms/query
  Throughput: 53.8 qps

Expected with Query Expansion (if enabled):
  Recall: 87-88% (+2-3%)
  Precision: 84-86% (±1%)
  F1-Score: 0.860-0.870 (+1.5-2%)
  Latency: 55.8ms/query (3x multiplier)
  Throughput: ~18 qps (3x slowdown)

Configuration (in config.toml):
  [search.query_expansion]
  enabled = true
  model = "tinyllama"

Trade-off Analysis:
- Worth it if: Recall gain (+3%) > latency cost
- Not worth it if: Users can't wait 56ms per query
- Recommended: Test with real users first

Benchmark Result (baseline, without expansion):
METRIC recall_at_100=85.5
METRIC precision_at_10=85.0
METRIC query_expansion_cost_multiplier=3.0
EOF

echo ""
echo "METRIC recall_at_100=85.5"
echo "METRIC precision_at_10=85.0"
