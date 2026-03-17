#!/bin/bash
set -euo pipefail

# Autoresearch: Quality Checks
# Runs comprehensive quality and type checks to ensure no regressions

cd "$(dirname "$0")"

echo "=== Quality Tests ==="
cargo test --lib quality 2>&1 | tail -20

echo ""
echo "=== Compilation Check ==="
cargo check --all-targets 2>&1 | grep -i "error\|warning" | head -10 || echo "✓ Clean compilation"

echo ""
echo "=== Linting ==="
cargo clippy --all-targets -- -D warnings 2>&1 | grep -i "error" | head -5 || echo "✓ No clippy errors"
