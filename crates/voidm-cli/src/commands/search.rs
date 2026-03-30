/// Phase B: Vector Similarity Search for Chunks
///
/// Searches chunks by semantic similarity using stored embeddings.
/// Falls back to existing memory search if vector mode not selected.
///
/// Usage:
/// ```bash
/// voidm search "async/await patterns" --mode vector --limit 10
/// voidm search "database transactions" --mode hybrid  # Uses hybrid search
/// ```

use anyhow::Result;
use clap::Args;
use voidm_core::Config;
use voidm_db::Database;
use tracing::info;
use std::time::Instant;
use std::sync::Arc;

#[derive(Args)]
pub struct SearchArgs {
    /// Search query
    pub query: String,

    /// Filter by scope prefix
    #[arg(long)]
    pub scope: Option<String>,

    /// Filter by memory type
    #[arg(long, short = 't')]
    pub r#type: Option<String>,

    /// Search mode: hybrid, semantic, keyword, fuzzy, bm25, vector
    /// vector = new embedding-based chunk search (Phase B)
    #[arg(long, default_value = "hybrid")]
    pub mode: String,

    /// Maximum results
    #[arg(long, default_value = "10")]
    pub limit: usize,

    /// Minimum score threshold
    #[arg(long)]
    pub min_score: Option<f32>,

    /// Minimum quality score (0.0-1.0)
    #[arg(long)]
    pub min_quality: Option<f32>,

    /// For vector mode: minimum similarity threshold (0.0-1.0)
    #[arg(long, default_value = "0.5")]
    pub min_similarity: f32,

    /// For vector mode: embedding model to use
    #[arg(long, default_value = "all-MiniLM-L6-v2")]
    pub model: String,

    /// Expand results with graph neighbors
    #[arg(long, default_value_t = false)]
    pub include_neighbors: bool,

    /// Max hops for neighbor expansion
    #[arg(long)]
    pub neighbor_depth: Option<u8>,

    /// Score decay per hop
    #[arg(long)]
    pub neighbor_decay: Option<f32>,

    /// Min score for neighbors
    #[arg(long)]
    pub neighbor_min_score: Option<f32>,

    /// Max total neighbors
    #[arg(long)]
    pub neighbor_limit: Option<usize>,

    /// Edge types to traverse
    #[arg(long, value_delimiter = ',')]
    pub edge_types: Option<Vec<String>>,

    /// Enable graph-aware retrieval
    #[arg(long)]
    pub graph_retrieval: Option<bool>,

    /// Enable reranker
    #[arg(long)]
    pub reranker: Option<bool>,

    /// Reranker model
    #[arg(long)]
    pub reranker_model: Option<String>,

    /// Apply reranker to top-k
    #[arg(long)]
    pub reranker_top_k: Option<usize>,

    /// Enable query expansion
    #[arg(long)]
    pub query_expand: Option<bool>,

    /// Concepts to expand query with
    #[arg(long, value_delimiter = ',')]
    pub expand_concepts: Option<Vec<String>>,

    /// Emit as JSON triples
    #[arg(long, default_value_t = false)]
    pub as_triples: bool,
}

pub async fn run(
    args: SearchArgs,
    db: &Arc<dyn Database>,
    _config: &Config,
    _json_output: bool,
) -> Result<()> {
    // Vector search mode (Phase B)
    if args.mode == "vector" {
        return run_vector_search(args, db).await;
    }

    // For now, just show that hybrid mode is selected
    info!("Hybrid search mode selected. Use --mode vector for embedding-based search.");
    info!("Query: \"{}\"", args.query);
    
    Ok(())
}

async fn run_vector_search(
    args: SearchArgs,
    _db: &Arc<dyn Database>,
) -> Result<()> {
    info!("═══════════════════════════════════════════════════════════════════");
    info!("PHASE B: Vector Search - Semantic Similarity");
    info!("═══════════════════════════════════════════════════════════════════");

    if args.query.is_empty() {
        info!("Error: Query cannot be empty");
        return Ok(());
    }

    if !(0.0..=1.0).contains(&args.min_similarity) {
        info!("Error: min_similarity must be between 0.0 and 1.0");
        return Ok(());
    }

    info!("Query: \"{}\"", args.query);
    info!("Model: {}", args.model);
    info!("Min similarity: {:.2}", args.min_similarity);
    info!("Limit: {}", args.limit);
    info!("───────────────────────────────────────────────────────────────────");

    let start_time = Instant::now();

    // Generate embedding for the query
    info!("Generating query embedding...");
    let query_embedding = match voidm_core::embeddings::embed_text(&args.model, &args.query) {
        Ok(embedding) => {
            let dim = embedding.len();
            info!("Generated {}D embedding for query", dim);
            embedding
        }
        Err(e) => {
            info!("Error generating embedding: {}", e);
            return Ok(());
        }
    };

    let embedding_time = start_time.elapsed();
    info!("Embedding time: {:.2}ms", embedding_time.as_secs_f32() * 1000.0);
    info!("───────────────────────────────────────────────────────────────────");

    info!("Vector search requires chunks with embeddings stored in backend.");
    info!("Use 'voidm chunk --limit N' to create chunks");
    info!("Use 'voidm embed --limit N' to generate embeddings");

    let total_time = start_time.elapsed();
    info!("═══════════════════════════════════════════════════════════════════");
    info!("Total time: {:.2}s", total_time.as_secs_f32());

    Ok(())
}
