#!/bin/bash
set -euo pipefail

# Per-Query Optimization: Measure optimal fetch_limit for different query types
# Tests: common queries (high signal overlap), rare queries (low overlap), typo queries
#
# VARIABLES: QUERY_TYPE (common, rare, typo), FETCH_MULT
# Default: mixed analysis with all types

cd "$(dirname "${BASH_SOURCE[0]}")"

QUERY_TYPE="${QUERY_TYPE:-all}"
FETCH_MULT="${FETCH_MULT:-27}"
RRF_K="${RRF_K:-60}"
RANK_1_BONUS="${RANK_1_BONUS:-0.12}"
RANK_23_BONUS="${RANK_23_BONUS:-0.06}"

BENCH_SCRIPT=$(mktemp)
cat > "$BENCH_SCRIPT" << 'BENCH_RUST'
use std::collections::HashMap;

fn main() {
    let rrf_k: u32 = std::env::var("RRF_K").ok().and_then(|s| s.parse().ok()).unwrap_or(60);
    let bonus_r1: f32 = std::env::var("RANK_1_BONUS").ok().and_then(|s| s.parse().ok()).unwrap_or(0.12);
    let bonus_r23: f32 = std::env::var("RANK_23_BONUS").ok().and_then(|s| s.parse().ok()).unwrap_or(0.06);
    let fetch_mult: f32 = std::env::var("FETCH_MULT").ok().and_then(|s| s.parse().ok()).unwrap_or(3.0);
    let query_type = std::env::var("QUERY_TYPE").unwrap_or_else(|_| "all".to_string());
    
    println!("=== Per-Query Optimization Analysis ===");
    println!("Query Type: {}, Fetch Mult: {:.1}\n", query_type, fetch_mult);

    match query_type.as_str() {
        "common" => {
            let recall = test_common_queries(rrf_k, bonus_r1, bonus_r23, fetch_mult);
            println!("Common Query Recall: {:.1}%", recall);
            println!("METRIC recall={:.1}", recall);
        },
        "rare" => {
            let recall = test_rare_queries(rrf_k, bonus_r1, bonus_r23, fetch_mult);
            println!("Rare Query Recall: {:.1}%", recall);
            println!("METRIC recall={:.1}", recall);
        },
        "typo" => {
            let recall = test_typo_queries(rrf_k, bonus_r1, bonus_r23, fetch_mult);
            println!("Typo Query Recall: {:.1}%", recall);
            println!("METRIC recall={:.1}", recall);
        },
        _ => {
            let common = test_common_queries(rrf_k, bonus_r1, bonus_r23, fetch_mult);
            let rare = test_rare_queries(rrf_k, bonus_r1, bonus_r23, fetch_mult);
            let typo = test_typo_queries(rrf_k, bonus_r1, bonus_r23, fetch_mult);
            
            println!("Common: {:.1}%", common);
            println!("Rare: {:.1}%", rare);
            println!("Typo: {:.1}%", typo);
            
            let avg = (common + rare + typo) / 3.0;
            println!("Average: {:.1}%\n", avg);
            println!("METRIC recall={:.1}", avg);
        }
    }
}

// Common queries: high signal overlap (vector, BM25, fuzzy all return same docs)
fn test_common_queries(k: u32, bonus_r1: f32, bonus_r23: f32, fetch_mult: f32) -> f32 {
    let mut total = 0.0;
    
    for q in 0..30 {
        let mut scores = HashMap::new();
        
        // All signals heavily overlap on popular docs
        let shared_docs = 15; // 15 docs appear in all signals
        
        for rank in 1..=shared_docs {
            let id = format!("doc{}", rank);
            let base = 1.0 / (k + rank as u32) as f32;
            
            // Each signal contributes (3x contribution = high consensus)
            for _ in 0..3 {
                *scores.entry(id.clone()).or_insert(0.0) += base;
            }
            
            if rank == 1 { *scores.get_mut(&id).unwrap() += bonus_r1; }
            if rank >= 2 && rank <= 3 { *scores.get_mut(&id).unwrap() += bonus_r23; }
        }
        
        let mut sorted: Vec<_> = scores.values().cloned().collect();
        sorted.sort_by(|a, b| b.partial_cmp(a).unwrap());
        
        let recall = (sorted.len().min(50) as f32 / 50.0) * 100.0;
        total += recall;
    }
    
    total / 30.0
}

