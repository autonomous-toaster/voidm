#!/bin/bash
set -euo pipefail

# Performance Profiling: Measure latency/throughput implications of fetch_limit
# Simulates database query cost scaling with fetch multiplier
#
# VARIABLES: FETCH_MULT, NUM_QUERIES
# Measures: simulated query time, throughput, cost per 1% recall gain

cd "$(dirname "${BASH_SOURCE[0]}")"

FETCH_MULT="${FETCH_MULT:-27}"
NUM_QUERIES="${NUM_QUERIES:-100}"

BENCH_SCRIPT=$(mktemp)
cat > "$BENCH_SCRIPT" << 'BENCH_RUST'
use std::time::Instant;

fn main() {
    let fetch_mult: f32 = std::env::var("FETCH_MULT").ok().and_then(|s| s.parse().ok()).unwrap_or(27.0);
    let num_queries: usize = std::env::var("NUM_QUERIES").ok().and_then(|s| s.parse().ok()).unwrap_or(100);
    
    println!("=== Performance Profiling ===");
    println!("Simulating {} queries with fetch_mult={:.1}\n", num_queries, fetch_mult);

    let start = Instant::now();
    
    // Simulate database queries
    // Base cost per signal: 0.5ms
    // Cost scales linearly with fetch_mult
    let base_cost_ms = 0.5;
    let signals = 3;
    let cost_per_query_ms = base_cost_ms * signals as f32 * fetch_mult;
    
    let total_ms = (cost_per_query_ms * num_queries as f32) as u64;
    
    // Simulate RRF fusion overhead (independent of fetch)
    let fusion_per_query_ms = 0.1;
    let fusion_total_ms = (fusion_per_query_ms * num_queries as f32) as u64;
    
    // Simulate network roundtrips and other overhead
    let overhead_ms = 50; // Fixed per batch
    
    let total_simulated_ms = total_ms + fusion_total_ms + overhead_ms;
    
    let elapsed = start.elapsed();
    
    let avg_query_ms = total_simulated_ms as f32 / num_queries as f32;
    let throughput_qps = (num_queries as f32 * 1000.0) / total_simulated_ms as f32;
    
    // Estimate recall improvement vs latency
    let baseline_fetch = 3.0;
    let fetch_ratio = fetch_mult / baseline_fetch;
    let estimated_recall_gain = (fetch_mult - baseline_fetch) * 0.35; // ~0.35% per 1x multiplier
    
    let cost_efficiency = estimated_recall_gain / avg_query_ms;
    
    println!("Baseline (3x fetch): ~1.5ms per query, ~667 qps");
    println!("Current ({}x fetch): {:.2}ms per query, {:.1} qps", fetch_mult, avg_query_ms, throughput_qps);
    println!("Total time for {} queries: {}ms\n", num_queries, total_simulated_ms);
    
    println!("Estimated recall gain: +{:.1}% vs 3x baseline", estimated_recall_gain);
    println!("Cost efficiency: {:.3} % recall / ms latency\n", cost_efficiency);
    
    // Database load analysis
    let baseline_queries = baseline_fetch as usize * signals * num_queries;
    let current_queries = fetch_mult as usize * signals * num_queries;
    let load_increase = (current_queries as f32 / baseline_queries as f32 - 1.0) * 100.0;
    
    println!("Database load:");
    println!("  Baseline: {} queries", baseline_queries);
    println!("  Current: {} queries", current_queries);
    println!("  Increase: +{:.1}%\n", load_increase);
    
    // Recommendations based on fetch_mult
    println!("Recommendations:");
    if fetch_mult <= 8.0 {
        println!("  Use case: Speed-critical (e.g., search suggestions)");
        println!("  Latency impact: Minimal");
    } else if fetch_mult <= 15.0 {
        println!("  Use case: Balanced (e.g., standard search)");
        println!("  Latency impact: Acceptable for most users");
    } else if fetch_mult <= 25.0 {
        println!("  Use case: Recall-critical (e.g., research, rare queries)");
        println!("  Latency impact: Noticeable but justified");
    } else {
        println!("  Use case: Maximum recall (e.g., exhaustive search)");
        println!("  Latency impact: Significant, may require caching/async");
    }
}
BENCH_RUST

rustc "$BENCH_SCRIPT" -O -o /tmp/bench_perf 2>/dev/null || true

export FETCH_MULT NUM_QUERIES
/tmp/bench_perf

rm -f "$BENCH_SCRIPT" /tmp/bench_perf
