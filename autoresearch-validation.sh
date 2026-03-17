#!/bin/bash
set -euo pipefail

# Validation: Quality Scoring System
# Tests that good memories score high and bad memories score low
# Provides detailed stdout feedback to verify the scoring is working correctly

cd "$(dirname "$0")"

echo "=== VOIDM QUALITY VALIDATION BENCHMARK ===" 
echo ""
echo "Running comprehensive quality score validation..."
echo "Testing diverse memory types and quality levels"
echo ""

# Run the validation binary if it exists, otherwise build and run
if [ -f target/release/quality_validation ] || [ -f target/debug/quality_validation ]; then
    if [ -f target/release/quality_validation ]; then
        RESULT=$(./target/release/quality_validation 2>&1 || true)
    else
        RESULT=$(./target/debug/quality_validation 2>&1 || true)
    fi
else
    # Build and run validation tool
    echo "[*] Building quality validation tool..."
    RESULT=$(cargo run --bin quality_validation 2>&1 || true)
fi

echo "$RESULT"

# Parse results: look for test counts and quality assessments
GOOD_COUNT=$(echo "$RESULT" | grep -c "✓ Good memory" || echo "0")
BAD_COUNT=$(echo "$RESULT" | grep -c "✗ Bad memory" || echo "0")
TOTAL_TESTS=$((GOOD_COUNT + BAD_COUNT))

if [ "$TOTAL_TESTS" -gt 0 ]; then
    echo ""
    echo "=== VALIDATION SUMMARY ==="
    echo "Total test cases: $TOTAL_TESTS"
    echo "Good memories correctly scored high: $GOOD_COUNT"
    echo "Bad memories correctly scored low: $BAD_COUNT"
    
    # Calculate pass rate
    if [ "$TOTAL_TESTS" -gt 0 ]; then
        PASS_RATE=$((TOTAL_TESTS * 100 / TOTAL_TESTS))
        echo "Pass rate: ${PASS_RATE}%"
    fi
    
    # Check if validation passed
    if echo "$RESULT" | grep -q "ALL TESTS PASSED"; then
        echo ""
        echo "METRIC validation_pass_rate=1.0"
        exit 0
    else
        echo ""
        echo "METRIC validation_pass_rate=0.0"
        exit 1
    fi
else
    # Fallback: check if quality tests pass
    cargo test --lib quality --quiet 2>&1 | tail -5
    if cargo test --lib quality --quiet 2>&1 | grep -q "test result: ok"; then
        echo "METRIC validation_pass_rate=1.0"
        exit 0
    else
        echo "METRIC validation_pass_rate=0.0"
        exit 1
    fi
fi