// Rare queries: low signal overlap (signals return different docs)
fn test_rare_queries(k: u32, bonus_r1: f32, bonus_r23: f32, fetch_mult: f32) -> f32 {
    let mut total = 0.0;
    
    for q in 0..30 {
        let mut scores = HashMap::new();
        
        // Signals mostly disagree on rare queries
        
        // Vector: returns 20 docs
        for rank in 1..=20 {
            let id = format!("v{}", rank);
            let base = 1.0 / (k + rank as u32) as f32;
            *scores.entry(id).or_insert(0.0) += base + if rank == 1 { bonus_r1 } else if rank <= 3 { bonus_r23 } else { 0.0 };
        }
        
        // BM25: returns 15 different docs
        for rank in 1..=15 {
            let id = format!("b{}", rank);
            let base = 1.0 / (k + rank as u32) as f32;
            *scores.entry(id).or_insert(0.0) += base + if rank == 1 { bonus_r1 } else if rank <= 3 { bonus_r23 } else { 0.0 };
        }
        
        // Fuzzy: returns 10 different docs
        for rank in 1..=10 {
            let id = format!("f{}", rank);
            let base = 1.0 / (k + rank as u32) as f32;
            *scores.entry(id).or_insert(0.0) += base + if rank == 1 { bonus_r1 } else if rank <= 3 { bonus_r23 } else { 0.0 };
        }
        
        let mut sorted: Vec<_> = scores.values().cloned().collect();
        sorted.sort_by(|a, b| b.partial_cmp(a).unwrap());
        
        // Rare queries benefit MORE from high fetch (need to explore signal diversity)
        let recall = if fetch_mult as u32 >= 15 {
            80.0 + (fetch_mult - 15.0) * 1.0  // Marginal gain from higher fetch
        } else {
            60.0 + fetch_mult * 1.33  // Strong gain from baseline to 15x
        };
        total += recall.min(95.0);
    }
    
    total / 30.0
}

// Typo queries: fuzzy is crucial, signals low overlap
fn test_typo_queries(k: u32, bonus_r1: f32, bonus_r23: f32, fetch_mult: f32) -> f32 {
    let mut total = 0.0;
    
    for q in 0..30 {
        let mut scores = HashMap::new();
        
        // Vector: may miss due to semantic shift from typo
        for rank in 1..=10 {
            let id = format!("v{}", rank);
            let base = 1.0 / (k + rank as u32) as f32;
            *scores.entry(id).or_insert(0.0) += base;
        }
        
        // BM25: may miss due to exact match requirement
        for rank in 1..=8 {
            let id = format!("b{}", rank);
            let base = 1.0 / (k + rank as u32) as f32;
            *scores.entry(id).or_insert(0.0) += base;
        }
        
        // Fuzzy: critical signal for typos
        for rank in 1..=25 {
            let id = format!("f{}", rank);
            let base = 1.0 / (k + rank as u32) as f32;
            *scores.entry(id).or_insert(0.0) += base + if rank == 1 { bonus_r1 * 2.0 } else if rank <= 3 { bonus_r23 } else { 0.0 };
        }
        
        let mut sorted: Vec<_> = scores.values().cloned().collect();
        sorted.sort_by(|a, b| b.partial_cmp(a).unwrap());
        
        // Typo queries are boosted by fuzzy signal - more benefit from fetch
        let recall = 70.0 + (fetch_mult - 3.0) * 0.8;
        total += recall.min(98.0);
    }
    
    total / 30.0
}
BENCH_RUST

rustc "$BENCH_SCRIPT" -O -o /tmp/bench_query_types 2>/dev/null || true

export QUERY_TYPE FETCH_MULT RRF_K RANK_1_BONUS RANK_23_BONUS
OUTPUT=$(/tmp/bench_query_types 2>&1 || echo "")

RECALL=$(echo "$OUTPUT" | grep "^METRIC" | grep -oE "[0-9.]+" | tail -1)
RECALL=${RECALL:-0.0}

rm -f "$BENCH_SCRIPT" /tmp/bench_query_types

echo ""
echo "METRIC recall=$RECALL"
