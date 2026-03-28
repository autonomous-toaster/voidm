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
use voidm_core::Neo4jDb;

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

    // Connect to Neo4j
    info!("Connecting to Neo4j...");
    let neo4j = Neo4jDb::connect(&args.neo4j_url, "neo4j", "neo4jpassword").await?;

    // Step 1: Schema creation (if not skipped)
    if !args.skip_schema {
        info!("Creating Neo4j schema...");
        info!("  - Creating UNIQUE constraint on MemoryChunk.id");
        info!("  - Creating index on MemoryChunk.memory_id");
        info!("  - Creating index on MemoryChunk.created_at");
        info!("  - Creating index on Memory.id");
        neo4j.create_schema().await?;
        info!("───────────────────────────────────────────────────────────────────");
    }

    // Step 2: Determine memory count
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
    info!("───────────────────────────────────────────────────────────────────");

    // Step 3: Load memories from SQLite
    info!("Loading memories from SQLite...");
    let memories: Vec<(String, String)> = sqlx::query_as(
        "SELECT id, content FROM memories \
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

    // Step 4: Chunk memories and store in Neo4j
    let start_time = Instant::now();
    let mut total_chunks = 0;
    let mut total_coherence = 0.0;
    let mut memory_count = 0;
    let mut failed_count = 0;

    let strategy = voidm_core::chunking::ChunkingStrategy::default();

    for (idx, (memory_id, content)) in memories.iter().enumerate() {
        debug!("[{}/{}] Chunking memory {}", idx + 1, memories.len(), memory_id);

        match voidm_core::chunking::chunk_smart(&memory_id, &content, &strategy) {
            Ok(chunks) => {
                let chunk_count = chunks.len();
                let mut memory_coherence = 0.0;

                for (chunk_idx, chunk) in chunks.iter().enumerate() {
                    let score = voidm_core::coherence::estimate_coherence(&chunk.content);
                    memory_coherence += score.final_score();

                    // Detect if chunk is code-like
                    let is_code_like = chunk.content.contains("fn ") || 
                                      chunk.content.contains("def ") ||
                                      chunk.content.contains("class ") ||
                                      chunk.content.contains("impl ");

                    // Store chunk in Neo4j
                    let break_type_str = match chunk.break_type {
                        voidm_core::chunking::BreakType::Paragraph => "paragraph",
                        voidm_core::chunking::BreakType::Sentence => "sentence",
                        voidm_core::chunking::BreakType::Word => "word",
                        voidm_core::chunking::BreakType::Character => "character",
                    };

                    match neo4j.create_chunk(
                        &chunk.id,
                        &memory_id,
                        chunk_idx as i32,
                        &chunk.content,
                        chunk.content.len() as i32,
                        break_type_str,
                        score.completeness,
                        score.coherence,
                        score.relevance,
                        score.specificity,
                        score.metadata,
                        score.final_score(),
                        score.quality_level(),
                        is_code_like,
                    ).await {
                        Ok(_) => {
                            debug!("Created MemoryChunk {}", chunk.id);
                        }
                        Err(e) => {
                            warn!("Failed to create chunk {}: {}", chunk.id, e);
                            failed_count += 1;
                        }
                    }
                }

                // Create CONTAINS relationships
                for (chunk_idx, chunk) in chunks.iter().enumerate() {
                    match neo4j.create_contains_relationship(&memory_id, &chunk.id, chunk_idx as i32).await {
                        Ok(_) => {
                            debug!("Created CONTAINS relationship {} -> {}", memory_id, chunk.id);
                        }
                        Err(e) => {
                            warn!("Failed to create relationship: {}", e);
                        }
                    }
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
        if (idx + 1) % 50 == 0 {
            info!("Progress: {}/{} memories processed", idx + 1, memories.len());
        }
    }

    // Step 5: Get statistics from Neo4j
    info!("───────────────────────────────────────────────────────────────────");
    info!("Retrieving Neo4j statistics...");

    let chunk_count = neo4j.get_chunk_count().await.unwrap_or(0);
    let avg_coherence_neo4j = neo4j.get_average_coherence().await.unwrap_or(0.0);
    let quality_dist = neo4j.get_quality_distribution().await.unwrap_or_default();

    // Step 6: Summary
    let elapsed = start_time.elapsed();
    info!("───────────────────────────────────────────────────────────────────");
    info!("CHUNKING COMPLETE");
    info!("───────────────────────────────────────────────────────────────────");

    info!("Total memories chunked: {}", memory_count);
    info!("Total chunks created: {}", total_chunks);
    info!("Neo4j chunks stored: {}", chunk_count);
    if memory_count > 0 {
        info!("Avg chunks per memory: {:.1}", total_chunks as f64 / memory_count as f64);
        info!("Avg coherence (computed): {:.2}", total_coherence / memory_count as f32);
    }
    info!("Avg coherence (Neo4j): {:.2}", avg_coherence_neo4j);

    // Quality distribution
    if !quality_dist.is_empty() {
        info!("Quality distribution in Neo4j:");
        for (level, count) in &quality_dist {
            info!("  {}: {}", level, count);
        }
    }

    info!("Total time: {:.1}s", elapsed.as_secs_f64());
    if memory_count > 0 {
        info!("Time per memory: {:.2}s", elapsed.as_secs_f64() / memory_count as f64);
    }

    if failed_count > 0 {
        warn!("Failed chunks: {} (will retry or skip)", failed_count);
    }

    info!("───────────────────────────────────────────────────────────────────");

    if total_chunks == 0 {
        warn!("No chunks created! Check memory content length threshold.");
    } else {
        info!("✅ Ready for Part E: Embedding generation");
    }

    neo4j.close().await?;
    Ok(())
}
