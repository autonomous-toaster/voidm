/// Quality Validation Benchmark
/// 
/// This tool validates that the quality scoring system correctly
/// identifies good and bad memories. It tests diverse scenarios
/// to ensure we're not overfitting to specific patterns.

use voidm_core::models::MemoryType;
use voidm_core::quality::compute_quality_score;

struct TestCase {
    name: &'static str,
    content: &'static str,
    memory_type: MemoryType,
    expected_score_range: (f32, f32), // (min, max)
}

fn main() {
    println!("╔════════════════════════════════════════════════════════════╗");
    println!("║      VOIDM QUALITY SCORING VALIDATION BENCHMARK            ║");
    println!("╚════════════════════════════════════════════════════════════╝");
    println!();

    let test_cases = vec![
        // === GOOD MEMORIES (should score > 0.70) ===
        TestCase {
            name: "Good Semantic - Generic Principle",
            content: "Service isolation prevents cascading failures in distributed systems. Proper circuit breakers and bulkheads are essential patterns. Always implement timeouts and fallback mechanisms.",
            memory_type: MemoryType::Semantic,
            expected_score_range: (0.70, 1.0),
        },
        TestCase {
            name: "Good Procedural - Clear Steps",
            content: "To deploy safely: (1) run tests locally, (2) build release, (3) verify checksums, (4) deploy to staging, (5) run smoke tests, (6) deploy to production. Never skip step 1.",
            memory_type: MemoryType::Procedural,
            expected_score_range: (0.65, 1.0),
        },
        TestCase {
            name: "Good Conceptual - Pattern Explanation",
            content: "The circuit breaker pattern has three states: Closed (normal operation), Open (failures detected, requests fail fast), Half-Open (testing if service recovered). Transition rules are critical for reliability.",
            memory_type: MemoryType::Conceptual,
            expected_score_range: (0.70, 1.0),
        },
        TestCase {
            name: "Good Episodic - Structured Event",
            content: "During Q1 system outage on 2024-01-15: Root cause was memory leak in cache layer. Recovery: upgraded cache version, cleared stale entries, restarted service. Prevention: added memory monitoring, implemented cache TTLs.",
            memory_type: MemoryType::Episodic,
            expected_score_range: (0.60, 0.95),
        },
        TestCase {
            name: "Good with Examples - Contextual",
            content: "PostgreSQL configuration: connection pooling essential for scalability. Example: pgbouncer limits concurrent connections (e.g., 100 pool, 10 reserve). Without pooling, 1000 connections → memory spike.",
            memory_type: MemoryType::Contextual,
            expected_score_range: (0.70, 1.0),
        },

        // === BAD MEMORIES (should score < 0.50) ===
        TestCase {
            name: "Bad - Task Log",
            content: "Today I fixed the bug. Completed the refactor. Done.",
            memory_type: MemoryType::Semantic,
            expected_score_range: (0.0, 0.50),
        },
        TestCase {
            name: "Bad - Status Update",
            content: "Status: In Progress. Update: Working on it. Milestone: 50% done.",
            memory_type: MemoryType::Semantic,
            expected_score_range: (0.0, 0.50),
        },
        TestCase {
            name: "Bad - Too Generic",
            content: "test",
            memory_type: MemoryType::Semantic,
            expected_score_range: (0.0, 0.30),
        },
        TestCase {
            name: "Bad - Personal Task",
            content: "I did the thing. We fixed the problem. My implementation works great.",
            memory_type: MemoryType::Semantic,
            expected_score_range: (0.0, 0.50),
        },
        TestCase {
            name: "Bad - Temporal Markers",
            content: "Today I worked on the API. Right now it's broken. This week we deployed. Currently fixing.",
            memory_type: MemoryType::Semantic,
            expected_score_range: (0.0, 0.50),
        },
        TestCase {
            name: "Bad - Very Repetitive",
            content: "test test test test test test test test test test test test test test test test test test test test",
            memory_type: MemoryType::Semantic,
            expected_score_range: (0.0, 0.40),
        },

        // === MIXED QUALITY (should score 0.40-0.70) ===
        TestCase {
            name: "Mixed - OK Quality",
            content: "When using Docker, remember that containers are ephemeral. You should mount volumes for persistent data. The image layer caching is important for build speed.",
            memory_type: MemoryType::Contextual,
            expected_score_range: (0.40, 0.75),
        },
        TestCase {
            name: "Mixed - Needs Work",
            content: "I learned about REST APIs today. They have endpoints and methods like GET and POST. This is important for web development.",
            memory_type: MemoryType::Semantic,
            expected_score_range: (0.30, 0.65),
        },

        // === MEMORY TYPE SPECIFIC ===
        TestCase {
            name: "Procedural - Can Use Done",
            content: "Run cargo build. Once done, run tests. When finished, commit changes. Finally done.",
            memory_type: MemoryType::Procedural,
            expected_score_range: (0.50, 0.95),
        },
        TestCase {
            name: "Semantic - Cannot Use Done",
            content: "Distributed systems are complex. Done. This is important. Done.",
            memory_type: MemoryType::Semantic,
            expected_score_range: (0.0, 0.50),
        },
    ];

    let mut passed = 0;
    let mut failed = 0;

    println!("Running {} test cases...", test_cases.len());
    println!();

    for test in &test_cases {
        let score = compute_quality_score(test.content, &test.memory_type);
        let actual_score = score.score;

        let in_range = actual_score >= test.expected_score_range.0
            && actual_score <= test.expected_score_range.1;

        if in_range {
            println!("✓ PASS: {}", test.name);
            println!("  Type: {}, Score: {:.3}, Expected: {:.2}-{:.2}",
                test.memory_type, actual_score,
                test.expected_score_range.0, test.expected_score_range.1);
            passed += 1;
        } else {
            println!("✗ FAIL: {}", test.name);
            println!("  Type: {}, Score: {:.3}, Expected: {:.2}-{:.2}",
                test.memory_type, actual_score,
                test.expected_score_range.0, test.expected_score_range.1);
            println!("  Breakdown:");
            println!("    - Genericity: {:.3}", score.genericity);
            println!("    - Abstraction: {:.3}", score.abstraction);
            println!("    - Temporal Indep: {:.3}", score.temporal_independence);
            println!("    - Task Indep: {:.3}", score.task_independence);
            println!("    - Substance: {:.3}", score.substance);
            println!("    - Entity Specificity: {:.3}", score.entity_specificity);
            failed += 1;
        }
        println!();
    }

    // Summary
    let total = passed + failed;
    let pass_rate = (passed as f32 / total as f32 * 100.0) as i32;

    println!("╔════════════════════════════════════════════════════════════╗");
    println!("║                   VALIDATION SUMMARY                       ║");
    println!("╚════════════════════════════════════════════════════════════╝");
    println!("Total Tests: {}", total);
    println!("Passed: {}", passed);
    println!("Failed: {}", failed);
    println!("Pass Rate: {}%", pass_rate);
    println!();

    if failed == 0 {
        println!("🎉 ALL TESTS PASSED - Quality scoring is working correctly!");
        println!("Good memories score high (>0.70), bad memories score low (<0.50)");
        println!();
        println!("METRIC validation_pass_rate={:.2}", 1.0);
        std::process::exit(0);
    } else {
        println!("❌ {} TESTS FAILED", failed);
        println!("Quality scoring may have issues. Review the failures above.");
        println!();
        println!("METRIC validation_pass_rate={:.2}", (passed as f32 / total as f32));
        std::process::exit(1);
    }
}
