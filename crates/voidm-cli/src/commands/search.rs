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
use voidm_core::memory_policy::{
    RETRIEVAL_MAX_CHARS_PER_CHUNK,
    RETRIEVAL_MAX_CHUNKS,
    RETRIEVAL_TOTAL_CHAR_BUDGET,
};
use voidm_db::Database;
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

    /// Filter by tag(s), comma-separated
    #[arg(long, value_delimiter = ',')]
    pub tag: Option<Vec<String>>,

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
    config: &Config,
    json_output: bool,
) -> Result<()> {
    // Vector search mode (Phase B)
    if args.mode == "vector" {
        return run_vector_search(args, db, config, json_output).await;
    }

    let opts = voidm_core::search::SearchOptions {
        query: args.query.clone(),
        mode: args.mode.parse()?,
        limit: args.limit,
        scope_filter: args.scope.clone(),
        type_filter: args.r#type.clone(),
        tag_filter: args.tag.clone(),
        min_score: args.min_score,
        min_quality: args.min_quality,
        include_neighbors: args.include_neighbors,
        neighbor_depth: args.neighbor_depth,
        neighbor_decay: args.neighbor_decay,
        neighbor_min_score: args.neighbor_min_score,
        neighbor_limit: args.neighbor_limit,
        edge_types: args.edge_types.clone(),
        intent: None,
    };

    let response = voidm_core::search::search(
        db.as_ref(),
        &opts,
        &config.embeddings.model,
        config.embeddings.enabled,
        config.search.min_score,
        &config.search,
    ).await?;

    if json_output {
        crate::output::print_json(&serde_json::json!({
            "query": args.query,
            "mode": args.mode,
            "limit": args.limit,
            "results": response.results,
            "threshold_applied": response.threshold_applied,
            "best_score": response.best_score,
        }))?;
    } else {
        if response.results.is_empty() {
            println!("No results.");
            return Ok(());
        }
        for (idx, result) in response.results.iter().enumerate() {
            println!("{}. [{}] {}  score={:.3}", idx + 1, result.memory_type, result.id, result.score);
            if let Some(title) = result.title.as_deref() {
                println!("   title: {}", title);
            }
            if !result.scopes.is_empty() {
                println!("   scopes: {}", result.scopes.join(", "));
            }
            if !result.tags.is_empty() {
                println!("   tags: {}", result.tags.join(", "));
            }
            println!("   content:");
            for line in result.content.lines() {
                println!("     {}", line);
            }
            if !result.context_chunks.is_empty() {
                println!("   context_chunks:");
                for chunk in &result.context_chunks {
                    println!("     - {}", chunk.replace('\n', " "));
                }
            }
            println!();
        }
    }

    Ok(())
}

async fn run_vector_search(
    args: SearchArgs,
    db: &Arc<dyn Database>,
    config: &Config,
    json_output: bool,
) -> Result<()> {
    if args.query.is_empty() {
        anyhow::bail!("Query cannot be empty");
    }

    if !(0.0..=1.0).contains(&args.min_similarity) {
        anyhow::bail!("min_similarity must be between 0.0 and 1.0");
    }

    let start_time = Instant::now();

    // Generate embedding for the query
    let model_name = if args.model.is_empty() { &config.embeddings.model } else { &args.model };
    let query_embedding = match voidm_core::embeddings::embed_text(model_name, &args.query) {
        Ok(embedding) => {
            embedding
        }
        Err(e) => {
            anyhow::bail!("Error generating embedding: {}", e);
        }
    };

    let embedding_time = start_time.elapsed();

    let chunk_hits = db.search_chunk_ann(
        query_embedding,
        args.limit * 3,
        args.scope.as_deref(),
        args.r#type.as_deref(),
    ).await?;

    let mut rows = Vec::new();
    let mut total_chars = 0usize;
    for (chunk_id, score) in chunk_hits.into_iter().take(RETRIEVAL_MAX_CHUNKS) {
        if let Some(chunk) = db.get_chunk(&chunk_id).await? {
            let text = chunk.get("text").and_then(|v| v.as_str()).unwrap_or("");
            let trimmed: String = text.chars().take(RETRIEVAL_MAX_CHARS_PER_CHUNK).collect();
            if total_chars + trimmed.len() > RETRIEVAL_TOTAL_CHAR_BUDGET { break; }
            total_chars += trimmed.len();
            rows.push(serde_json::json!({
                "chunk_id": chunk_id,
                "score": score,
                "content": trimmed,
                "memory_id": chunk.get("memory_id").cloned().unwrap_or(serde_json::Value::Null),
            }));
        }
    }

    let total_time = start_time.elapsed();
    if json_output {
        crate::output::print_json(&serde_json::json!({
            "query": args.query,
            "mode": "vector",
            "embedding_time_ms": (embedding_time.as_secs_f32() * 1000.0),
            "total_time_ms": (total_time.as_secs_f32() * 1000.0),
            "results": rows,
        }))?;
    } else {
        if rows.is_empty() {
            println!("No matching chunks found.");
            return Ok(());
        }
        println!("Vector search results for: {}", args.query);
        for (idx, row) in rows.iter().enumerate() {
            println!("{}. {}  score={:.3}", idx + 1, row["chunk_id"].as_str().unwrap_or(""), row["score"].as_f64().unwrap_or(0.0));
            if let Some(memory_id) = row["memory_id"].as_str() {
                if !memory_id.is_empty() {
                    println!("   memory_id: {}", memory_id);
                }
            }
            println!("   content:");
            for line in row["content"].as_str().unwrap_or("").lines() {
                println!("     {}", line);
            }
            println!();
        }
    }

    Ok(())
}
