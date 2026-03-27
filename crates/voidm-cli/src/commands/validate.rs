use anyhow::Result;
use clap::Args;
use sqlx::SqlitePool;
use voidm_core::chunking::{chunk_smart, ChunkingStrategy};
use voidm_core::coherence::estimate_coherence;
use std::time::Instant;

#[derive(Args)]
pub struct ValidationArgs {
    /// Number of memories to test
    #[arg(long, default_value = "10")]
    pub limit: usize,

    /// Minimum content length to test (chars)
    #[arg(long, default_value = "1000")]
    pub min_length: usize,
}

pub async fn run(args: ValidationArgs, pool: &SqlitePool) -> Result<()> {
    println!("\n╔════════════════════════════════════════════════════════════════╗");
    println!("║  PHASE A VALIDATION: Smart Chunking on Real Data               ║");
    println!("╚════════════════════════════════════════════════════════════════╝\n");

    // Load real memories
    let memories: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT id, content, type FROM memories \
         WHERE LENGTH(content) > ? \
         ORDER BY RANDOM() LIMIT ?"
    )
    .bind(args.min_length as i32)
    .bind(args.limit as i32)
    .fetch_all(pool)
    .await?;

    if memories.is_empty() {
        println!("⚠️  No memories found with content > {} chars", args.min_length);
        return Ok(());
    }

    println!("Loaded {} real memories from SQLite", memories.len());
    println!("{}\n", "=".repeat(70));

    let strategy = ChunkingStrategy::default();
    let mut smart_stats = Vec::new();
    let mut total_smart_coherence = 0.0;
    let mut total_chunks = 0usize;
    let mut total_time = std::time::Duration::ZERO;

    for (idx, (id, content, memory_type)) in memories.iter().enumerate() {
        println!("\n[Memory {}] {}", idx + 1, id);
        println!("  Type: {}", memory_type);
        println!("  Size: {} chars", content.len());
        println!("  Preview: {}...", 
            &content[..60.min(content.len())].replace('\n', " "));

        // Smart chunking with timing
        let start = Instant::now();
        match chunk_smart(&id, &content, &strategy) {
            Ok(chunks) => {
                let elapsed = start.elapsed();
                total_time += elapsed;
                
                println!("  ✅ Smart chunking: {} chunks ({:.1}ms)", chunks.len(), elapsed.as_secs_f32() * 1000.0);
                
                let mut memory_coherence = 0.0;

                for (chunk_idx, chunk) in chunks.iter().enumerate() {
                    let score = estimate_coherence(&chunk.content);
                    let final_score = score.final_score();
                    memory_coherence += final_score;
                    
                    let level = score.quality_level();
                    println!("    Chunk {}: {} chars, coherence {:.2} {}", 
                        chunk_idx, chunk.size, final_score, level);
                }

                let avg_coherence = if chunks.is_empty() { 
                    0.0 
                } else { 
                    memory_coherence / chunks.len() as f32 
                };

                println!("  Avg coherence: {:.2}", avg_coherence);
                smart_stats.push((id.clone(), avg_coherence, chunks.len()));
                total_smart_coherence += avg_coherence;
                total_chunks += chunks.len();

                // Alert if low coherence
                if avg_coherence < 0.75 {
                    println!("  ⚠️  WARNING: Low coherence (< 0.75)");
                }
            }
            Err(e) => {
                println!("  ❌ Smart chunking failed: {}", e);
            }
        }
    }

    // Summary
    println!("\n{}", "=".repeat(70));
    println!("\n📊 SUMMARY STATISTICS\n");

    let avg_smart_coherence = if smart_stats.is_empty() { 
        0.0 
    } else { 
        total_smart_coherence / smart_stats.len() as f32 
    };

    println!("Total memories tested: {}", smart_stats.len());
    println!("Total chunks created: {}", total_chunks);
    println!("Avg chunks per memory: {:.1}", 
        if smart_stats.is_empty() { 0.0 } else { total_chunks as f32 / smart_stats.len() as f32 });
    println!("Average coherence: {:.2}", avg_smart_coherence);
    println!("Total processing time: {:.1}ms", total_time.as_secs_f32() * 1000.0);
    println!("Avg time per memory: {:.1}ms", 
        if smart_stats.is_empty() { 0.0 } else { total_time.as_secs_f32() * 1000.0 / smart_stats.len() as f32 });

    // Count by quality level
    let excellent = smart_stats.iter().filter(|(_, c, _)| *c >= 0.8).count();
    let good = smart_stats.iter().filter(|(_, c, _)| *c >= 0.6 && *c < 0.8).count();
    let fair = smart_stats.iter().filter(|(_, c, _)| *c >= 0.3 && *c < 0.6).count();
    let poor = smart_stats.iter().filter(|(_, c, _)| *c < 0.3).count();

    println!("\nQuality distribution:");
    println!("  🟣 EXCELLENT (0.80+): {} ({:.0}%)", excellent, excellent as f32 / smart_stats.len() as f32 * 100.0);
    println!("  🟢 GOOD (0.60-0.79): {} ({:.0}%)", good, good as f32 / smart_stats.len() as f32 * 100.0);
    println!("  🟡 FAIR (0.30-0.59): {} ({:.0}%)", fair, fair as f32 / smart_stats.len() as f32 * 100.0);
    println!("  🔴 POOR (<0.30): {} ({:.0}%)", poor, poor as f32 / smart_stats.len() as f32 * 100.0);

    println!("\n{}", "=".repeat(70));

    // Validation result
    if avg_smart_coherence >= 0.75 {
        println!("\n✅ VALIDATION PASSED");
        println!("Average coherence {:.2} meets target of 0.75+", avg_smart_coherence);
        println!("Algorithm is ready for Part D (chunking 900 memories)");
    } else if avg_smart_coherence >= 0.60 {
        println!("\n⚠️  VALIDATION MARGINAL");
        println!("Average coherence {:.2} below target of 0.75", avg_smart_coherence);
        println!("Algorithm works but quality is mediocre");
        println!("Consider: adjust parameters or add special content handling");
    } else {
        println!("\n❌ VALIDATION FAILED");
        println!("Average coherence {:.2} is too low", avg_smart_coherence);
        println!("Algorithm needs improvement before Part D");
    }

    println!("\n{}", "=".repeat(70));

    Ok(())
}
