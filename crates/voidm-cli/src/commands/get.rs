use anyhow::Result;
use clap::Args;
use sqlx::SqlitePool;
use voidm_core::{crud, crud_trait};

#[derive(Args)]
pub struct GetArgs {
    /// Memory ID or short prefix (min 4 chars)
    pub id: String,
}

pub async fn run(args: GetArgs, db: &std::sync::Arc<dyn voidm_db_trait::Database>, _pool: &sqlx::SqlitePool, json: bool) -> Result<()> {
    let id = match crud::resolve_id(db.as_ref(), &args.id).await {
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
    match crud_trait::get_memory(db, &id).await? {
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
                if let Some(qs) = m.quality_score {
                    println!("Quality:    {:.2}", qs);
                }
                println!("Created:    {}", m.created_at);
                if !m.scopes.is_empty() { println!("Scopes:     {}", m.scopes.join(", ")); }
                if !m.tags.is_empty()   { println!("Tags:       {}", m.tags.join(", ")); }
                
                // Display auto-generated tags if present
                if let Some(serde_json::Value::Array(auto_tags)) = m.metadata.get("auto_generated_tags") {
                    let auto_tag_strs: Vec<String> = auto_tags
                        .iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect();
                    if !auto_tag_strs.is_empty() {
                        println!("Auto-Tags:  {}", auto_tag_strs.join(", "));
                    }
                }
                
                println!();
                println!("{}", m.content);
            }
        }
    }
    Ok(())
}
