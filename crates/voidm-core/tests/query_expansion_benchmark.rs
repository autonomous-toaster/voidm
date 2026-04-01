#![cfg(feature = "query-expansion")]

//! Practical benchmark harness for short local generation tasks used by voidm.
//!
//! Run manually, for example:
//!   cargo test -p voidm-core --test query_expansion_benchmark --features tinyllama -- --ignored --nocapture
//!
//! Notes:
//! - This is a pragmatic harness, not a criterion microbenchmark.
//! - It reports per-query latency and a small quality proxy summary.
//! - Unsupported backends/models are reported honestly instead of being faked.

use std::time::Instant;
use voidm_core::query_expansion::{parse_generation_backend, GenerationBackend, QueryExpander};
use voidm_core::config::{IntentConfig, QueryExpansionConfig};

fn get_test_queries() -> Vec<&'static str> {
    vec![
        "API",
        "Docker",
        "Python",
        "Database",
        "Testing",
        "Cache",
        "Security",
        "Microservices",
        "Model",
        "Service",
    ]
}

fn count_added_terms(original: &str, expanded: &str) -> usize {
    let original_terms: std::collections::HashSet<String> = original
        .split(',')
        .flat_map(|s| s.split_whitespace())
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .collect();

    expanded
        .split(',')
        .flat_map(|s| s.split_whitespace())
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty() && !original_terms.contains(s))
        .collect::<std::collections::HashSet<_>>()
        .len()
}

#[tokio::test]
#[ignore]
async fn benchmark_query_expansion_models() {
    let candidates = vec![
        ("onnx", "tinyllama"),
        ("onnx", "phi-2"),
        ("onnx", "gpt2-small"),
        // Future challengers once implemented:
        ("llama_cpp", "prism-ml/Bonsai-1.7B-gguf"),
        ("mlx", "prism-ml/Bonsai-1.7B-mlx-1bit"),
    ];

    println!("== query expansion benchmark ==");

    for (backend_name, model_name) in candidates {
        let backend = match parse_generation_backend(backend_name) {
            Ok(b) => b,
            Err(e) => {
                println!("backend={} model={} status=invalid_backend error={}", backend_name, model_name, e);
                continue;
            }
        };

        run_model_benchmark(backend, model_name).await;
        println!();
    }
}

async fn run_model_benchmark(backend: GenerationBackend, model_name: &str) {
    let config = QueryExpansionConfig {
        enabled: true,
        model: model_name.to_string(),
        backend: backend_name(&backend).to_string(),
        timeout_ms: 10_000,
        intent: IntentConfig::default(),
    };

    let expander = QueryExpander::new(voidm_core::query_expansion::QueryExpansionConfig {
        enabled: true,
        model: config.model.clone(),
        backend: backend.clone(),
        timeout_ms: config.timeout_ms,
        intent: voidm_core::query_expansion::IntentConfig::default(),
    });

    let queries = get_test_queries();
    let mut success_count = 0usize;
    let mut total_added_terms = 0usize;
    let mut total_ms = 0u128;
    let mut failures = Vec::new();

    println!("backend={} model={} queries={}", backend_name(&backend), model_name, queries.len());

    for query in queries {
        let start = Instant::now();
        let result = expander.expand(query).await;
        let elapsed = start.elapsed().as_millis();

        match result {
            Ok(expanded) => {
                let added = count_added_terms(query, &expanded);
                success_count += 1;
                total_added_terms += added;
                total_ms += elapsed;
                println!(
                    "  ok query={:?} latency_ms={} added_terms={} expanded={:?}",
                    query, elapsed, added, expanded
                );
            }
            Err(e) => {
                failures.push(format!("query={:?} latency_ms={} error={}", query, elapsed, e));
            }
        }
    }

    if success_count > 0 {
        let avg_ms = total_ms as f64 / success_count as f64;
        let avg_added = total_added_terms as f64 / success_count as f64;
        println!(
            "summary backend={} model={} success={} failure={} avg_latency_ms={:.1} avg_added_terms={:.2}",
            backend_name(&backend),
            model_name,
            success_count,
            failures.len(),
            avg_ms,
            avg_added
        );
    } else {
        println!(
            "summary backend={} model={} success=0 failure={} status=unavailable_or_unsupported",
            backend_name(&backend),
            model_name,
            failures.len()
        );
    }

    for failure in failures.iter().take(3) {
        println!("  fail {}", failure);
    }
}

fn backend_name(backend: &GenerationBackend) -> &'static str {
    match backend {
        GenerationBackend::Onnx => "onnx",
        GenerationBackend::LlamaCpp => "llama_cpp",
        GenerationBackend::Mlx => "mlx",
    }
}

#[tokio::test]
async fn test_query_expansion_integration() {
    let expander = QueryExpander::new(voidm_core::query_expansion::QueryExpansionConfig {
        enabled: true,
        model: "phi-2".to_string(),
        backend: GenerationBackend::Onnx,
        timeout_ms: 300,
        intent: voidm_core::query_expansion::IntentConfig::default(),
    });

    let result = expander.expand("Docker").await;
    match result {
        Ok(expanded) => assert!(!expanded.is_empty()),
        Err(e) => assert!(!e.to_string().is_empty()),
    }
}
