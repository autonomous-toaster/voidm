use anyhow::Result;
use std::sync::Arc;
use clap::Args;
use sqlx::SqlitePool;
use voidm_core::{crud, crud_trait, models::EdgeType};
use voidm_db_trait::Database;

#[derive(Args)]
pub struct UnlinkArgs {
    /// Source memory ID or short prefix
    pub from: String,
    /// Edge type: RELATES_TO, SUPPORTS, CONTRADICTS, DERIVED_FROM, PRECEDES, PART_OF, EXEMPLIFIES, INVALIDATES
    pub rel: String,
    /// Target memory ID or short prefix
    pub to: String,
}

pub async fn run(args: UnlinkArgs, db: &Arc<dyn Database>, _pool: &SqlitePool, json: bool) -> Result<()> {
    let edge_type: EdgeType = args.rel.parse()?;
    let from = crud::resolve_id(db.as_ref(), &args.from).await?;
    let to   = crud::resolve_id(db.as_ref(), &args.to).await?;
    let removed = crud_trait::unlink_memories(db, &from, edge_type.as_str(), &to).await?;

    if removed {
        if json {
            println!("{}", serde_json::json!({ "removed": true, "from": from, "rel": args.rel, "to": to }));
        } else {
            println!("Unlinked: {} {} {}", from, args.rel, to);
        }
    } else {
        if json {
            println!("{}", serde_json::json!({
                "error": format!("Edge not found: {} --[{}]--> {}", from, args.rel, to),
                "from": from, "rel": args.rel, "to": to
            }));
        } else {
            eprintln!("Error: Edge not found: {} --[{}]--> {}", from, args.rel, to);
            eprintln!("Hint: Use 'voidm graph neighbors {}' to see existing edges.", from);
        }
        std::process::exit(1);
    }
    Ok(())
}
