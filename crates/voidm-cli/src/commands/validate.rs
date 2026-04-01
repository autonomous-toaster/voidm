use anyhow::Result;
use clap::Args;
use voidm_db::Database;
use voidm_core::chunking::ChunkingStrategy;
use voidm_core::coherence::estimate_coherence;
use std::time::Instant;
use tracing::{info, warn, debug};

#[derive(Args)]
pub struct ValidationArgs {
    /// Number of memories to test
    #[arg(long, default_value = "10")]
    pub limit: usize,

    /// Minimum content length to test (chars)
    #[arg(long, default_value = "1000")]
    pub min_length: usize,
}

pub async fn run(args: ValidationArgs, db: &std::sync::Arc<dyn Database>) -> Result<()> {
    info!("═══════════════════════════════════════════════════════════════════");
    info!("PHASE A VALIDATION: Smart Chunking on Real Data");
    info!("═══════════════════════════════════════════════════════════════════");

    // Load memories for validation
    let memories_raw = db.fetch_memories_for_chunking(args.limit).await?;

    if memories_raw.is_empty() {
        warn!("No memories found to validate");
        return Ok(());
    }

    info!("Loaded {} memories from backend", memories_raw.len());
    info!("───────────────────────────────────────────────────────────────────");

    let strategy = ChunkingStrategy {
        target_size: voidm_core::memory_policy::CHUNK_TARGET_SIZE,
        min_chunk_size: voidm_core::memory_policy::CHUNK_MIN_SIZE,
        max_chunk_size: voidm_core::memory_policy::CHUNK_MAX_SIZE,
        overlap: voidm_core::memory_policy::CHUNK_OVERLAP,
        smart_breaks: true,
    };
    let mut smart_stats = Vec::new();
    let mut total_smart_coherence = 0.0;
    let mut total_chunks = 0usize;
    let mut total_time = std::time::Duration::ZERO;

    for (idx, (id, content, created_at)) in memories_raw.iter().enumerate() {
        let preview = content.chars().take(60).collect::<String>().replace('\n', " ");
        debug!("[Memory {}] {} (Size: {} chars)", idx + 1, id, content.len());
        debug!("  Preview: {}...", preview);

        // Smart chunking with timing
        let start = Instant::now();
        let chunks = voidm_core::embeddings::chunk_memory(&id, &content, &created_at, &strategy);
        let elapsed = start.elapsed();
        total_time += elapsed;

        info!("[Memory {}] Smart chunking: {} chunks ({:.1}ms)", 
            idx + 1, chunks.len(), elapsed.as_secs_f32() * 1000.0);

        let mut memory_coherence = 0.0;

        for (chunk_idx, chunk) in chunks.iter().enumerate() {
            let score = estimate_coherence(&chunk.content);
            let final_score = score.final_score();
            memory_coherence += final_score;

            let level = score.quality_level();
            debug!("  Chunk {}: {} chars, coherence {:.2} {}", 
                chunk_idx, chunk.size, final_score, level);
        }

        let avg_coherence = if chunks.is_empty() { 
            0.0 
        } else { 
            memory_coherence / chunks.len() as f32 
        };

        info!("[Memory {}] Avg coherence: {:.2}", idx + 1, avg_coherence);
        smart_stats.push((id.clone(), avg_coherence, chunks.len()));
        total_smart_coherence += avg_coherence;
        total_chunks += chunks.len();

        // Alert if low coherence
        if avg_coherence < 0.75 {
            warn!("[Memory {}] Low coherence (< 0.75)", idx + 1);
        }
    }

    // Summary
    info!("───────────────────────────────────────────────────────────────────");
    info!("SUMMARY STATISTICS");

    let avg_smart_coherence = if smart_stats.is_empty() { 
        0.0 
    } else { 
        total_smart_coherence / smart_stats.len() as f32 
    };

    info!("Total memories tested: {}", smart_stats.len());
    info!("Total chunks created: {}", total_chunks);
    info!("Avg chunks per memory: {:.1}", 
        if smart_stats.is_empty() { 0.0 } else { total_chunks as f32 / smart_stats.len() as f32 });
    info!("Average coherence: {:.2}", avg_smart_coherence);
    info!("Total processing time: {:.1}ms", total_time.as_secs_f32() * 1000.0);
    info!("Avg time per memory: {:.1}ms", 
        if smart_stats.is_empty() { 0.0 } else { total_time.as_secs_f32() * 1000.0 / smart_stats.len() as f32 });

    // Count by quality level
    let excellent = smart_stats.iter().filter(|(_, c, _)| *c >= 0.8).count();
    let good = smart_stats.iter().filter(|(_, c, _)| *c >= 0.6 && *c < 0.8).count();
    let fair = smart_stats.iter().filter(|(_, c, _)| *c >= 0.3 && *c < 0.6).count();
    let poor = smart_stats.iter().filter(|(_, c, _)| *c < 0.3).count();

    info!("Quality distribution:");
    info!("  EXCELLENT (0.80+): {} ({:.0}%)", excellent, excellent as f32 / smart_stats.len() as f32 * 100.0);
    info!("  GOOD (0.60-0.79): {} ({:.0}%)", good, good as f32 / smart_stats.len() as f32 * 100.0);
    info!("  FAIR (0.30-0.59): {} ({:.0}%)", fair, fair as f32 / smart_stats.len() as f32 * 100.0);
    info!("  POOR (<0.30): {} ({:.0}%)", poor, poor as f32 / smart_stats.len() as f32 * 100.0);

    info!("───────────────────────────────────────────────────────────────────");

    // Validation result
    if avg_smart_coherence >= 0.75 {
        info!("VALIDATION PASSED");
        info!("Average coherence {:.2} meets target of 0.75+", avg_smart_coherence);
        info!("Algorithm is ready for Part D (chunking 900 memories)");
    } else if avg_smart_coherence >= 0.60 {
        warn!("VALIDATION MARGINAL");
        warn!("Average coherence {:.2} below target of 0.75", avg_smart_coherence);
        warn!("Algorithm works but quality is mediocre");
        warn!("Consider: adjust parameters or add special content handling");
    } else {
        warn!("VALIDATION FAILED");
        warn!("Average coherence {:.2} is too low", avg_smart_coherence);
        warn!("Algorithm needs improvement before Part D");
    }

    info!("───────────────────────────────────────────────────────────────────");

    Ok(())
}
