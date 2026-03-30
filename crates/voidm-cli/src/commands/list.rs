use anyhow::Result;
use std::sync::Arc;
use clap::Args;
use voidm_core::{Config, crud_trait};
use voidm_db::Database;

#[derive(Args)]
pub struct ListArgs {
    /// Filter by scope prefix
    #[arg(long)]
    pub scope: Option<String>,

    /// Filter by memory type
    #[arg(long, short = 't')]
    pub r#type: Option<String>,

    /// Maximum results
    #[arg(long, default_value = "20")]
    pub limit: usize,
}

pub async fn run(args: ListArgs, db: &Arc<dyn Database>, _config: &Config, json: bool) -> Result<()> {
    let memories = crud_trait::list_memories_filtered(
        db,
        args.scope.as_deref(),
        args.r#type.as_deref(),
        Some(args.limit),
    ).await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&memories)?);
    } else {
        if memories.is_empty() {
            println!("No memories found.");
            return Ok(());
        }
        for m in &memories {
            // Use char_indices to safely truncate at 80 chars (not bytes)
            let preview = if m.content.chars().count() > 80 {
                let truncated: String = m.content.chars().take(80).collect();
                format!("{}...", truncated)
            } else {
                m.content.clone()
            };
            println!("{} [{}] imp:{} {}", m.id, m.memory_type, m.importance, m.created_at);
            println!("  {}", preview);
            if !m.scopes.is_empty() {
                println!("  Scopes: {}", m.scopes.join(", "));
            }
            println!();
        }
    }
    Ok(())
}
