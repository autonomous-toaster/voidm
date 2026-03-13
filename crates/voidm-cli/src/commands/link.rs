use anyhow::Result;
use clap::Args;
use sqlx::SqlitePool;
use std::time::Instant;
use voidm_core::{crud, models::EdgeType, resolve_id, user_interactions::track_interaction};

#[derive(Args)]
pub struct LinkArgs {
    /// Source memory ID or short prefix
    pub from: String,
    /// Edge type (SUPPORTS, CONTRADICTS, DERIVED_FROM, PRECEDES, PART_OF, EXEMPLIFIES, INVALIDATES, RELATES_TO)
    pub rel: String,
    /// Target memory ID or short prefix
    pub to: String,
    /// Note (required for RELATES_TO)
    #[arg(long)]
    pub note: Option<String>,
}

pub async fn run(args: LinkArgs, pool: &SqlitePool, json: bool) -> Result<()> {
    let start = Instant::now();
    let user_id = std::env::var("VOIDM_USER").unwrap_or_else(|_| "agent-cli".to_string());
    
    let edge_type: EdgeType = args.rel.parse()?;
    let from = resolve_id(pool, &args.from).await?;
    let to   = resolve_id(pool, &args.to).await?;
    let resp = crud::link_memories(pool, &from, &edge_type, &to, args.note.as_deref()).await?;

    // Track the link/explore interaction
    let duration_ms = start.elapsed().as_millis() as i64;
    let context = format!("explore:{}", args.rel);
    let _ = track_interaction(
        pool,
        &user_id,
        "explore",
        &from,
        &format!("{} {} {}", from[..from.len().min(8)].to_string(), args.rel, to[..to.len().min(8)].to_string()),
        "success",
        duration_ms,
        Some(&context),
    ).await;

    if json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
    } else {
        println!("Linked: {} {} {}", resp.from, resp.rel, resp.to);
        if let Some(ref w) = resp.conflict_warning {
            eprintln!("Warning: {}", w.message);
        }
    }
    Ok(())
}
