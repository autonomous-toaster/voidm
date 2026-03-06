use anyhow::Result;
use clap::Args;
use sqlx::SqlitePool;
use voidm_core::{crud, resolve_id};

#[derive(Args)]
pub struct GetArgs {
    /// Memory ID or short prefix (min 4 chars)
    pub id: String,
}

pub async fn run(args: GetArgs, pool: &SqlitePool, json: bool) -> Result<()> {
    let id = match resolve_id(pool, &args.id).await {
        Ok(id) => id,
        Err(e) => {
            if json {
                println!("{}", serde_json::json!({ "error": e.to_string(), "id": args.id }));
            } else {
                eprintln!("Error: {}", e);
            }
            std::process::exit(1);
        }
    };
    match crud::get_memory(pool, &id).await? {
        None => {
            if json {
                println!("{}", serde_json::json!({ "error": format!("Memory '{}' not found", id), "id": id }));
            } else {
                eprintln!("Error: Memory '{}' not found", id);
            }
            std::process::exit(1);
        }
        Some(m) => {
            if json {
                println!("{}", serde_json::to_string_pretty(&m)?);
            } else {
                println!("ID:         {}", m.id);
                println!("Type:       {}", m.memory_type);
                println!("Importance: {}", m.importance);
                println!("Created:    {}", m.created_at);
                if !m.scopes.is_empty() { println!("Scopes:     {}", m.scopes.join(", ")); }
                if !m.tags.is_empty()   { println!("Tags:       {}", m.tags.join(", ")); }
                println!();
                println!("{}", m.content);
            }
        }
    }
    Ok(())
}
