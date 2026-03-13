use anyhow::Result;
use clap::Args;
use sqlx::SqlitePool;
use std::time::Instant;
use voidm_core::{
    Config, search::{SearchOptions, SearchMode, search}, user_interactions::track_interaction,
    user_adaptive_system::{UserAdaptiveConfig, get_adaptive_response},
    user_personalization::get_personalized_rankings,
};

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

    /// Search mode: hybrid, semantic, keyword, fuzzy, bm25
    #[arg(long, default_value = "hybrid")]
    pub mode: String,

    /// Maximum results
    #[arg(long, default_value = "10")]
    pub limit: usize,

    /// Minimum score threshold (hybrid mode only). Overrides config search.min_score.
    /// Use --min-score 0 to disable filtering.
    #[arg(long)]
    pub min_score: Option<f32>,

    /// Minimum quality score (0.0-1.0) for results. Filters by quality_score.
    /// Use --min-quality 0.7 to exclude low-quality memories.
    #[arg(long)]
    pub min_quality: Option<f32>,

    /// Expand results with graph neighbors
    #[arg(long, default_value_t = false)]
    pub include_neighbors: bool,

    /// Max hops for neighbor expansion (default: config, hard cap: 3)
    #[arg(long)]
    pub neighbor_depth: Option<u8>,

    /// Score decay per hop (default: config neighbor_decay)
    #[arg(long)]
    pub neighbor_decay: Option<f32>,

    /// Min score for neighbors to be included (default: config neighbor_min_score)
    #[arg(long)]
    pub neighbor_min_score: Option<f32>,

    /// Max total neighbors to append (default: same as --limit)
    #[arg(long)]
    pub neighbor_limit: Option<usize>,

    /// Comma-separated edge types to traverse (default: PART_OF,SUPPORTS,DERIVED_FROM,EXEMPLIFIES)
    #[arg(long, value_delimiter = ',')]
    pub edge_types: Option<Vec<String>>,
}

