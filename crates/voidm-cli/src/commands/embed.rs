/// Part E: Embedding Generation for Chunks
///
/// This command generates embeddings for all chunks using fastembed.
///
/// Usage:
/// ```bash
/// voidm embed --limit 10
/// voidm embed --all
/// voidm embed --model "all-MiniLM-L6-v2" --batch-size 32
/// ```

use anyhow::Result;
use clap::Args;
use voidm_db::Database;
use tracing::{info, debug};
use std::time::Instant;
use std::sync::Arc;

#[derive(Args)]
pub struct EmbedArgs {
    /// Number of chunks to embed (default: 10)
    #[arg(long, default_value = "10")]
    pub limit: usize,

    /// Embed all chunks (ignore --limit)
    #[arg(long)]
    pub all: bool,

    /// Embedding model name (from fastembed)
    #[arg(long, default_value = "all-MiniLM-L6-v2")]
    pub model: String,

    /// Batch size for embedding generation
    #[arg(long, default_value = "32")]
    pub batch_size: usize,

    /// Skip chunks that already have embeddings
    #[arg(long, default_value = "true")]
    pub skip_existing: bool,
}

pub async fn run(args: EmbedArgs, db: &Arc<dyn Database>) -> Result<()> {
    info!("═══════════════════════════════════════════════════════════════════");
    info!("PHASE A PART E: Embedding Generation for Chunks");
    info!("═══════════════════════════════════════════════════════════════════");

    let total_chunks = if args.all {
        999999  // Load all chunks
    } else {
        args.limit
    };

    info!("Model: {}", args.model);
    info!("Batch size: {}", args.batch_size);
    info!("Total chunks to process: {}", total_chunks);
    info!("───────────────────────────────────────────────────────────────────");

    // Load chunks from backend
    info!("Loading chunks from backend...");
    let chunks = db.fetch_chunks(total_chunks).await?;
    
    info!("Loaded {} chunks", chunks.len());
    info!("───────────────────────────────────────────────────────────────────");

    if chunks.is_empty() {
        info!("No chunks to embed");
        return Ok(());
    }

    let start_time = Instant::now();
    let mut total_processed = 0;
    let mut total_skipped = 0;
    let mut embedding_batches = 0;
    let mut total_embedding_time = std::time::Duration::ZERO;

    // Process chunks in batches
    let chunk_batches: Vec<Vec<_>> = chunks
        .chunks(args.batch_size)
        .map(|batch| batch.to_vec())
        .collect();

    info!("Processing {} batches of max {} chunks", chunk_batches.len(), args.batch_size);
    info!("───────────────────────────────────────────────────────────────────");

    for (batch_idx, batch) in chunk_batches.iter().enumerate() {
        let batch_start = Instant::now();
        
        // Extract texts from chunk tuples
        let texts: Vec<String> = batch.iter().map(|(_, content, _)| content.clone()).collect();
        
        debug!("[Batch {}/{}] Embedding {} chunks", 
            batch_idx + 1, chunk_batches.len(), texts.len());

        // Generate embeddings using fastembed
        match voidm_core::embeddings::embed_batch(&args.model, &texts) {
            Ok(embeddings) => {
                // Store each embedding
                for (chunk_tuple, embedding) in batch.iter().zip(embeddings.iter()) {
                    let (chunk_id, _, _) = chunk_tuple;
                    
                    match db.store_chunk_embedding(chunk_id.clone(), chunk_id.clone(), embedding.clone()).await {
                        Ok((stored_id, dim)) => {
                            total_processed += 1;
                            debug!("Stored {}D embedding for {}", dim, stored_id);
                        }
                        Err(e) => {
                            debug!("Failed to store embedding for {}: {}", chunk_id, e);
                            total_skipped += 1;
                        }
                    }
                }
                
                embedding_batches += 1;
                let batch_elapsed = batch_start.elapsed();
                total_embedding_time += batch_elapsed;
                
                info!("[Batch {}/{}] Embedded {} chunks in {:.2}ms",
                    batch_idx + 1,
                    chunk_batches.len(),
                    texts.len(),
                    batch_elapsed.as_secs_f32() * 1000.0
                );
            }
            Err(e) => {
                info!("[Batch {}/{}] Embedding failed: {}", batch_idx + 1, chunk_batches.len(), e);
                total_skipped += texts.len();
            }
        }
    }

    let total_elapsed = start_time.elapsed();

    info!("═══════════════════════════════════════════════════════════════════");
    info!("EMBEDDING COMPLETE");
    info!("═══════════════════════════════════════════════════════════════════");
    info!("Total chunks embedded: {}", total_processed);
    info!("Total chunks skipped: {}", total_skipped);
    info!("Batch count: {}", embedding_batches);
    info!("Total embedding time: {:.2}s", total_embedding_time.as_secs_f32());
    info!("Total elapsed time: {:.2}s", total_elapsed.as_secs_f32());
    info!("Time per chunk: {:.3}ms", 
        if total_processed > 0 {
            total_embedding_time.as_secs_f32() * 1000.0 / total_processed as f32
        } else {
            0.0
        }
    );
    info!("───────────────────────────────────────────────────────────────────");
    info!("✅ Embedding generation complete - {} chunks embedded", total_processed);

    Ok(())
}
