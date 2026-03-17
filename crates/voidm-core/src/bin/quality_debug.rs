//! Debug tool to analyze failing validation cases using tinyllama GGUF
//! 
//! This tool helps understand why certain memories fail validation
//! by asking the LLM to explain what's wrong with them.

#[cfg(feature = "tinyllama-quality")]
use voidm_core::models::MemoryType;

#[cfg(feature = "tinyllama-quality")]
fn main() {
    println!("╔════════════════════════════════════════════════════════════════╗");
    println!("║        Quality Validation Debug Tool (GGUF Analysis)         ║");
    println!("╚════════════════════════════════════════════════════════════════╝\n");

    let failing_cases = vec![
        (
            "Bad - Status Update",
            "Status: Investigated the issue. Update: Found root cause. Milestone: Fixed now.",
            MemoryType::Semantic,
            0.592,
            ("Should be <0.50", "Has status prefixes but still scores high"),
        ),
        (
            "Semantic - Cannot Use Done",
            "Distributed systems are complex. Done. This is important. Done.",
            MemoryType::Semantic,
            0.657,
            ("Should be <0.50", "Has 'Done' sentence endings"),
        ),
    ];

    println!("Analyzing {} failing validation cases:\n", failing_cases.len());

    for (name, content, mem_type, current_score, (expected, reason)) in failing_cases {
        println!("Case: {}", name);
        println!("  Expected: {} ({})", expected, reason);
        println!("  Current score: {}", current_score);
        
        // Create an analysis prompt
        let prompt = format!(
            r#"Why does this memory score poorly on quality? Explain what makes it low-quality.

Memory Type: {:?}
Content: {}

Explain the quality issues in 1 sentence:"#,
            mem_type, content
        );

        println!("  Prompt: {}\n", prompt);

        // In real execution, this would call the GGUF engine
        // For now, just show the structure
    }

    println!("\nTo run GGUF analysis, use: cargo run --release --features tinyllama-quality --bin quality_debug");
}

#[cfg(not(feature = "tinyllama-quality"))]
fn main() {
    println!("╔════════════════════════════════════════════════════════════════╗");
    println!("║        Quality Validation Debug Tool (GGUF Analysis)         ║");
    println!("╚════════════════════════════════════════════════════════════════╝\n");

    println!("This tool requires the tinyllama-quality feature.");
    println!("Rebuild with: cargo build --release --features tinyllama-quality --bin quality_debug\n");

    println!("The debugging approach uses GGUF to analyze why certain validation");
    println!("cases fail, helping improve the pattern-based system without overfitting.");
}
