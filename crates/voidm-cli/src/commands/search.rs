use anyhow::Result;
use clap::Args;
use sqlx::SqlitePool;
use voidm_core::{Config, search::{SearchOptions, SearchMode, search}};

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
}

pub async fn run(args: SearchArgs, pool: &SqlitePool, config: &Config, json: bool) -> Result<()> {
    let mode: SearchMode = args.mode.parse()?;

    let opts = SearchOptions {
        query: args.query.clone(),
        mode,
        limit: args.limit,
        scope_filter: args.scope,
        type_filter: args.r#type,
        min_score: args.min_score,
    };

    let resp = search(
        pool,
        &opts,
        &config.embeddings.model,
        config.embeddings.enabled,
        config.search.min_score,
    ).await?;

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

        for r in &resp.results {
            println!("[{:.3}] {} ({})", r.score, r.id, r.memory_type);
            let preview = if r.content.len() > 100 {
                format!("{}...", voidm_core::search::safe_truncate(&r.content, 100))
            } else {
                r.content.clone()
            };
            println!("  {}", preview);
            if !r.scopes.is_empty() {
                println!("  Scopes: {}", r.scopes.join(", "));
            }
            println!();
        }
    }
    Ok(())
}