pub async fn run(args: SearchArgs, pool: &SqlitePool, config: &Config, json: bool) -> Result<()> {
    let mode: SearchMode = args.mode.parse()?;
    let start = Instant::now();
    let user_id = std::env::var("VOIDM_USER").unwrap_or_else(|_| "agent-cli".to_string());
    
    // Store these before they're moved
    let query_clone = args.query.clone();
    let scope_clone = args.scope.clone();

    let opts = SearchOptions {
        query: args.query.clone(),
        mode,
        limit: args.limit,
        scope_filter: args.scope,
        type_filter: args.r#type,
        min_score: args.min_score,
        min_quality: args.min_quality,
        include_neighbors: args.include_neighbors,
        neighbor_depth: args.neighbor_depth,
        neighbor_decay: args.neighbor_decay,
        neighbor_min_score: args.neighbor_min_score,
        neighbor_limit: args.neighbor_limit,
        edge_types: args.edge_types,
    };

    let mut resp = search(
        pool,
        &opts,
        &config.embeddings.model,
        config.embeddings.enabled,
        config.search.min_score,
        &config.search,
    ).await?;

    // Apply personalization if user has enough interactions
    let adaptive_config = UserAdaptiveConfig::default();
    
    if !resp.results.is_empty() {
        // Try to get adaptive response for this user
        if let Ok(adaptive) = get_adaptive_response(pool, &user_id, &adaptive_config).await {
            // If user has learned history, apply personalization to rankings
            if adaptive.confidence > 0.5 {
                // Build a personalization boost map
                let mut boost_map: std::collections::HashMap<String, f32> = std::collections::HashMap::new();
                
                // Get personalized rankings for this user
                if let Ok(personalized_rankings) = get_personalized_rankings(pool, &user_id, 100).await {
                    for ranking in personalized_rankings {
                        boost_map.insert(ranking.concept_id, ranking.personalized_score);
                    }
                }
                
                // Rerank results using the boost map
                if !boost_map.is_empty() {
                    for result in resp.results.iter_mut() {
                        if let Some(boost) = boost_map.get(&result.id) {
                            // Blend original score with personalized boost
                            let original = result.score;
                            result.score = original * 0.6 + boost * 0.4;
                        }
                    }
                    // Re-sort by personalized scores
                    resp.results.sort_by(|a, b| {
                        b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal)
                    });
                }
            }
        }
    }

    // Track the search interaction
    let duration_ms = start.elapsed().as_millis() as i64;
    let scope_str = scope_clone.as_deref().unwrap_or("general").to_string();
    let context = format!("search:{}", scope_str);
    let result_status = if resp.results.is_empty() { "no_results" } else { "success" };
    
    let _ = track_interaction(
        pool,
        &user_id,
        "search",
        &query_clone,
        &query_clone,
        result_status,
        duration_ms,
        Some(&context),
    ).await;

    if json {
        if resp.results.is_empty() {
            // Return best result even if below threshold, so agent can decide
            if let Some(best_score) = resp.best_score {
                let threshold = resp.threshold_applied.unwrap_or(config.search.min_score);
                let threshold_rounded = (threshold as f64 * 100.0).round() / 100.0;
                let best_rounded = (best_score as f64 * 100.0).round() / 100.0;
                println!("{}", serde_json::json!({
                    "results": [],
                    "threshold": threshold_rounded,
                    "best_score": best_rounded,
                    "hint": format!(
                        "No results above score {:.2}. Best match scored {:.2}. \
                         Try --min-score {:.1} or --mode semantic.",
                        threshold,
                        best_score,
                        (best_score * 0.9).max(0.0)
                    )
                }));
            } else {
                println!("{}", serde_json::json!({
                    "results": [],
                    "threshold": null,
                    "best_score": null,
                    "hint": "No memories found. Use 'voidm add' to create memories."
                }));
            }
        } else {
            println!("{}", serde_json::to_string_pretty(&resp.results)?);
        }
    } else {
        if resp.results.is_empty() {
            if let Some(threshold) = resp.threshold_applied {
                let best = resp.best_score.unwrap_or(0.0);
                eprintln!(
                    "No results above score {:.2} (best match: {:.2}).",
                    threshold, best
                );
                eprintln!(
                    "Try: --min-score {:.1}  or  --mode semantic  or  --min-score 0 to disable filtering.",
                    (best * 0.9).max(0.0)
                );
            } else {
                println!("No results found. Use 'voidm add' to create memories.");
            }
            return Ok(());
        }

        // Check if personalization was applied
        let adaptive_config_check = UserAdaptiveConfig::default();
        let personalization_active = if let Ok(adaptive) = get_adaptive_response(pool, &user_id, &adaptive_config_check).await {
            adaptive.confidence > 0.5
        } else {
            false
        };

        for r in &resp.results {
            if r.source == "graph" {
                let rel = r.rel_type.as_deref().unwrap_or("?");
                let dir = r.direction.as_deref().unwrap_or("?");
                let depth = r.hop_depth.unwrap_or(0);
                let parent = r.parent_id.as_deref().unwrap_or("?");
                println!("  ↳ [{:.3}] {} ({}) [graph: {} {} depth={}  parent={}]",
                    r.score, r.id, r.memory_type, rel, dir, depth, &parent[..8.min(parent.len())]);
            } else {
                println!("[{:.3}] {} ({})", r.score, r.id, r.memory_type);
            }
            let preview = if r.content.len() > 100 {
                format!("{}...", voidm_core::search::safe_truncate(&r.content, 100))
            } else {
                r.content.clone()
            };
            println!("  {}", preview);
            if let Some(qs) = r.quality_score {
                println!("  Quality: {:.2}", qs);
            }
            if !r.scopes.is_empty() {
                println!("  Scopes: {}", r.scopes.join(", "));
            }
            println!();
        }

        // Show personalization status
        if personalization_active {
            eprintln!("💡 Results are personalized by user preferences (confidence: {}+)", "high");
        }
    }
    Ok(())
}
