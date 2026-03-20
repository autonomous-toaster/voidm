use anyhow::Result;
use std::sync::Arc;
use clap::Args;
use sqlx::SqlitePool;
use voidm_core::{crud, crud_trait, models::{EdgeType, LinkResponse}};
use voidm_db_trait::Database;

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

pub async fn run(args: LinkArgs, db: &Arc<dyn Database>, _pool: &SqlitePool, json: bool) -> Result<()> {
    let edge_type: EdgeType = args.rel.parse()?;
    let from = crud::resolve_id(db.as_ref(), &args.from).await?;
    let to   = crud::resolve_id(db.as_ref(), &args.to).await?;
    let resp_json = crud_trait::link_memories(db, &from, edge_type.as_str(), &to, args.note.as_deref()).await?;
    let resp: LinkResponse = serde_json::from_value(resp_json)?;

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
