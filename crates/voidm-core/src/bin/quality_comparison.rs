//! Compare GGUF-based vs pattern-based quality scoring
//! 
//! Run with:
//! - cargo run --release --bin quality_comparison
//! - cargo run --release --features tinyllama-quality --bin quality_comparison

use voidm_core::models::MemoryType;

fn main() {
    println!("╔════════════════════════════════════════════════════════════════╗");
    println!("║      QUALITY COMPARISON: GGUF vs Pattern-Based Scoring       ║");
    println!("╚════════════════════════════════════════════════════════════════╝\n");

    let test_cases = vec![
        (
            "Good Semantic - Generic Principle",
            "REST APIs use stateless communication. Clients send requests, servers respond. This simplifies scaling.",
            MemoryType::Semantic,
        ),
        (
            "Bad - Status Update",
            "Status: Investigated the issue. Update: Found root cause. Milestone: Fixed now.",
            MemoryType::Semantic,
        ),
        (
            "Bad - Single Word",
            "test",
            MemoryType::Semantic,
        ),
        (
            "Bad - Very Repetitive",
            "test test test test test test test test test test test test test test test test test test test test",
            MemoryType::Semantic,
        ),
        (
            "Good Procedural - Clear Steps",
            "To implement caching: 1) Set TTL header. 2) Validate before use. 3) Invalidate on mutation. 4) Monitor cache hits.",
            MemoryType::Procedural,
        ),
    ];

    #[cfg(feature = "tinyllama-quality")]
    {
        println!("[GGUF FEATURE ENABLED]\n");
        println!("Testing GGUF-based quality extraction with {} test cases:\n", test_cases.len());
        
        for (name, content, mem_type) in &test_cases {
            println!("Testing: {}", name);
            
            match voidm_core::tinyllama_quality::extract_quality_features(content, mem_type) {
                Ok(features) => {
                    let score = voidm_core::tinyllama_quality::compute_score_from_features(&features);
                    println!("  ✓ GGUF Score: {:.3}", score);
                    println!("    - Genericity: {:.2}", features.genericity);
                    println!("    - Abstraction: {:.2}", features.abstraction);
                    println!("    - Temporal Independence: {:.2}", features.temporal_independence);
                    println!("    - Task Independence: {:.2}", features.task_independence);
                    println!("    - Substance: {:.2}", features.substance);
                    println!("    - Entity Specificity: {:.2}", features.entity_specificity);
                    println!("    - Reasoning: {}\n", features.reasoning);
                }
                Err(e) => {
                    println!("  ✗ GGUF Error: {}\n", e);
                }
            }
        }
    }

    #[cfg(not(feature = "tinyllama-quality"))]
    {
        println!("[GGUF FEATURE DISABLED - using pattern-based scoring]\n");
        println!("To test GGUF-based scoring, rebuild with: --features tinyllama-quality\n");
    }

    println!("Comparison complete!");
}
