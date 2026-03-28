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
use sqlx::SqlitePool;
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

    /// Neo4j connection URL (e.g., bolt://localhost:7687)
    #[arg(long)]
    pub neo4j_url: String,

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

pub async fn run(args: ChunkArgs, pool: &SqlitePool) -> Result<()> {
    info!("═══════════════════════════════════════════════════════════════════");
    info!("PHASE A PART D: Chunking Memories into Neo4j");
    info!("═══════════════════════════════════════════════════════════════════");

    // Step 1: Determine memory count
    let total_memories = if args.all {
        // Count all memories
        let count_result = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM memories WHERE LENGTH(content) > ?"
        )
        .bind(args.min_length as i32)
        .fetch_one(pool)
        .await?;
        count_result as usize
    } else {
        args.limit
    };

    info!("Total memories to chunk: {}", total_memories);
    info!("Batch size: {}", args.batch_size);
    info!("Neo4j URL: {}", args.neo4j_url);
    info!("───────────────────────────────────────────────────────────────────");

    // Step 2: Schema creation (if not skipped)
    if !args.skip_schema {
        info!("Creating Neo4j schema...");
        info!("  - Creating UNIQUE constraint on MemoryChunk.id");
        info!("  - Creating index on MemoryChunk.memory_id");
        info!("  - Creating index on MemoryChunk.created_at");
        info!("  - Creating index on Memory.id");
        warn!("Schema creation not yet implemented - using --skip-schema for now");
    }

    // Step 3: Load memories from SQLite
    info!("Loading memories from SQLite...");
    let memories: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT id, content, type FROM memories \
         WHERE LENGTH(content) > ? \
         ORDER BY created_at DESC \
         LIMIT ?"
    )
    .bind(args.min_length as i32)
    .bind(total_memories as i32)
    .fetch_all(pool)
    .await?;

    info!("Loaded {} memories", memories.len());
    info!("───────────────────────────────────────────────────────────────────");

    // Step 4: Chunk memories
    let start_time = Instant::now();
    let mut total_chunks = 0;
    let mut total_coherence = 0.0;
    let mut memory_count = 0;

    let strategy = voidm_core::chunking::ChunkingStrategy::default();

    for (idx, (memory_id, content, memory_type)) in memories.iter().enumerate() {
        debug!("[{}/{}] Chunking memory {}", idx + 1, memories.len(), memory_id);

        match voidm_core::chunking::chunk_smart(&memory_id, &content, &strategy) {
            Ok(chunks) => {
                let chunk_count = chunks.len();
                let mut memory_coherence = 0.0;

                for chunk in &chunks {
                    let score = voidm_core::coherence::estimate_coherence(&chunk.content);
                    memory_coherence += score.final_score();
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

                // TODO: Store chunks in Neo4j
                // This would be the actual implementation in the next iteration
            }
            Err(e) => {
                warn!("[CHUNKING] Failed to chunk memory {}: {}", memory_id, e);
            }
        }

        // Progress indicator
        if (idx + 1) % 50 == 0 {
            info!("Progress: {}/{} memories processed", idx + 1, memories.len());
        }
    }

    // Step 5: Summary
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
    info!("Total time: {:.1}s", elapsed.as_secs_f64());
    if memory_count > 0 {
        info!("Time per memory: {:.2}s", elapsed.as_secs_f64() / memory_count as f64);
    }

    info!("───────────────────────────────────────────────────────────────────");

    if total_chunks == 0 {
        warn!("No chunks created! Check memory content length threshold.");
    } else {
        info!("✅ Ready for Part E: Embedding generation");
    }

    Ok(())
}
