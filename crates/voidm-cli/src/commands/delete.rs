use anyhow::Result;
use clap::Args;
use std::sync::Arc;
use voidm_core::crud_trait;
use voidm_db::{Database, ResolveResult};

#[derive(Args)]
pub struct DeleteArgs {
    /// Memory ID (full) or short prefix (min 8 characters)
    /// Examples:
    ///   - Full ID: mem_abcde1234567890abcd (any length, exact match)
    ///   - Prefix: mem_abcde1234 (min 8 chars, deletes all matching)
    pub id: String,
    /// Skip confirmation prompt
    #[arg(long)]
    pub yes: bool,
}

pub async fn run(args: DeleteArgs, db: &Arc<dyn Database>, json: bool) -> Result<()> {
    // Resolve memory ID - may return single or multiple matches
    let resolve_result = match db.resolve_memory_id(&args.id).await {
        Ok(result) => result,
        Err(e) => {
            if json {
                println!("{}", serde_json::json!({ "error": e.to_string(), "id": args.id }));
            } else {
                eprintln!("Error: {}", e);
            }
            std::process::exit(1);
        }
    };

    // Handle both single and bulk deletes
    let ids_to_delete = match resolve_result {
        ResolveResult::Single(id) => vec![id],
        ResolveResult::Multiple(ids) => {
            // For multiple matches, show them if not --yes
            if !args.yes && !json {
                eprintln!("Memory ID '{}' matches {} memories:", args.id, ids.len());
                for id in &ids {
                    eprintln!("  {}", id);
                }
                eprint!("Delete all {} memories? [y/N] ", ids.len());
                let mut input = String::new();
                std::io::stdin().read_line(&mut input)?;
                if !input.trim().eq_ignore_ascii_case("y") {
                    println!("Aborted.");
                    return Ok(());
                }
            }
            ids
        }
    };

    // Delete each memory
    let mut deleted_count = 0;
    let mut errors = Vec::new();
    
    for id in &ids_to_delete {
        match crud_trait::delete_memory(db, id).await {
            Ok(true) => deleted_count += 1,
            Ok(false) => errors.push(format!("'{}' not found", id)),
            Err(e) => errors.push(format!("'{}': {}", id, e)),
        }
    }

    if json {
        if errors.is_empty() {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "result": {
                        "deleted": deleted_count,
                        "count": ids_to_delete.len(),
                        "ids": ids_to_delete
                    }
                }))?
            );
        } else {
            println!(
                "{}",
                serde_json::json!({
                    "result": {
                        "deleted": deleted_count,
                        "count": ids_to_delete.len(),
                        "ids": ids_to_delete
                    },
                    "errors": errors
                })
            );
        }
    } else {
        if ids_to_delete.len() == 1 {
            println!("Deleted memory '{}'", ids_to_delete[0]);
        } else {
            println!("Deleted {} memories matching '{}'", deleted_count, args.id);
        }
        if !errors.is_empty() {
            eprintln!("\nErrors:");
            for err in &errors {
                eprintln!("  {}", err);
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        std::process::exit(1)
    }
}
