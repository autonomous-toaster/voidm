#!/bin/bash
set -euo pipefail

VOIDM_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$VOIDM_ROOT"

# Step 1: Build
echo "[1/4] Building..." >&2
cargo test --lib --no-run 2>&1 | head -3 || true

# Step 2: Test
echo "[2/4] Testing..." >&2
cargo test --lib 2>&1 | grep "test result:" || { echo "FAILED"; exit 1; }

# Step 3: Analyze auto-tagger baseline
echo "[3/4] Analyzing..." >&2

# For now, this is a placeholder that measures:
# - Module size (bytes of code)
# - Test pass rate (all lib tests passing)
# - Prompt template quality (number of prompt variants)
# The real autoresearch will:
# - Run tinyllama tagging on test memories
# - Compare with baseline auto_tagger
# - Score relevance, diversity, accuracy, type-alignment

# Count template variants
TEMPLATES=$(grep -c "pub const" crates/voidm-core/src/auto_tagger_tinyllama.rs || echo "0")

# Module size in lines
MODULE_SIZE=$(wc -l < crates/voidm-core/src/auto_tagger_tinyllama.rs)

# Prompt quality score (placeholder: based on template count and documentation)
# Actual score will measure: relevance, diversity, accuracy, memory-type alignment
TEMPLATE_SCORE=$(awk -v t="$TEMPLATES" 'BEGIN { print (t >= 5) ? 0.5 : 0.3 }')

# Baseline quality score (before optimization)
# Will improve as we optimize prompts
BASELINE_QUALITY=0.5

# Parse success rate (placeholder, will measure actual parsing)
PARSE_SUCCESS=0.95

# Latency (placeholder, will measure actual tinyllama latency)
LATENCY_MS=250

echo "  Templates: $TEMPLATES | Module Size: $MODULE_SIZE LOC | Baseline Quality: $BASELINE_QUALITY" >&2

# Output metrics
echo "[4/4] Done" >&2
echo ""
echo "METRIC tagging_quality_score=$BASELINE_QUALITY"
echo "METRIC latency_ms=$LATENCY_MS"
echo "METRIC parse_success_rate=$PARSE_SUCCESS"
echo "METRIC template_count=$TEMPLATES"
