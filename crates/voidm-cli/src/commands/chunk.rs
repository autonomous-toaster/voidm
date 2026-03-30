/// Part D: Chunking CLI Command
///
/// This command chunks memories from SQLite and stores them in Neo4j.
///
/// Usage:
/// ```bash
/// voidm chunk --limit 10 --neo4j-url bolt://localhost:7687
/// voidm chunk --all --neo4j-url bolt://localhost:7687
/// ```

use anyhow::Result;
use clap::Args;
use voidm_db::Database;
use tracing::{info, warn, debug};
use std::time::Instant;

#[derive(Args)]
pub struct ChunkArgs {
    /// Number of memories to chunk (default: 10)
    #[arg(long, default_value = "10")]
    pub limit: usize,

    /// Chunk all memories (ignore --limit)
    #[arg(long)]
    pub all: bool,

    /// Neo4j connection URL (e.g., neo4j+s://xyz.databases.neo4j.io) - optional
    #[arg(long)]
    pub neo4j_url: Option<String>,

    /// Minimum memory content length to chunk (chars)
    #[arg(long, default_value = "100")]
    pub min_length: usize,

    /// Batch size for Neo4j commits
    #[arg(long, default_value = "100")]
    pub batch_size: usize,

    /// Skip schema creation (assume already exists)
    #[arg(long)]
    pub skip_schema: bool,
}

pub async fn run(args: ChunkArgs, db: &std::sync::Arc<dyn Database>) -> Result<()> {
    info!("═══════════════════════════════════════════════════════════════════");
    info!("PHASE A PART D: Chunking Memories");
    info!("═══════════════════════════════════════════════════════════════════");

    // Step 1: Determine memory count
    // Step 1: Determine memory count (all memories by default since DB abstracted)
    let total_memories = if args.all {
        999999  // Use a large number, fetch_memories_for_chunking will respect actual count
    } else {
        args.limit
    };

    info!("Total memories to chunk: {}", total_memories);
    info!("───────────────────────────────────────────────────────────────────");

    // Step 2: Load memories from database (backend-agnostic)
    info!("Loading memories from backend...");
    let memories = db.fetch_memories_for_chunking(total_memories).await?;

    info!("Loaded {} memories", memories.len());
    info!("───────────────────────────────────────────────────────────────────");

    // Step 3: Chunk memories
    let start_time = Instant::now();
    let mut total_chunks = 0;
    let mut total_coherence = 0.0;
    let mut memory_count = 0;
    let mut failed_count = 0;
    let mut quality_dist = std::collections::HashMap::new();

    let strategy = voidm_core::chunking::ChunkingStrategy::default();

    for (idx, (memory_id, content, created_at)) in memories.iter().enumerate() {
        debug!("[{}/{}] Chunking memory {}", idx + 1, memories.len(), memory_id);

        match voidm_core::chunking::chunk_smart(&memory_id, &content, &strategy, &created_at) {
            Ok(chunks) => {
                let chunk_count = chunks.len();
                let mut memory_coherence = 0.0;

                for chunk in chunks.iter() {
                    let score = voidm_core::coherence::estimate_coherence(&chunk.content);
                    memory_coherence += score.final_score();
                    
                    // Track quality distribution
                    let quality = score.quality_level().to_string();
                    *quality_dist.entry(quality).or_insert(0) += 1;
                }

                let avg_coherence = if chunk_count > 0 {
                    memory_coherence / chunk_count as f32
                } else {
                    0.0
                };

                info!(
                    "[CHUNKING] Memory {}: {} chunks, coherence {:.2}",
                    memory_id, chunk_count, avg_coherence
                );

                total_chunks += chunk_count;
                total_coherence += avg_coherence;
                memory_count += 1;
            }
            Err(e) => {
                warn!("[CHUNKING] Failed to chunk memory {}: {}", memory_id, e);
                failed_count += 1;
            }
        }

        // Progress indicator
        if (idx + 1) % 100 == 0 {
            info!("Progress: {}/{} memories processed", idx + 1, memories.len());
        }
    }

    // Step 4: Summary
    let elapsed = start_time.elapsed();
    info!("───────────────────────────────────────────────────────────────────");
    info!("CHUNKING COMPLETE");
    info!("───────────────────────────────────────────────────────────────────");

    info!("Total memories chunked: {}", memory_count);
    info!("Total chunks created: {}", total_chunks);
    if memory_count > 0 {
        info!("Avg chunks per memory: {:.1}", total_chunks as f64 / memory_count as f64);
        info!("Avg coherence: {:.2}", total_coherence / memory_count as f32);
    }

    // Quality distribution
    if !quality_dist.is_empty() {
        info!("Quality distribution:");
        for level in &["🟣 EXCELLENT", "🟢 GOOD", "🟡 FAIR", "🔴 POOR"] {
            if let Some(count) = quality_dist.get(*level) {
                let pct = *count as f64 / total_chunks as f64 * 100.0;
                info!("  {}: {} ({:.1}%)", level, count, pct);
            }
        }
    }

    info!("Total time: {:.1}s", elapsed.as_secs_f64());
    if memory_count > 0 {
        info!("Time per memory: {:.2}s", elapsed.as_secs_f64() / memory_count as f64);
        info!("Time per chunk: {:.3}ms", elapsed.as_secs_f64() * 1000.0 / total_chunks as f64);
    }

    if failed_count > 0 {
        warn!("Failed chunks: {}", failed_count);
    }

    info!("───────────────────────────────────────────────────────────────────");

    if total_chunks == 0 {
        warn!("No chunks created! Check memory content length threshold.");
    } else {
        info!("✅ Chunking ready for storage and Part E: Embedding generation");
    }

    Ok(())
}
